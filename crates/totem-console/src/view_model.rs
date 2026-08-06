//! View models the console renders: parsed, target-agnostic shapes for the
//! gateway's `GET /landscape/:repo` and `POST /recall` responses
//! (docs/solution-intent.md §5, G2).
//!
//! Deliberately does not depend on `totem-store` (its `LandscapeView` has no
//! `Deserialize`, and pulls in `surrealdb`, which this wasm32-targeted crate
//! must not) or `totem-gateway` (server-only deps: axum, rmcp, surrealdb).
//! Memory records reuse [`totem_core::MemoryRecord`] directly, since that
//! crate is already the shared, dependency-light contract both sides parse.

use std::collections::BTreeMap;

use serde::Deserialize;
use totem_core::{AccessLogEntry, CurationEvent, MemoryCategory, MemoryRecord, PromotionEvent};

/// Why a gateway response body could not be turned into a view model.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ViewModelError {
    /// The body was not the JSON shape expected.
    #[error("parsing {what}: {detail}")]
    Json {
        /// What was being parsed (`"landscape"`, `"recall"`).
        what: &'static str,
        /// The underlying parse failure, as text — `serde_json::Error` is
        /// not `Clone`/`Eq`, so it cannot be stored directly (and a field
        /// literally named `source` would make thiserror treat it as
        /// `#[source]`, which also requires `std::error::Error`).
        detail: String,
    },
}

/// One repo, mirroring `totem_store::landscape::RepoView`'s wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepoView {
    /// The repo id.
    pub id: String,
    /// Its display name.
    pub name: String,
}

/// One system, mirroring `totem_store::landscape::SystemView`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SystemView {
    /// The system id.
    pub id: String,
    /// Its display name.
    pub name: String,
}

/// One component, mirroring `totem_store::landscape::ComponentView`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ComponentView {
    /// The component's short id.
    pub id: String,
    /// The system it belongs to.
    pub system: String,
    /// Its display name.
    pub name: String,
    /// Its lifecycle stage, if set.
    pub stage: Option<String>,
    /// The plain names of every owner currently related to it.
    pub owners: Vec<String>,
}

/// One advance, mirroring `totem_store::landscape::AdvanceView`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AdvanceView {
    /// The advance id.
    pub id: String,
    /// The system it belongs to.
    pub system: String,
    /// Its title.
    pub title: String,
    /// Its frontmatter status, if set.
    pub status: Option<String>,
    /// The short component ids it currently impacts.
    pub components: Vec<String>,
}

/// The merged landscape view for one repo, as `GET /landscape/:repo` returns
/// it — the landscape dashboard's read model.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct LandscapeViewModel {
    /// The repo, if it has ever been synced.
    pub repo: Option<RepoView>,
    /// Every system belonging to it.
    #[serde(default)]
    pub systems: Vec<SystemView>,
    /// Every component belonging to it.
    #[serde(default)]
    pub components: Vec<ComponentView>,
    /// Every advance belonging to it.
    #[serde(default)]
    pub advances: Vec<AdvanceView>,
}

/// Parse `GET /landscape/:repo`'s response body.
pub fn parse_landscape(body: &str) -> Result<LandscapeViewModel, ViewModelError> {
    serde_json::from_str(body).map_err(|error| ViewModelError::Json {
        what: "landscape",
        detail: error.to_string(),
    })
}

/// `POST /recall`'s response body: the merged, scope-resolved records
/// (mirrors `totem_gateway::RecallResponse`'s wire shape).
#[derive(Debug, Clone, Deserialize)]
struct RecallResponseModel {
    records: Vec<MemoryRecord>,
}

/// Parse `POST /recall`'s response body into its records.
pub fn parse_memories(body: &str) -> Result<Vec<MemoryRecord>, ViewModelError> {
    serde_json::from_str::<RecallResponseModel>(body)
        .map(|response| response.records)
        .map_err(|error| ViewModelError::Json {
            what: "recall",
            detail: error.to_string(),
        })
}

/// `POST /promotions/pending`'s response body: open proposals aimed at a
/// scope the reader can reach (ADV-CONSOLE-002's promotion approval queue).
#[derive(Debug, Clone, Deserialize)]
struct PromotionQueueResponseModel {
    pending: Vec<PromotionEvent>,
}

/// Parse `POST /promotions/pending`'s response body into its queued
/// proposals.
pub fn parse_promotion_queue(body: &str) -> Result<Vec<PromotionEvent>, ViewModelError> {
    serde_json::from_str::<PromotionQueueResponseModel>(body)
        .map(|response| response.pending)
        .map_err(|error| ViewModelError::Json {
            what: "promotion queue",
            detail: error.to_string(),
        })
}

/// `POST /promotions/:id/record`'s response body: the record a queued
/// proposal names, or `None` once it is decided.
#[derive(Debug, Clone, Deserialize)]
struct ProposedRecordResponseModel {
    record: Option<MemoryRecord>,
}

/// Parse `POST /promotions/:id/record`'s response body.
pub fn parse_proposed_record(body: &str) -> Result<Option<MemoryRecord>, ViewModelError> {
    serde_json::from_str::<ProposedRecordResponseModel>(body)
        .map(|response| response.record)
        .map_err(|error| ViewModelError::Json {
            what: "proposed record",
            detail: error.to_string(),
        })
}

/// `POST /uncertainty/pending`'s response body: Uncertainty records awaiting
/// a human decision (ADV-CONSOLE-002's Uncertainty queue).
#[derive(Debug, Clone, Deserialize)]
struct UncertaintyQueueResponseModel {
    pending: Vec<MemoryRecord>,
}

/// Parse `POST /uncertainty/pending`'s response body into its queued records.
pub fn parse_uncertainty_queue(body: &str) -> Result<Vec<MemoryRecord>, ViewModelError> {
    serde_json::from_str::<UncertaintyQueueResponseModel>(body)
        .map(|response| response.pending)
        .map_err(|error| ViewModelError::Json {
            what: "uncertainty queue",
            detail: error.to_string(),
        })
}

/// `POST /audit/:id`'s response body: one memory's full audit trail
/// (ADV-CONSOLE-002) — its provenance (via [`MemoryRecord::provenance`]),
/// access history, curator lineage, and promotion history.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AuditTrailViewModel {
    /// The record itself.
    pub record: MemoryRecord,
    /// Every logged read or write naming this record.
    pub access_log: Vec<AccessLogEntry>,
    /// The merges (and rollbacks) this record took part in.
    pub curation_history: Vec<CurationEvent>,
    /// This record's whole scope history.
    pub promotion_history: Vec<PromotionEvent>,
}

/// Parse `POST /audit/:id`'s response body.
pub fn parse_audit_trail(body: &str) -> Result<AuditTrailViewModel, ViewModelError> {
    serde_json::from_str(body).map_err(|error| ViewModelError::Json {
        what: "audit trail",
        detail: error.to_string(),
    })
}

/// Group memories by category, in category's declared (`Ord`) order — the
/// memory browser's primary grouping (Solution Intent §5: "browse memories
/// by scope and category").
pub fn group_by_category(records: &[MemoryRecord]) -> BTreeMap<MemoryCategory, Vec<&MemoryRecord>> {
    let mut grouped: BTreeMap<MemoryCategory, Vec<&MemoryRecord>> = BTreeMap::new();
    for record in records {
        grouped.entry(record.category).or_default().push(record);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use totem_core::{ActorId, Author, Content, Harness, Provenance, Scope, SessionId};

    use super::*;

    fn landscape_json() -> &'static str {
        r#"{
            "repo": { "id": "058-totem", "name": "Totem" },
            "systems": [{ "id": "058-totem-core", "name": "Totem Core" }],
            "components": [
                {
                    "id": "console",
                    "system": "058-totem-core",
                    "name": "Totem Console",
                    "stage": "incubating",
                    "owners": ["058-totem"]
                }
            ],
            "advances": [
                {
                    "id": "ADV-CONSOLE-001",
                    "system": "058-totem-core",
                    "title": "Landscape dashboard + memory browser",
                    "status": "in_progress",
                    "components": ["console"]
                }
            ]
        }"#
    }

    #[test]
    fn a_synced_landscape_parses_into_its_view_model() {
        let view = parse_landscape(landscape_json()).expect("valid landscape JSON parses");

        assert_eq!(view.repo.as_ref().expect("repo present").id, "058-totem");
        assert_eq!(view.systems.len(), 1);
        assert_eq!(view.components[0].id, "console");
        assert_eq!(view.components[0].stage.as_deref(), Some("incubating"));
        assert_eq!(view.advances[0].id, "ADV-CONSOLE-001");
        assert_eq!(view.advances[0].components, vec!["console".to_string()]);
    }

    #[test]
    fn an_unsynced_repo_s_null_landscape_parses_as_empty_not_an_error() {
        let view =
            parse_landscape(r#"{"repo": null, "systems": [], "components": [], "advances": []}"#)
                .expect("a null repo still parses");

        assert!(view.repo.is_none());
        assert!(view.systems.is_empty());
    }

    #[test]
    fn malformed_landscape_json_reports_what_failed_to_parse() {
        let error = parse_landscape("not json").expect_err("garbage does not parse");
        assert!(matches!(
            error,
            ViewModelError::Json {
                what: "landscape",
                ..
            }
        ));
    }

    fn a_memory_record(category: MemoryCategory, scope: Scope, body: &str) -> MemoryRecord {
        MemoryRecord::new(
            category,
            scope,
            Content::new(body),
            Provenance::new(
                Author::Human(ActorId::new("ada").expect("valid actor id")),
                Harness::Console,
                SessionId::new("sess-1").expect("valid session id"),
                Utc::now(),
            ),
        )
    }

    #[test]
    fn a_recall_response_round_trips_through_the_view_model() {
        let record = a_memory_record(
            MemoryCategory::Knowledge,
            Scope::Actor(ActorId::new("ada").expect("valid actor id")),
            "the store enforces scope isolation",
        );
        let body = serde_json::json!({ "records": [record] }).to_string();

        let parsed = parse_memories(&body).expect("valid recall JSON parses");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], record);
    }

    #[test]
    fn malformed_recall_json_reports_what_failed_to_parse() {
        let error = parse_memories("not json").expect_err("garbage does not parse");
        assert!(matches!(error, ViewModelError::Json { what: "recall", .. }));
    }

    #[test]
    fn memories_group_by_category_in_category_order() {
        let actor_scope = || Scope::Actor(ActorId::new("ada").expect("valid actor id"));
        let records = vec![
            a_memory_record(MemoryCategory::Instructions, actor_scope(), "standing rule"),
            a_memory_record(MemoryCategory::Episodic, actor_scope(), "turn 1"),
            a_memory_record(MemoryCategory::Episodic, actor_scope(), "turn 2"),
        ];

        let grouped = group_by_category(&records);
        let categories: Vec<MemoryCategory> = grouped.keys().copied().collect();

        assert_eq!(
            categories,
            vec![MemoryCategory::Episodic, MemoryCategory::Instructions],
            "BTreeMap orders by MemoryCategory's declared Ord (Episodic before Instructions)"
        );
        assert_eq!(grouped[&MemoryCategory::Episodic].len(), 2);
        assert_eq!(grouped[&MemoryCategory::Instructions].len(), 1);
    }

    fn a_promotion_event() -> totem_core::PromotionEvent {
        totem_core::PromotionEvent::propose(
            totem_core::MemoryId::new(),
            Scope::Actor(ActorId::new("ada").expect("valid actor id")),
            Scope::Project(totem_core::RepoId::new("srswart/totem").expect("valid repo id")),
            Provenance::new(
                Author::Human(ActorId::new("ada").expect("valid actor id")),
                Harness::Console,
                SessionId::new("sess-1").expect("valid session id"),
                Utc::now(),
            ),
        )
    }

    #[test]
    fn a_promotion_queue_response_parses_into_its_events() {
        let proposal = a_promotion_event();
        let body = serde_json::json!({ "pending": [proposal] }).to_string();

        let parsed = parse_promotion_queue(&body).expect("valid queue JSON parses");

        assert_eq!(parsed, vec![proposal]);
    }

    #[test]
    fn malformed_promotion_queue_json_reports_what_failed_to_parse() {
        let error = parse_promotion_queue("not json").expect_err("garbage does not parse");
        assert!(matches!(
            error,
            ViewModelError::Json {
                what: "promotion queue",
                ..
            }
        ));
    }

    #[test]
    fn a_proposed_record_response_parses_its_optional_record() {
        let record = a_memory_record(
            MemoryCategory::Instructions,
            Scope::Actor(ActorId::new("ada").expect("valid actor id")),
            "run cargo fmt before every commit",
        );
        let body = serde_json::json!({ "record": record }).to_string();
        assert_eq!(
            parse_proposed_record(&body).expect("valid record JSON parses"),
            Some(record)
        );

        let decided = serde_json::json!({ "record": null }).to_string();
        assert_eq!(
            parse_proposed_record(&decided).expect("a decided proposal still parses"),
            None
        );
    }

    #[test]
    fn an_uncertainty_queue_response_parses_into_its_records() {
        let contested = a_memory_record(
            MemoryCategory::Uncertainty,
            Scope::Project(totem_core::RepoId::new("srswart/totem").expect("valid repo id")),
            "actually the deploy runs on Thursdays",
        );
        let body = serde_json::json!({ "pending": [contested] }).to_string();

        let parsed = parse_uncertainty_queue(&body).expect("valid queue JSON parses");

        assert_eq!(parsed, vec![contested]);
    }

    #[test]
    fn an_audit_trail_response_parses_every_section() {
        let record = a_memory_record(
            MemoryCategory::Knowledge,
            Scope::Project(totem_core::RepoId::new("srswart/totem").expect("valid repo id")),
            "a note worth auditing",
        );
        let body = serde_json::json!({
            "record": record,
            "access_log": [],
            "curation_history": [],
            "promotion_history": [a_promotion_event()],
        })
        .to_string();

        let trail = parse_audit_trail(&body).expect("valid audit JSON parses");

        assert_eq!(trail.record, record);
        assert!(trail.access_log.is_empty());
        assert!(trail.curation_history.is_empty());
        assert_eq!(trail.promotion_history.len(), 1);
    }

    #[test]
    fn malformed_audit_trail_json_reports_what_failed_to_parse() {
        let error = parse_audit_trail("not json").expect_err("garbage does not parse");
        assert!(matches!(
            error,
            ViewModelError::Json {
                what: "audit trail",
                ..
            }
        ));
    }
}
