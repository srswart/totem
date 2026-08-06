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

/// A category's relative weight in retrieval ranking, normalized from its
/// `injection_priority` (already the per-category weight the lifecycle
/// carries — Instructions highest, Episodic lowest;
/// `docs/solution-intent.md` §2.1/§4).
pub fn category_weight(category: MemoryCategory) -> f32 {
    f32::from(category.lifecycle().injection_priority) / 100.0
}

/// Convert a vector-search distance into a `(0, 1]` closeness term, or a
/// neutral `1.0` when the recall carried no probe at all — ranking then
/// depends only on value and currency, not vector proximity
/// (docs/solution-intent.md §4).
///
/// A negative distance (never expected from SurrealDB's `knn_distance`, but
/// not a type-level impossibility) is treated as zero rather than amplifying
/// relevance past 1.0.
pub fn relevance_from_distance(distance: Option<f64>) -> f32 {
    match distance {
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
        let samples = [0.0, 0.1, 0.5, 1.0, 2.0, 10.0];
        let mut previous = f32::MAX;
        for distance in samples {
            let relevance = relevance_from_distance(Some(distance));
            assert!(
                relevance <= previous,
                "relevance rose from {previous} to {relevance} at distance {distance}",
            );
            assert!(
                relevance > 0.0 && relevance <= 1.0,
                "{relevance} left (0, 1] at distance {distance}",
            );
            previous = relevance;
        }
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
