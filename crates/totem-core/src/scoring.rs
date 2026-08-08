//! Retrieval ranking: relevance × value × currency, weighted per category
//! (docs/solution-intent.md §4; ADV-CORE-002).
//!
//! Pure functions only — no storage, no clock reads beyond the `elapsed`
//! callers compute themselves. `totem-store` is where these get applied to
//! real records and real writes; keeping the maths here means the ranking
//! rules can be tested without a database.
//!
//! [`docs/tech-direction/value-attribution.md`](../../../docs/tech-direction/value-attribution.md)
//! (ADV-CORE-004) is the investigation this advance implements: lead with the
//! citation signal, do not weight raw retrieval into `value_score` directly
//! (VAL-003), and no half-life for `currency` decay has ever been measured
//! against real usage — [`DEFAULT_CURRENCY_HALF_LIFE`] is a provisional
//! placeholder, not a finding.

use chrono::TimeDelta;

use crate::category::MemoryCategory;

/// How long an unreinforced, decaying memory's `currency` takes to halve.
///
/// No production recall/reinforcement telemetry exists yet (VAL-004/VAL-005
/// in docs/tech-direction/value-attribution.md), so this cannot be a measured
/// value — it is a starting point, tunable once real usage data accumulates.
pub const DEFAULT_CURRENCY_HALF_LIFE: TimeDelta = TimeDelta::days(14);

/// Exponential half-life decay of `currency`, clamped to `[0.0, 1.0]`.
///
/// `elapsed` is clamped to zero before use, so clock skew that makes it
/// negative cannot push currency *above* its starting value — decay only
/// ever holds currency steady or lowers it. A non-positive `half_life`
/// (a caller bug, not a real half-life) is treated the same way: no decay,
/// rather than a division by zero.
pub fn decay_currency(currency: f32, elapsed: TimeDelta, half_life: TimeDelta) -> f32 {
    if half_life <= TimeDelta::zero() {
        return currency.clamp(0.0, 1.0);
    }
    let elapsed_secs = elapsed.num_seconds().max(0) as f64;
    let half_life_secs = half_life.num_seconds() as f64;
    let factor = 0.5_f64.powf(elapsed_secs / half_life_secs);
    (f64::from(currency) * factor).clamp(0.0, 1.0) as f32
}

/// The `currency` a category's own lifecycle says a record should carry right
/// now: unchanged for a category whose lifecycle says it never decays
/// (`docs/solution-intent.md` §2.1 — Episodic, Identity, Instructions,
/// Uncertainty), or [`decay_currency`]'s output for one that does (Knowledge,
/// Context).
///
/// This is a read-time computation only. Nothing here persists a decayed
/// value — the stored `currency` field only ever moves on an actual write
/// (a reinforcement on use resets it; ADV-CURATOR-002, not yet built, is
/// where a scheduled process would eventually persist decay for records that
/// are never reread).
pub fn effective_currency(
    category: MemoryCategory,
    stored_currency: f32,
    elapsed: TimeDelta,
) -> f32 {
    if !category.lifecycle().decays {
        return stored_currency.clamp(0.0, 1.0);
    }
    decay_currency(stored_currency, elapsed, DEFAULT_CURRENCY_HALF_LIFE)
}

/// A category's relative weight in retrieval ranking, compressed from its
/// `injection_priority` into the same bounded range every other
/// non-relevance factor obeys (ADV-CORE-008).
///
/// **What changed and why.** This used to be `injection_priority / 100`,
/// giving Episodic 0.1 and Instructions 1.0 — a **10x range**, against
/// relevance's 2x among records that pass the gate. Category was therefore
/// five times more influential than what the caller actually asked, and an
/// unrelated `Instructions` memory beat an exact match on a `Knowledge` one.
/// Measured on the deployment: relevance 0.548 x weight 1.0 = 0.548 for the
/// unrelated record, against 1.0 x 0.863 x 0.5 = 0.432 for the exact match.
///
/// **The deeper mistake was reusing the wrong number.** `injection_priority`
/// answers *"if I am assembling a context window, what goes in first?"* —
/// a question about a budget, where a 10x spread is reasonable. Retrieval
/// ranking asks *"which of these did they mean?"*, and the priority order is
/// worth keeping there while its magnitude is not.
///
/// So the order is preserved exactly and only the span is compressed, onto
/// `[1/(1+gate), 1.0]` — the same range relevance itself spans among records
/// that compete. Instructions still outranks Knowledge at equal relevance;
/// it can no longer outrank a materially better match.
///
/// This is one rule applied uniformly: **no non-relevance factor may outweigh
/// relevance's own range.** `value_score` obeys it via
/// [`saturating_value`], `currency` is already `[0, 1]`, and this was the
/// term still exempt.
pub fn category_weight(category: MemoryCategory) -> f32 {
    // `injection_priority` is documented as 10..=100 across the six
    // categories; normalize within that observed span rather than 0..=100, so
    // the lowest category maps to the floor rather than somewhere above it.
    const LOWEST_PRIORITY: f32 = 10.0;
    const HIGHEST_PRIORITY: f32 = 100.0;
    let floor = 1.0 / (1.0 + RELEVANCE_GATE_DISTANCE as f32);
    let position = (f32::from(category.lifecycle().injection_priority) - LOWEST_PRIORITY)
        / (HIGHEST_PRIORITY - LOWEST_PRIORITY);
    floor + position.clamp(0.0, 1.0) * (1.0 - floor)
}

/// Cosine distance beyond which a record does not compete at all, whatever
/// its history (ADV-CORE-008).
///
/// **Why 1.0, and not a number tuned until the tests passed.** The store's
/// index is `DIST COSINE`, so distance is `1 - cos θ` over `[0, 2]`, and
/// **1.0 is exactly orthogonality**: a record at this distance shares no
/// direction with the query at all. Above it the two are *negatively*
/// correlated. So the gate is not a tuning parameter but a statement with a
/// meaning — a record must be positively similar to what was asked, not
/// merely less dissimilar than its neighbours.
///
/// That distinction is the whole defect. Before this gate, a record could
/// place first while being unrelated to the query, purely on
/// `category_weight` and `currency`: an `Instructions` memory carries a 2.3x
/// structural advantage over a `Knowledge` one before any query is asked,
/// and relevance's entire range is only 3x, so relevance could not always
/// overcome it.
pub const RELEVANCE_GATE_DISTANCE: f64 = 1.0;

/// The most a memory's accumulated history may multiply its score
/// (ADV-CORE-008).
///
/// **Derived from the gate, not chosen.** The claim is
/// *history may matter as much as relevance, and never more* — so the ceiling
/// must equal relevance's range **among records that actually compete**.
///
/// The gate changes that range, which is easy to get wrong: relevance spans
/// `[0.33, 1.0]` (3x) over all distances, but a record past
/// [`RELEVANCE_GATE_DISTANCE`] scores zero and is dropped, so among survivors
/// it spans `[1/(1+gate), 1.0]`. That ratio is exactly `1 + gate`:
///
/// ```text
/// relevance(0) / relevance(gate) = 1 / (1/(1+gate)) = 1 + gate = 2.0
/// ```
///
/// Writing it as a derivation rather than a literal means the two constants
/// cannot drift apart: move the gate and the ceiling follows, still honouring
/// the same claim. A hand-picked 3.0 would have quietly let history outweigh
/// relevance by 1.5x — the very failure this advance exists to fix, smaller
/// and harder to see.
pub const VALUE_SATURATION_CEILING: f32 = 1.0 + RELEVANCE_GATE_DISTANCE as f32;

/// Bound `value_score`'s contribution so it cannot grow without limit.
///
/// `CITATION_BOOST` adds 0.2 per citation with no ceiling, so a raw
/// `value_score` is unbounded and would eventually dominate every other term
/// — the same class of failure as the one this advance fixes, arriving later
/// through a different door. **This is prophylactic, not curative:**
/// `value_score` is 1.0 on every record in the estate today, so this changes
/// no present ranking. It is here so it does not have to be added in a panic.
///
/// The curve is `k·v / (k − 1 + v)`, chosen so that `v = 1` maps to exactly
/// `1.0` — the default every record starts at is untouched, and this advance
/// therefore cannot silently rescale the whole corpus — while `v → ∞`
/// approaches [`VALUE_SATURATION_CEILING`].
///
/// Negative input (not reachable through the store, which floors at zero, but
/// not a type-level impossibility) is treated as zero.
pub fn saturating_value(value_score: f32) -> f32 {
    let value = value_score.max(0.0);
    if value == 0.0 {
        return 0.0;
    }
    let k = VALUE_SATURATION_CEILING;
    // `k / (1 + (k-1)/v)`, algebraically identical to `k·v / (k-1+v)` but
    // stable at the top of the range: the direct form multiplies before
    // dividing, so `k · f32::MAX` overflows to infinity and the bound this
    // function exists to enforce silently disappears. Found by the test that
    // asserts the bound holds at `f32::MAX`.
    k / (1.0 + (k - 1.0) / value)
}

/// Convert a vector-search distance into a `[0, 1]` closeness term, or a
/// neutral `1.0` when the recall carried no probe at all — ranking then
/// depends only on value and currency, not vector proximity
/// (docs/solution-intent.md §4).
///
/// Returns **exactly zero** past [`RELEVANCE_GATE_DISTANCE`], which
/// [`combined_score`] already turns into a zero score: the gate needs no
/// special case downstream, because "zero in any one factor zeroes the whole
/// score" was already this module's rule (ADV-CORE-008).
///
/// A negative distance (never expected from SurrealDB's `knn_distance`, but
/// not a type-level impossibility) is treated as zero rather than amplifying
/// relevance past 1.0.
pub fn relevance_from_distance(distance: Option<f64>) -> f32 {
    match distance {
        Some(distance) if distance > RELEVANCE_GATE_DISTANCE => 0.0,
        Some(distance) => (1.0 / (1.0 + distance.max(0.0))) as f32,
        None => 1.0,
    }
}

/// Retrieval ranking = relevance × value × currency, weighted per category
/// (docs/solution-intent.md §4). Zero in any one factor zeroes the whole
/// score — a retired-in-all-but-name record (`value_score` driven to zero)
/// should not out-ride a lucky relevance match.
pub fn combined_score(
    relevance: f32,
    value_score: f32,
    currency: f32,
    category_weight: f32,
) -> f32 {
    relevance * value_score * currency * category_weight
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ADV-CORE-008 ──────────────────────────────────────────────────────
    //
    // The converse test is written FIRST and deliberately, because the risk
    // in this advance is not failing to fix the bug — it is over-correcting
    // Totem into a plain vector index. A memory that has proven useful must
    // still win against one that has not, when relevance is comparable. That
    // is much of what Totem is for (docs/solution-intent.md §4), and it is
    // the property most easily destroyed by a gate.

    #[test]
    fn a_well_used_memory_still_beats_an_unused_one_at_comparable_relevance() {
        // Same category, same closeness; only history differs.
        let close = Some(0.20);
        let well_used = combined_score(
            relevance_from_distance(close),
            saturating_value(3.0),
            1.0,
            category_weight(MemoryCategory::Knowledge),
        );
        let unused = combined_score(
            relevance_from_distance(close),
            saturating_value(1.0),
            1.0,
            category_weight(MemoryCategory::Knowledge),
        );
        assert!(
            well_used > unused,
            "the value loop must survive this advance: {well_used} vs {unused}"
        );
    }

    #[test]
    fn category_priority_order_survives_the_compression() {
        // The span is compressed; the ORDER is not touched. If this advance
        // reordered the categories it would have changed what Totem
        // prioritises, which is a §2.1 decision and not its to make.
        use MemoryCategory::*;
        let ordered = [
            Episodic,
            Knowledge,
            Identity,
            Uncertainty,
            Context,
            Instructions,
        ];
        for pair in ordered.windows(2) {
            assert!(
                category_weight(pair[0]) < category_weight(pair[1]),
                "{:?} must still rank below {:?}",
                pair[0],
                pair[1]
            );
        }
        assert_eq!(
            category_weight(Instructions),
            1.0,
            "the top is still the top"
        );
    }

    #[test]
    fn an_instructions_memory_still_wins_at_equal_relevance() {
        // The value loop's sibling: category priority must still count for
        // something. Same distance, same history — only the category differs.
        let close = Some(0.2);
        let instructions = combined_score(
            relevance_from_distance(close),
            1.0,
            1.0,
            category_weight(MemoryCategory::Instructions),
        );
        let knowledge = combined_score(
            relevance_from_distance(close),
            1.0,
            1.0,
            category_weight(MemoryCategory::Knowledge),
        );
        assert!(
            instructions > knowledge,
            "category priority must survive: {instructions} vs {knowledge}"
        );
    }

    #[test]
    fn no_non_relevance_factor_outweighs_relevance_s_own_range() {
        // The single rule this advance applies uniformly. If a new factor is
        // ever added to `combined_score` without obeying it, this is the test
        // that should be extended — and the failure it prevents is the one
        // that shipped: something other than the question deciding the answer.
        let relevance_range = relevance_from_distance(Some(0.0))
            / relevance_from_distance(Some(RELEVANCE_GATE_DISTANCE));

        let category_range = category_weight(MemoryCategory::Instructions)
            / category_weight(MemoryCategory::Episodic);
        assert!(
            category_range <= relevance_range + f32::EPSILON,
            "category spans {category_range}x against relevance's {relevance_range}x"
        );

        let value_range = saturating_value(f32::MAX) / saturating_value(1.0);
        assert!(value_range <= relevance_range + f32::EPSILON);

        // currency is `[0, 1]` by construction, so its range is bounded by 1.
    }

    #[test]
    fn a_fresher_memory_still_beats_a_decayed_one_at_comparable_relevance() {
        let close = Some(0.20);
        let fresh = combined_score(relevance_from_distance(close), 1.0, 1.0, 0.5);
        let stale = combined_score(relevance_from_distance(close), 1.0, 0.4, 0.5);
        assert!(
            fresh > stale,
            "currency must still count: {fresh} vs {stale}"
        );
    }

    #[test]
    fn decay_at_zero_elapsed_leaves_currency_unchanged() {
        assert_eq!(
            decay_currency(1.0, TimeDelta::zero(), DEFAULT_CURRENCY_HALF_LIFE),
            1.0
        );
        assert_eq!(
            decay_currency(0.4, TimeDelta::zero(), DEFAULT_CURRENCY_HALF_LIFE),
            0.4
        );
    }

    #[test]
    fn decay_halves_currency_at_exactly_one_half_life() {
        let half_life = TimeDelta::days(14);
        let decayed = decay_currency(1.0, half_life, half_life);
        assert!(
            (decayed - 0.5).abs() < 1e-4,
            "expected ~0.5 at one half-life, got {decayed}",
        );
    }

    #[test]
    fn decay_is_monotonically_non_increasing_with_elapsed_time() {
        let samples = [0, 1, 2, 7, 14, 30, 90, 365];
        let mut previous = f32::MAX;
        for days in samples {
            let decayed = decay_currency(1.0, TimeDelta::days(days), DEFAULT_CURRENCY_HALF_LIFE);
            assert!(
                decayed <= previous,
                "currency rose from {previous} to {decayed} after {days} days",
            );
            assert!((0.0..=1.0).contains(&decayed), "{decayed} left [0, 1]");
            previous = decayed;
        }
    }

    #[test]
    fn decay_never_amplifies_currency_above_its_starting_value() {
        for elapsed_days in [0, 5, 50] {
            let decayed = decay_currency(
                0.7,
                TimeDelta::days(elapsed_days),
                DEFAULT_CURRENCY_HALF_LIFE,
            );
            assert!(
                decayed <= 0.7,
                "{decayed} exceeds the starting currency 0.7"
            );
        }
    }

    #[test]
    fn decay_treats_negative_elapsed_as_zero_rather_than_amplifying() {
        assert_eq!(
            decay_currency(0.8, TimeDelta::days(-5), DEFAULT_CURRENCY_HALF_LIFE),
            0.8,
        );
    }

    #[test]
    fn decay_ignores_a_non_positive_half_life_instead_of_dividing_by_zero() {
        assert_eq!(
            decay_currency(0.6, TimeDelta::days(10), TimeDelta::zero()),
            0.6
        );
    }

    #[test]
    fn effective_currency_never_decays_for_a_lifecycle_that_says_it_does_not() {
        for category in [
            MemoryCategory::Episodic,
            MemoryCategory::Identity,
            MemoryCategory::Instructions,
            MemoryCategory::Uncertainty,
        ] {
            assert!(
                !category.lifecycle().decays,
                "test assumption broken for {category:?}",
            );
            let unchanged = effective_currency(category, 0.9, TimeDelta::days(365));
            assert_eq!(unchanged, 0.9, "{category:?} decayed despite decays: false");
        }
    }

    #[test]
    fn effective_currency_decays_for_a_lifecycle_that_says_it_does() {
        for category in [MemoryCategory::Knowledge, MemoryCategory::Context] {
            assert!(
                category.lifecycle().decays,
                "test assumption broken for {category:?}",
            );
            let matches_decay_currency = effective_currency(category, 1.0, TimeDelta::days(30));
            let expected = decay_currency(1.0, TimeDelta::days(30), DEFAULT_CURRENCY_HALF_LIFE);
            assert_eq!(matches_decay_currency, expected);
            assert!(matches_decay_currency < 1.0, "{category:?} did not decay");
        }
    }

    #[test]
    fn category_weight_ranks_categories_in_their_lifecycles_own_priority_order() {
        // docs/solution-intent.md §2.1: Instructions highest, Episodic lowest.
        let instructions = category_weight(MemoryCategory::Instructions);
        let context = category_weight(MemoryCategory::Context);
        let uncertainty = category_weight(MemoryCategory::Uncertainty);
        let identity = category_weight(MemoryCategory::Identity);
        let knowledge = category_weight(MemoryCategory::Knowledge);
        let episodic = category_weight(MemoryCategory::Episodic);

        assert!(instructions > context);
        assert!(context > uncertainty);
        assert!(uncertainty > identity);
        assert!(identity > knowledge);
        assert!(knowledge > episodic);

        for category in MemoryCategory::ALL {
            let weight = category_weight(category);
            assert!(
                weight > 0.0 && weight <= 1.0,
                "{category:?}'s weight {weight} left (0, 1]",
            );
        }
    }

    #[test]
    fn relevance_with_no_probe_is_neutral() {
        assert_eq!(relevance_from_distance(None), 1.0);
    }

    #[test]
    fn relevance_at_zero_distance_is_maximal() {
        assert_eq!(relevance_from_distance(Some(0.0)), 1.0);
    }

    #[test]
    fn relevance_decreases_monotonically_as_distance_grows() {
        // The range is `[0, 1]`, not `(0, 1]`: ADV-CORE-008 made relevance
        // exactly zero past `RELEVANCE_GATE_DISTANCE`. This assertion
        // previously read `relevance > 0.0`, which was the correct contract
        // before the gate existed and is deliberately changed here — a
        // record beyond orthogonality is meant to score zero and be dropped.
        let samples = [0.0, 0.1, 0.5, 1.0, 2.0, 10.0];
        let mut previous = f32::MAX;
        for distance in samples {
            let relevance = relevance_from_distance(Some(distance));
            assert!(
                relevance <= previous,
                "relevance rose from {previous} to {relevance} at distance {distance}",
            );
            assert!(
                (0.0..=1.0).contains(&relevance),
                "{relevance} left [0, 1] at distance {distance}",
            );
            previous = relevance;
        }
    }

    #[test]
    fn the_gate_is_orthogonality_and_zeroes_the_whole_score() {
        // At the gate exactly, a record still competes; past it, not at all.
        assert!(relevance_from_distance(Some(RELEVANCE_GATE_DISTANCE)) > 0.0);
        assert_eq!(
            relevance_from_distance(Some(RELEVANCE_GATE_DISTANCE + 0.01)),
            0.0
        );

        // And the zero propagates: no accumulated history rescues it. This is
        // the defect ADV-CORE-008 exists to fix, stated at the unit level.
        let unrelated_but_privileged = combined_score(
            relevance_from_distance(Some(1.5)),
            saturating_value(1000.0),
            1.0,
            category_weight(MemoryCategory::Instructions),
        );
        assert_eq!(
            unrelated_but_privileged, 0.0,
            "a record past orthogonality must not compete, whatever its history"
        );
    }

    #[test]
    fn saturating_value_leaves_the_default_untouched_and_bounds_the_rest() {
        // v = 1 is what every record starts at; mapping it anywhere but 1.0
        // would silently rescale the entire corpus.
        assert_eq!(saturating_value(1.0), 1.0);

        assert!(
            saturating_value(2.0) > saturating_value(1.0),
            "still ordered"
        );
        assert!(
            saturating_value(100.0) > saturating_value(10.0),
            "still ordered"
        );

        // Bounded by the ceiling, approached asymptotically rather than
        // clamped, so ordering survives far out. (In f32 the approach reaches
        // the ceiling exactly at large inputs; the property that matters is
        // that it never exceeds it.)
        assert!(saturating_value(1.0e9) <= VALUE_SATURATION_CEILING);
        assert!(saturating_value(1.0e9) > VALUE_SATURATION_CEILING - 0.01);
        assert!(
            saturating_value(1.0e4) < saturating_value(1.0e5),
            "still ordered far out"
        );
    }

    #[test]
    fn history_can_never_outweigh_relevance_by_more_than_relevance_s_own_range() {
        // The falsifiable claim behind VALUE_SATURATION_CEILING = 3.0:
        // history may matter as much as relevance, and never more.
        // Relevance's range among records that COMPETE — records past the
        // gate score zero and are dropped, so the full [0.33, 1.0] span is
        // not what history has to be measured against.
        let relevance_range = relevance_from_distance(Some(0.0))
            / relevance_from_distance(Some(RELEVANCE_GATE_DISTANCE));
        let value_range = saturating_value(f32::MAX) / saturating_value(1.0);
        assert!(
            value_range <= relevance_range + f32::EPSILON,
            "history spans {value_range}x but relevance only {relevance_range}x —              the larger term wins in the limit, which is this advance's defect"
        );
    }

    #[test]
    fn saturating_value_treats_a_negative_score_as_zero() {
        assert_eq!(saturating_value(-5.0), 0.0);
    }

    #[test]
    fn relevance_treats_a_negative_distance_as_zero() {
        assert_eq!(
            relevance_from_distance(Some(-3.0)),
            relevance_from_distance(Some(0.0))
        );
    }

    #[test]
    fn combined_score_is_the_product_of_all_four_factors() {
        assert_eq!(combined_score(0.5, 2.0, 0.5, 1.0), 0.5);
        assert_eq!(combined_score(1.0, 1.0, 1.0, 1.0), 1.0);
    }

    #[test]
    fn combined_score_is_zero_when_any_single_factor_is_zero() {
        assert_eq!(combined_score(0.0, 1.0, 1.0, 1.0), 0.0);
        assert_eq!(combined_score(1.0, 0.0, 1.0, 1.0), 0.0);
        assert_eq!(combined_score(1.0, 1.0, 0.0, 1.0), 0.0);
        assert_eq!(combined_score(1.0, 1.0, 1.0, 0.0), 0.0);
    }

    #[test]
    fn combined_score_is_monotonically_increasing_in_each_positive_factor() {
        let base = combined_score(0.3, 0.3, 0.3, 0.3);
        assert!(combined_score(0.6, 0.3, 0.3, 0.3) > base, "relevance");
        assert!(combined_score(0.3, 0.6, 0.3, 0.3) > base, "value_score");
        assert!(combined_score(0.3, 0.3, 0.6, 0.3) > base, "currency");
        assert!(combined_score(0.3, 0.3, 0.3, 0.6) > base, "category_weight");
    }
}
