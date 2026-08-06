//! Explicit value feedback: the input side of the value loop
//! (docs/solution-intent.md §4; ADV-GATEWAY-004 gap-fill).
//!
//! ADV-CORE-002's ranking already reinforces a record on every recall
//! (`totem-store`'s `reinforce_usage`) and boosts a cited source's
//! `value_score` on save (the `derived_from` citation boost) — both automatic
//! signals. Neither captures an agent or human *explicitly* saying a memory
//! was used, wrong, or stale — the zero-data-point gap
//! [docs/tech-direction/value-attribution.md](../../../docs/tech-direction/value-attribution.md)
//! (VAL-005) names. This module is the domain vocabulary for that signal;
//! `totem-store` is where it gets applied to a real record (the same split
//! `scoring.rs` draws for the automatic signals).
//!
//! Pure functions only, same as `scoring.rs` — no storage, no clock reads.

use crate::record::Economics;
use serde::{Deserialize, Serialize};

/// An explicit signal about whether a memory still holds up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackSignal {
    /// The memory was retrieved and held up.
    Used,
    /// The memory is incorrect.
    Wrong,
    /// The memory is stale, though not necessarily incorrect.
    Stale,
}

/// How much a `used` signal raises `value_score`.
///
/// One flat constant, not per-category tuning — the same choice
/// `memory.rs`'s `CITATION_BOOST` makes, for the same reason: no measured
/// per-category weighting exists yet (VAL-005 has zero explicit-feedback data
/// points), so a single provisional constant is honest and a fabricated
/// per-category table would not be.
pub const USED_BOOST: f32 = 0.1;

/// How much a `wrong` signal lowers `value_score`, floored at `0.0` so
/// repeated negative feedback cannot drive it negative and invert ranking's
/// sort order (`scoring::combined_score` multiplies all four factors
/// together).
pub const WRONG_PENALTY: f32 = 0.3;

/// Apply an explicit feedback signal to a record's economics, returning the
/// updated value.
///
/// `used` and `wrong` move `value_score`; `stale` moves `currency` to `0.0`
/// instead, since a report that a memory is out of date is a currency claim,
/// not a correctness claim — `effective_currency` (`scoring.rs`) will let it
/// recover on the next reinforced use, same as any other decayed record.
pub fn apply_feedback(economics: &Economics, signal: FeedbackSignal) -> Economics {
    let mut next = economics.clone();
    match signal {
        FeedbackSignal::Used => next.value_score = (next.value_score + USED_BOOST).max(0.0),
        FeedbackSignal::Wrong => next.value_score = (next.value_score - WRONG_PENALTY).max(0.0),
        FeedbackSignal::Stale => next.currency = 0.0,
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> Economics {
        Economics::fresh()
    }

    #[test]
    fn used_raises_value_score() {
        let next = apply_feedback(&fresh(), FeedbackSignal::Used);
        assert!(next.value_score > fresh().value_score);
        assert_eq!(next.currency, fresh().currency);
    }

    #[test]
    fn wrong_lowers_value_score() {
        let next = apply_feedback(&fresh(), FeedbackSignal::Wrong);
        assert!(next.value_score < fresh().value_score);
        assert_eq!(next.currency, fresh().currency);
    }

    #[test]
    fn repeated_wrong_signals_floor_value_score_at_zero_rather_than_go_negative() {
        let mut economics = fresh();
        for _ in 0..20 {
            economics = apply_feedback(&economics, FeedbackSignal::Wrong);
        }
        assert_eq!(economics.value_score, 0.0);
    }

    #[test]
    fn stale_zeroes_currency_but_leaves_value_score_untouched() {
        let next = apply_feedback(&fresh(), FeedbackSignal::Stale);
        assert_eq!(next.currency, 0.0);
        assert_eq!(next.value_score, fresh().value_score);
    }

    #[test]
    fn every_signal_round_trips_through_json() {
        for signal in [
            FeedbackSignal::Used,
            FeedbackSignal::Wrong,
            FeedbackSignal::Stale,
        ] {
            let json = serde_json::to_string(&signal).expect("serialises");
            let back: FeedbackSignal = serde_json::from_str(&json).expect("deserialises");
            assert_eq!(back, signal);
        }
    }
}
