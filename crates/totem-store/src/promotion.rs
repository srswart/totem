//! The promotion repository: the only code path that changes a record's scope.
//!
//! Every other repository treats `scope` as immutable — [`MemoryRepository`]
//! writes it once on `save` and never touches it again, and `revise` says so
//! outright. That is what makes this module the whole of the attack surface for
//! the project's highest-severity failure, and why the statement that writes a
//! scope lives inside the same transaction as the event authorising it.
//!
//! Three rules do the work, and each is checked here rather than trusted to a
//! caller:
//!
//! - **You may only propose what you can already read.** The record is fetched
//!   through [`MemoryRepository::get`] with the proposer's own chain, so a
//!   record outside it reads as absent and cannot be promoted.
//! - **You may only propose into a scope you can already reach.** The target
//!   must be in the proposer's chain — the same rule `save` applies to a new
//!   record, applied to the destination of a move. A decision is held to the
//!   same rule against the *decider's* chain.
//! - **The move and its event are one transaction.** The scope update must
//!   affect exactly one row or the whole statement is thrown away, so there is
//!   no recorded promotion that did not happen, and no promotion that happened
//!   without a record.
//!
//! What propose-time authority does *not* cover is the approval itself: an
//! approver reaching into `actor:ada` to move her record is authorised by the
//! proposal she wrote, not by the approver's own chain. That is the sanctioned
//! crossing, and it is narrow by construction — see
//! [`proposed_record`](PromotionRepository::proposed_record).

use chrono::{TimeDelta, Utc};
use surrealdb::types::{Object, RecordId, RecordIdKey, SurrealValue, Value};
use surrealdb::{Connection, Surreal};
use totem_core::{
    MemoryId, MemoryRecord, PromotionError, PromotionEvent, PromotionEventKind, PromotionId,
    PromotionPath, PromotionPolicy, Provenance, Scope, ScopeChain,
};

use crate::error::{StoreError, StoreResult};
use crate::memory::MemoryRepository;
use crate::row::{self, MEMORY_TABLE, RowError, objects, readable_scopes};

const PROMOTION_TABLE: &str = "promotion_event";

fn kind_key(kind: PromotionEventKind) -> &'static str {
    match kind {
        PromotionEventKind::Proposed => "proposed",
        PromotionEventKind::AutoApproved => "auto_approved",
        PromotionEventKind::Approved => "approved",
        PromotionEventKind::Rejected => "rejected",
        PromotionEventKind::Demoted => "demoted",
    }
}

fn kind_from(key: &str) -> Result<PromotionEventKind, RowError> {
    match key {
        "proposed" => Ok(PromotionEventKind::Proposed),
        "auto_approved" => Ok(PromotionEventKind::AutoApproved),
        "approved" => Ok(PromotionEventKind::Approved),
        "rejected" => Ok(PromotionEventKind::Rejected),
        "demoted" => Ok(PromotionEventKind::Demoted),
        other => Err(row::malformed(format!(
            "unknown promotion event kind: {other}"
        ))),
    }
}

fn promotion_thing(id: PromotionId) -> RecordId {
    RecordId::new(PROMOTION_TABLE, RecordIdKey::from(id.to_string()))
}

fn promotion_id(thing: &RecordId) -> Result<PromotionId, RowError> {
    let RecordIdKey::String(key) = &thing.key else {
        return Err(row::malformed(format!(
            "promotion id is not a string key: {thing:?}"
        )));
    };
    key.parse()
        .map_err(|_| row::malformed(format!("promotion id is not a uuid: {key}")))
}

fn scope_from(row: &Object, key: &str) -> Result<Scope, RowError> {
    row::string(row, key)?
        .parse()
        .map_err(|error| row::malformed(format!("stored {key} is not a scope: {error}")))
}

fn from_row(row: &Object) -> Result<PromotionEvent, RowError> {
    Ok(PromotionEvent {
        id: promotion_id(&row::record_id(row, "id")?)?,
        memory: row::memory_id(&row::record_id(row, "memory")?)?,
        kind: kind_from(&row::string(row, "kind")?)?,
        from_scope: scope_from(row, "from_scope")?,
        to_scope: scope_from(row, "to_scope")?,
        proposal: match row.get("proposal") {
            None | Some(Value::None) | Some(Value::Null) => None,
            Some(_) => Some(promotion_id(&row::record_id(row, "proposal")?)?),
        },
        reason: match row.get("reason") {
            None | Some(Value::None) | Some(Value::Null) => None,
            Some(_) => Some(row::string(row, "reason")?),
        },
        provenance: row::provenance_from_row(&match row::field(row, "provenance")? {
            Value::Object(value) => value.clone(),
            other => {
                return Err(row::malformed(format!(
                    "`provenance` is not an object: {other:?}"
                )));
            }
        })?,
    })
}

/// The stored shape of one event, at a store-assigned position in the trail.
///
/// `recorded_at` is stamped here rather than taken from `provenance.created_at`
/// because that field is whatever the calling harness reported: ordering the
/// audit trail by it would let a caller with a wrong clock — or a deliberately
/// backdated one — rearrange the record of who decided what. Events written in
/// one batch are separated by `position` nanoseconds so that a proposal and the
/// decision recorded alongside it have a total order rather than a tie.
fn to_row(event: &PromotionEvent, position: i64) -> Object {
    let mut row = Object::new();
    row.insert("id", promotion_thing(event.id));
    row.insert("memory", row::memory_thing(event.memory));
    row.insert("kind", kind_key(event.kind));
    row.insert("from_scope", event.from_scope.to_string());
    row.insert("to_scope", event.to_scope.to_string());
    row.insert(
        "proposal",
        event
            .proposal
            .map_or(Value::None, |id| promotion_thing(id).into_value()),
    );
    row.insert(
        "reason",
        event
            .reason
            .clone()
            .map_or(Value::None, SurrealValue::into_value),
    );
    row.insert(
        "recorded_at",
        row::instant(Utc::now() + TimeDelta::nanoseconds(position)),
    );
    row.insert("provenance", row::provenance_to_row(&event.provenance));
    row
}

/// What proposing a promotion did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromotionOutcome {
    /// Policy allowed the move without a human: the record is already at the
    /// wider scope, and both the ask and the automatic approval are recorded.
    Promoted {
        /// The recorded ask.
        proposal: PromotionEvent,
        /// The recorded automatic approval. Boxed only to keep the two
        /// variants a similar size; it derefs like any other event.
        decision: Box<PromotionEvent>,
    },
    /// A human must decide. The record has not moved and is not readable at the
    /// target scope until someone approves.
    Pending {
        /// The recorded ask, now in the queue.
        proposal: PromotionEvent,
    },
}

/// Proposals, decisions, and demotions.
#[derive(Debug)]
pub struct PromotionRepository<'a, C: Connection> {
    db: &'a Surreal<C>,
    policy: PromotionPolicy,
}

impl<'a, C: Connection> PromotionRepository<'a, C> {
    pub(crate) fn new(db: &'a Surreal<C>, policy: PromotionPolicy) -> Self {
        Self { db, policy }
    }

    /// Ask for a record to move to a wider scope.
    ///
    /// Returns [`PromotionOutcome::Promoted`] when policy allows the move
    /// outright and it has already happened, or [`PromotionOutcome::Pending`]
    /// when it is queued for a human.
    ///
    /// Refused when the proposer cannot read the record (reported as
    /// [`StoreError::NotFound`], never as forbidden), when the target is not in
    /// the proposer's own chain, or when the category or direction makes the
    /// move impossible.
    pub async fn propose(
        &self,
        proposer: &ScopeChain,
        memory: MemoryId,
        to: Scope,
        provenance: Provenance,
    ) -> StoreResult<PromotionOutcome> {
        let record = MemoryRepository::new(self.db)
            .get(proposer, memory)
            .await?
            .ok_or(StoreError::NotFound(memory))?;
        if !proposer.contains(&to) {
            return Err(StoreError::ScopeDenied { scope: to });
        }

        let path = self
            .policy
            .check_promotion(record.category, &record.scope, &to)?;
        let proposal = PromotionEvent::propose(memory, record.scope, to, provenance.clone());
        match path {
            // Unreachable through `check_promotion`, which refuses a forbidden
            // category outright. Kept as the second of two independent
            // refusals rather than an `unreachable!`, so a future change to
            // either one cannot quietly open the path.
            PromotionPath::Forbidden => Err(PromotionError::Forbidden(record.category).into()),
            PromotionPath::HumanGated => {
                self.append(&[&proposal]).await?;
                Ok(PromotionOutcome::Pending { proposal })
            }
            PromotionPath::Automatic => {
                let decision = proposal.auto_approved(provenance);
                self.apply(&[&proposal, &decision], &proposal).await?;
                Ok(PromotionOutcome::Promoted {
                    proposal,
                    decision: Box::new(decision),
                })
            }
        }
    }

    /// Approve a queued proposal, moving the record.
    pub async fn approve(
        &self,
        approver: &ScopeChain,
        proposal: PromotionId,
        provenance: Provenance,
    ) -> StoreResult<PromotionEvent> {
        let proposal = self.open_proposal(approver, proposal).await?;
        let decision = proposal.approved(provenance);
        self.apply(&[&decision], &proposal).await?;
        Ok(decision)
    }

    /// Refuse a queued proposal. The record does not move, and the refusal is
    /// recorded — a proposal that simply disappeared would leave no answer to
    /// "was this ever asked for?".
    pub async fn reject(
        &self,
        approver: &ScopeChain,
        proposal: PromotionId,
        provenance: Provenance,
        reason: Option<String>,
    ) -> StoreResult<PromotionEvent> {
        let proposal = self.open_proposal(approver, proposal).await?;
        let decision = with_reason(proposal.rejected(provenance), reason);
        self.append(&[&decision]).await?;
        Ok(decision)
    }

    /// Narrow a record's scope, compensating an earlier promotion.
    ///
    /// Never queued: narrowing reduces exposure, so it is the rollback lever
    /// and must not wait on the queue that let the promotion through. The
    /// target must still be in the caller's own chain — demoting *into* another
    /// actor's private scope would move the record out of everyone's reach and
    /// into theirs, which is a leak wearing a rollback's clothes.
    pub async fn demote(
        &self,
        actor: &ScopeChain,
        memory: MemoryId,
        to: Scope,
        provenance: Provenance,
        reason: Option<String>,
    ) -> StoreResult<PromotionEvent> {
        let record = MemoryRepository::new(self.db)
            .get(actor, memory)
            .await?
            .ok_or(StoreError::NotFound(memory))?;
        if !actor.contains(&to) {
            return Err(StoreError::ScopeDenied { scope: to });
        }
        self.policy
            .check_demotion(record.category, &record.scope, &to)?;

        let event = with_reason(
            PromotionEvent::demotion(memory, record.scope, to, provenance),
            reason,
        );
        self.apply(&[&event], &event).await?;
        Ok(event)
    }

    /// The queue a human decides from (the one ADV-CONSOLE-002 renders):
    /// proposals nobody has answered yet, aimed at a scope this reader can
    /// reach, oldest first.
    pub async fn pending(&self, reader: &ScopeChain) -> StoreResult<Vec<PromotionEvent>> {
        let sql = format!(
            "LET $decided = (SELECT VALUE proposal FROM {PROMOTION_TABLE} WHERE proposal != NONE);\n\
             SELECT * FROM {PROMOTION_TABLE} WHERE kind = 'proposed' AND to_scope IN $scopes \
                 AND id NOT IN $decided ORDER BY recorded_at ASC;"
        );
        let mut response = self
            .db
            .query(sql)
            .bind(("scopes", readable_scopes(reader)))
            .await?
            .check()?;
        events(response.take(1)?)
    }

    /// One record's whole scope history, oldest first.
    ///
    /// Scope-filtered at either end of each move: a reader who can reach
    /// neither where a record came from nor where it went has no business
    /// knowing it moved.
    pub async fn history(
        &self,
        reader: &ScopeChain,
        memory: MemoryId,
    ) -> StoreResult<Vec<PromotionEvent>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {PROMOTION_TABLE} WHERE memory = $memory \
                 AND (from_scope IN $scopes OR to_scope IN $scopes) ORDER BY recorded_at ASC"
            ))
            .bind(("memory", row::memory_thing(memory)))
            .bind(("scopes", readable_scopes(reader)))
            .await?
            .check()?;
        events(response.take(0)?)
    }

    /// The record a queued proposal names, for the reviewers who must decide
    /// on it.
    ///
    /// This is the one place a reader sees a record its own chain does not
    /// reach, and it is deliberate: proposing a private note for a wider scope
    /// *is* asking that scope's reviewers to read it. The disclosure is bounded
    /// on every side — the proposal must still be open, the reviewer must be
    /// able to reach the scope it targets, and the record fetched is pinned to
    /// the id and origin scope the proposal itself recorded, so no other row
    /// can be reached through it. `None` once the proposal is decided: after
    /// approval the record is readable normally, and after rejection it is not
    /// readable at all.
    pub async fn proposed_record(
        &self,
        reviewer: &ScopeChain,
        proposal: PromotionId,
    ) -> StoreResult<Option<MemoryRecord>> {
        let proposal = match self.open_proposal(reviewer, proposal).await {
            Ok(proposal) => proposal,
            Err(StoreError::PromotionNotFound(_) | StoreError::PromotionDecided(_)) => {
                return Ok(None);
            }
            Err(other) => return Err(other),
        };

        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {MEMORY_TABLE} WHERE id = $id AND scope = $from"
            ))
            .bind(("id", row::memory_thing(proposal.memory)))
            .bind(("from", proposal.from_scope.to_string()))
            .await?
            .check()?;
        let rows = objects(response.take(0)?)?;
        rows.first()
            .map(|row| row::from_row(row).map_err(StoreError::from))
            .transpose()
    }

    /// Load a proposal this reviewer is entitled to decide, refusing one that
    /// has already been answered.
    ///
    /// A proposal aimed outside the reviewer's chain reads as absent rather
    /// than forbidden, for the same reason [`MemoryRepository::get`] does: a
    /// "you may not" would confirm that someone, somewhere, wants to share
    /// something with a scope this caller cannot see.
    async fn open_proposal(
        &self,
        reviewer: &ScopeChain,
        id: PromotionId,
    ) -> StoreResult<PromotionEvent> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {PROMOTION_TABLE} \
                 WHERE id = $id AND kind = 'proposed' AND to_scope IN $scopes"
            ))
            .bind(("id", promotion_thing(id)))
            .bind(("scopes", readable_scopes(reviewer)))
            .await?
            .check()?;
        let Some(proposal) = events(response.take(0)?)?.into_iter().next() else {
            return Err(StoreError::PromotionNotFound(id));
        };

        let mut response = self
            .db
            .query(format!(
                "SELECT VALUE id FROM {PROMOTION_TABLE} WHERE proposal = $id"
            ))
            .bind(("id", promotion_thing(id)))
            .await?
            .check()?;
        let decisions: Value = response.take(0)?;
        let decided = decisions
            .into_array()
            .map_err(|_| StoreError::Row("decision query did not return an array".to_string()))?;
        if !decided.is_empty() {
            return Err(StoreError::PromotionDecided(id));
        }
        Ok(proposal)
    }

    /// Record events that change nothing.
    async fn append(&self, events: &[&PromotionEvent]) -> StoreResult<()> {
        self.db
            .query(format!("INSERT INTO {PROMOTION_TABLE} $rows"))
            .bind(("rows", rows(events)))
            .await?
            .check()?;
        Ok(())
    }

    /// Record events *and* make the move they authorise, or neither.
    ///
    /// The `IF ... THROW` is the point of the whole statement: `UPDATE` matching
    /// no rows is not an error in SurrealQL, so without it a record whose scope
    /// had drifted since it was read would leave behind an event claiming a
    /// move that never happened. `scope = $from` pins the update to the exact
    /// origin the event records, and `category != $episodic` is the same
    /// defence in depth the rest of the crate applies — the schema's own EVENT
    /// already refuses to touch an episodic row.
    async fn apply(&self, events: &[&PromotionEvent], effect: &PromotionEvent) -> StoreResult<()> {
        let sql = format!(
            "BEGIN TRANSACTION;\n\
             LET $moved = (UPDATE {MEMORY_TABLE} SET scope = $to \
                 WHERE id = $memory AND scope = $from AND category != $episodic RETURN AFTER);\n\
             IF array::len($moved) != 1 {{ \
                 THROW 'the promotion did not apply to exactly one record'; }};\n\
             INSERT INTO {PROMOTION_TABLE} $rows;\n\
             COMMIT TRANSACTION;"
        );
        self.db
            .query(sql)
            .bind(("memory", row::memory_thing(effect.memory)))
            .bind(("from", effect.from_scope.to_string()))
            .bind(("to", effect.to_scope.to_string()))
            .bind((
                "episodic",
                row::category_key(totem_core::MemoryCategory::Episodic),
            ))
            .bind(("rows", rows(events)))
            .await?
            .check()?;
        Ok(())
    }
}

fn with_reason(event: PromotionEvent, reason: Option<String>) -> PromotionEvent {
    match reason {
        Some(reason) => event.with_reason(reason),
        None => event,
    }
}

fn rows(events: &[&PromotionEvent]) -> Vec<Value> {
    events
        .iter()
        .enumerate()
        .map(|(position, event)| to_row(event, position as i64).into_value())
        .collect()
}

fn events(rows: Value) -> StoreResult<Vec<PromotionEvent>> {
    objects(rows)?
        .iter()
        .map(|row| from_row(row).map_err(StoreError::from))
        .collect()
}

#[cfg(test)]
mod tests {
    //! The transaction guard, which the public API cannot reach.
    //!
    //! `propose` and `demote` both refuse a forbidden or drifted move in Rust
    //! before any statement runs, so the `IF ... THROW` inside `apply` is only
    //! ever the *second* refusal. These tests call `apply` directly to prove
    //! that second refusal is real — without them, the guard would be untested
    //! precisely because the first one works.

    use super::*;
    use crate::Store;
    use totem_core::{
        ActorId, Author, Content, Harness, MemoryCategory, MemoryRecord, RepoId, SessionId,
    };

    struct Fixture {
        store: Store<surrealdb::engine::local::Db>,
        chain: ScopeChain,
        provenance: Provenance,
        repo: RepoId,
    }

    async fn fixture() -> Fixture {
        let store = Store::in_memory().await.expect("embedded engine connects");
        store.migrate().await.expect("migrations apply");
        let ada = ActorId::new("ada").expect("valid actor id");
        let repo = RepoId::new("srswart/totem").expect("valid repo id");
        Fixture {
            chain: ScopeChain::resolve(&ada, Some(&repo), &[]),
            provenance: Provenance::new(
                Author::Human(ada),
                Harness::Console,
                SessionId::new("s1").expect("valid session id"),
                "2026-08-06T06:00:00Z".parse().expect("valid timestamp"),
            ),
            store,
            repo,
        }
    }

    async fn saved(fixture: &Fixture, category: MemoryCategory, scope: Scope) -> MemoryRecord {
        let record = MemoryRecord::new(
            category,
            scope,
            Content::new("a note"),
            fixture.provenance.clone(),
        );
        fixture
            .store
            .memories()
            .save(&fixture.chain, &record)
            .await
            .expect("saved");
        record
    }

    #[tokio::test]
    async fn an_event_is_never_recorded_for_a_move_that_did_not_happen() {
        let fixture = fixture().await;
        let record = saved(
            &fixture,
            MemoryCategory::Knowledge,
            Scope::Project(fixture.repo.clone()),
        )
        .await;

        // The event claims the record sits at platform scope. It does not, so
        // the UPDATE matches no row — which SurrealQL does not treat as an
        // error, and which would otherwise leave behind an event asserting a
        // promotion that never occurred.
        let event = PromotionEvent::propose(
            record.id,
            Scope::Platform,
            Scope::Platform,
            fixture.provenance.clone(),
        );
        let refused = fixture.store.promotions().apply(&[&event], &event).await;
        assert!(
            refused.is_err(),
            "a move matching no record was recorded anyway: {refused:?}",
        );
        assert!(
            fixture
                .store
                .promotions()
                .history(&fixture.chain, record.id)
                .await
                .expect("history reads")
                .is_empty(),
            "a refused move left an event behind",
        );
    }

    #[tokio::test]
    async fn the_transaction_refuses_an_episodic_move_the_policy_would_have_caught_first() {
        let fixture = fixture().await;
        let record = saved(
            &fixture,
            MemoryCategory::Episodic,
            Scope::Actor(ActorId::new("ada").expect("valid actor id")),
        )
        .await;

        let event = PromotionEvent::demotion(
            record.id,
            record.scope.clone(),
            Scope::Platform,
            fixture.provenance.clone(),
        );
        let refused = fixture.store.promotions().apply(&[&event], &event).await;
        assert!(
            refused.is_err(),
            "an episodic record was moved past the policy: {refused:?}",
        );

        let unmoved = fixture
            .store
            .memories()
            .get(&fixture.chain, record.id)
            .await
            .expect("read succeeds")
            .expect("still there");
        assert_eq!(unmoved.scope, record.scope);
    }
}
