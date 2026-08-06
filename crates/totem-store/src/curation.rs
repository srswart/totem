//! The curation repository: the only code path that retires a record.
//!
//! Everything a curator does happens here, and it does exactly two things —
//! retire a set of originals in favour of a superseding record, and put them
//! back. There is no delete, in this module or anywhere else in the crate: the
//! component invariant ("curation never deletes") is enforced by there being no
//! statement that could.
//!
//! Four rules do the work, and each is checked here rather than trusted to the
//! job that calls it:
//!
//! - **You may only supersede what you can already read.** Every original is
//!   re-fetched through [`MemoryRepository::get`] with the curator's own chain,
//!   so a record outside it reads as absent and cannot be merged away. The
//!   *stored* record is what the policy then judges — never the caller's copy,
//!   which could claim a status the row does not have.
//! - **You may only write the survivor where you could already write.** The
//!   superseding record's scope must be in the curator's chain, the same rule
//!   [`MemoryRepository::save`] applies to any other write.
//! - **A merge applies to exactly the rows its event names, or to none.** Each
//!   `UPDATE` is pinned to the id, the scope, the category, *and* the status
//!   the event recorded, and the transaction throws unless it matched every
//!   row. `UPDATE` matching nothing is not an error in SurrealQL, so without
//!   the count check a merge that raced a live write would leave behind an
//!   event describing something that never happened.
//! - **A rollback restores the status each record actually held.** Not
//!   "active" — the status in the event, so undoing a merge cannot quietly
//!   promote a contested record to a trusted one.

use chrono::{TimeDelta, Utc};
use surrealdb::types::{Object, RecordId, RecordIdKey, SurrealValue, Value};
use surrealdb::{Connection, Surreal};
use totem_core::{
    CurationError, CurationEvent, CurationEventKind, CurationId, CurationPolicy, MemoryCategory,
    MemoryId, MemoryRecord, MemoryStatus, Provenance, ScopeChain,
};

use crate::error::{StoreError, StoreResult};
use crate::memory::{MemoryRepository, check_dimensions};
use crate::row::{self, MEMORY_TABLE, RowError, objects, readable_scopes};

const CURATION_TABLE: &str = "curation_event";

fn kind_key(kind: CurationEventKind) -> &'static str {
    match kind {
        CurationEventKind::Merged => "merged",
        CurationEventKind::RolledBack => "rolled_back",
    }
}

fn kind_from(key: &str) -> Result<CurationEventKind, RowError> {
    match key {
        "merged" => Ok(CurationEventKind::Merged),
        "rolled_back" => Ok(CurationEventKind::RolledBack),
        other => Err(row::malformed(format!(
            "unknown curation event kind: {other}"
        ))),
    }
}

fn curation_thing(id: CurationId) -> RecordId {
    RecordId::new(CURATION_TABLE, RecordIdKey::from(id.to_string()))
}

fn curation_id(thing: &RecordId) -> Result<CurationId, RowError> {
    let RecordIdKey::String(key) = &thing.key else {
        return Err(row::malformed(format!(
            "curation id is not a string key: {thing:?}"
        )));
    };
    key.parse()
        .map_err(|_| row::malformed(format!("curation id is not a uuid: {key}")))
}

fn to_row(event: &CurationEvent, position: i64) -> Object {
    let mut row = Object::new();
    row.insert("id", curation_thing(event.id));
    row.insert("kind", kind_key(event.kind));
    row.insert("merged", row::memory_thing(event.merged));
    row.insert("scope", event.scope.to_string());
    row.insert(
        "superseded",
        event
            .superseded
            .iter()
            .map(|supersession| {
                let mut entry = Object::new();
                entry.insert("memory", row::memory_thing(supersession.memory));
                entry.insert("prior_status", row::status_key(supersession.prior_status));
                entry.into_value()
            })
            .collect::<Vec<Value>>(),
    );
    row.insert(
        "rolls_back",
        event
            .rolls_back
            .map_or(Value::None, |id| curation_thing(id).into_value()),
    );
    row.insert(
        "reason",
        event
            .reason
            .clone()
            .map_or(Value::None, SurrealValue::into_value),
    );
    // Store-assigned, and separated by `position` nanoseconds within a batch,
    // so events written together still have a total order.
    row.insert(
        "recorded_at",
        row::instant(Utc::now() + TimeDelta::nanoseconds(position)),
    );
    row.insert("provenance", row::provenance_to_row(&event.provenance));
    row
}

fn from_row(row: &Object) -> Result<CurationEvent, RowError> {
    let superseded = match row::field(row, "superseded")? {
        Value::Array(entries) => entries
            .iter()
            .map(|entry| {
                let entry = entry
                    .clone()
                    .into_object()
                    .map_err(|_| row::malformed("a supersession is not an object"))?;
                Ok(totem_core::Supersession {
                    memory: row::memory_id(&row::record_id(&entry, "memory")?)?,
                    prior_status: row::status_from(&row::string(&entry, "prior_status")?)?,
                })
            })
            .collect::<Result<Vec<_>, RowError>>()?,
        other => {
            return Err(row::malformed(format!(
                "`superseded` is not an array: {other:?}"
            )));
        }
    };

    Ok(CurationEvent {
        id: curation_id(&row::record_id(row, "id")?)?,
        kind: kind_from(&row::string(row, "kind")?)?,
        merged: row::memory_id(&row::record_id(row, "merged")?)?,
        scope: row::string(row, "scope")?
            .parse()
            .map_err(|error| row::malformed(format!("stored scope is not a scope: {error}")))?,
        superseded,
        rolls_back: match row.get("rolls_back") {
            None | Some(Value::None) | Some(Value::Null) => None,
            Some(_) => Some(curation_id(&row::record_id(row, "rolls_back")?)?),
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

fn events(rows: Value) -> StoreResult<Vec<CurationEvent>> {
    objects(rows)?
        .iter()
        .map(|row| from_row(row).map_err(StoreError::from))
        .collect()
}

/// One `UPDATE` in a status transition, and the number of rows it must match.
///
/// Merges and rollbacks are the same shape — move a named set of records from
/// the status they hold to the status they should hold — so both are built from
/// these rather than from two hand-written statements that could drift apart.
struct StatusMove {
    ids: Vec<Value>,
    from: MemoryStatus,
    to: MemoryStatus,
}

impl StatusMove {
    /// The statement, and the binding names it expects, for position `index`.
    fn statement(&self, index: usize) -> String {
        format!(
            "LET $moved{index} = (UPDATE {MEMORY_TABLE} SET governance.status = $to{index} \
                 WHERE id IN $ids{index} AND scope = $scope AND governance.status = $from{index} \
                 RETURN AFTER);\n\
             IF array::len($moved{index}) != {count} {{ \
                 THROW 'the curation did not apply to exactly the records it names'; }};\n",
            count = self.ids.len()
        )
    }
}

/// Group supersessions by the status they must move *from*, so a merge that
/// found one contested record among active ones still pins every row to the
/// exact status the event recorded.
fn moves_from_event(event: &CurationEvent, restoring: bool) -> Vec<StatusMove> {
    let mut moves: Vec<StatusMove> = Vec::new();
    for supersession in &event.superseded {
        let (from, to) = if restoring {
            (MemoryStatus::Retired, supersession.prior_status)
        } else {
            (supersession.prior_status, MemoryStatus::Retired)
        };
        let id = row::memory_thing(supersession.memory).into_value();
        match moves
            .iter_mut()
            .find(|candidate| candidate.from == from && candidate.to == to)
        {
            Some(existing) => existing.ids.push(id),
            None => moves.push(StatusMove {
                ids: vec![id],
                from,
                to,
            }),
        }
    }
    moves
}

/// Curator merges and their rollbacks.
#[derive(Debug)]
pub struct CurationRepository<'a, C: Connection> {
    db: &'a Surreal<C>,
    policy: CurationPolicy,
}

impl<'a, C: Connection> CurationRepository<'a, C> {
    pub(crate) fn new(db: &'a Surreal<C>, policy: CurationPolicy) -> Self {
        Self { db, policy }
    }

    /// The active records of `category` a curator may consider, oldest first.
    ///
    /// Deliberately not [`MemoryRepository::recall`]: a recall *meters* what it
    /// returns — `use_count` up, `currency` refreshed — and a background job
    /// that scanned everything hourly would manufacture the retrieval signal
    /// the value loop ranks on (docs/project-brief.md G4). This reads the same
    /// scope-resolved set and touches nothing.
    ///
    /// Refuses a category no curator may act on alone, rather than returning an
    /// empty list: "nothing to do" and "you may not" are different answers, and
    /// only one of them is a bug when it is wrong.
    pub async fn candidates(
        &self,
        curator: &ScopeChain,
        category: MemoryCategory,
    ) -> StoreResult<Vec<MemoryRecord>> {
        if !self.policy.may_curate(category) {
            return Err(CurationError::Forbidden(category).into());
        }
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {MEMORY_TABLE} WHERE scope IN $scopes AND category = $category \
                 AND governance.status = $active ORDER BY provenance.created_at ASC, id ASC"
            ))
            .bind(("scopes", readable_scopes(curator)))
            .bind(("category", row::category_key(category).to_string()))
            .bind(("active", row::status_key(MemoryStatus::Active).to_string()))
            .await?
            .check()?;
        objects(response.take(0)?)?
            .iter()
            .map(|row| row::from_row(row).map_err(StoreError::from))
            .collect()
    }

    /// Write `merged` and retire the records it supersedes, or do neither.
    ///
    /// The originals stay exactly where they are, readable, with their history
    /// intact; only their status changes. `merged` should cite them in
    /// `provenance.derived_from`, which is the lineage a reader follows back
    /// from the survivor — this method records the reverse direction, in the
    /// event.
    pub async fn merge(
        &self,
        curator: &ScopeChain,
        merged: &MemoryRecord,
        superseded: &[MemoryRecord],
        provenance: Provenance,
    ) -> StoreResult<CurationEvent> {
        if !curator.contains(&merged.scope) {
            return Err(StoreError::ScopeDenied {
                scope: merged.scope.clone(),
            });
        }
        if let Some(embedding) = &merged.content.embedding {
            check_dimensions(embedding)?;
        }

        // Judge the rows as they are, not as the caller last saw them.
        let memories = MemoryRepository::new(self.db);
        let mut stored = Vec::with_capacity(superseded.len());
        for original in superseded {
            stored.push(
                memories
                    .get(curator, original.id)
                    .await?
                    .ok_or(StoreError::NotFound(original.id))?,
            );
        }

        let event = self.policy.merge(merged, &stored, provenance)?;
        let mut sql = String::from("BEGIN TRANSACTION;\n");
        sql.push_str(&format!("INSERT INTO {MEMORY_TABLE} $survivor;\n"));
        let moves = moves_from_event(&event, false);
        for (index, status_move) in moves.iter().enumerate() {
            sql.push_str(&status_move.statement(index));
        }
        sql.push_str(&format!("INSERT INTO {CURATION_TABLE} $event;\n"));
        sql.push_str("COMMIT TRANSACTION;");

        let mut request = self
            .db
            .query(sql)
            .bind(("survivor", row::to_row(merged)))
            .bind(("scope", event.scope.to_string()))
            .bind(("event", to_row(&event, 0)));
        request = bind_moves(request, &moves);
        request.await?.check()?;
        Ok(event)
    }

    /// Undo a merge: the originals return to the statuses the event recorded,
    /// and the superseding record is retired in their place.
    ///
    /// Refused for an event this caller cannot see (reported as absent, never
    /// as forbidden), for anything that is not a merge, and for a merge that
    /// has already been rolled back — a second rollback would claim to restore
    /// records that are already restored.
    pub async fn rollback(
        &self,
        curator: &ScopeChain,
        merge: CurationId,
        provenance: Provenance,
        reason: Option<String>,
    ) -> StoreResult<CurationEvent> {
        let event = self
            .event(curator, merge)
            .await?
            .ok_or(StoreError::CurationNotFound(merge))?;
        if self.rolled_back(merge).await? {
            return Err(StoreError::CurationRolledBack(merge));
        }

        let rollback = event.rolled_back(provenance)?;
        let rollback = match reason {
            Some(reason) => rollback.with_reason(reason),
            None => rollback,
        };

        let mut sql = String::from("BEGIN TRANSACTION;\n");
        let moves = moves_from_event(&rollback, true);
        for (index, status_move) in moves.iter().enumerate() {
            sql.push_str(&status_move.statement(index));
        }
        // The survivor is retired in exchange, pinned to `active` so a rollback
        // cannot race a second one into retiring something twice.
        sql.push_str(&format!(
            "LET $survivor = (UPDATE {MEMORY_TABLE} SET governance.status = $retired \
                 WHERE id = $merged AND scope = $scope AND governance.status = $active \
                 RETURN AFTER);\n\
             IF array::len($survivor) != 1 {{ \
                 THROW 'the rollback did not retire the superseding record'; }};\n"
        ));
        sql.push_str(&format!("INSERT INTO {CURATION_TABLE} $event;\n"));
        sql.push_str("COMMIT TRANSACTION;");

        let mut request = self
            .db
            .query(sql)
            .bind(("merged", row::memory_thing(rollback.merged)))
            .bind(("scope", rollback.scope.to_string()))
            .bind((
                "retired",
                row::status_key(MemoryStatus::Retired).to_string(),
            ))
            .bind(("active", row::status_key(MemoryStatus::Active).to_string()))
            .bind(("event", to_row(&rollback, 0)));
        request = bind_moves(request, &moves);
        request.await?.check()?;
        Ok(rollback)
    }

    /// Every curation event this reader may see, oldest first — the feed a
    /// console audit view renders (ADV-CONSOLE-002).
    ///
    /// Scope-filtered on the event's own `scope`, which is sound precisely
    /// because a merge may not cross a boundary: one scope covers the survivor
    /// and every original alike.
    pub async fn events(&self, reader: &ScopeChain) -> StoreResult<Vec<CurationEvent>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {CURATION_TABLE} WHERE scope IN $scopes ORDER BY recorded_at ASC"
            ))
            .bind(("scopes", readable_scopes(reader)))
            .await?
            .check()?;
        events(response.take(0)?)
    }

    /// One record's curation history, oldest first: the merges that superseded
    /// it and the merge that produced it, with their rollbacks.
    pub async fn history(
        &self,
        reader: &ScopeChain,
        memory: MemoryId,
    ) -> StoreResult<Vec<CurationEvent>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {CURATION_TABLE} \
                 WHERE scope IN $scopes AND (merged = $memory OR $memory IN superseded.*.memory) \
                 ORDER BY recorded_at ASC"
            ))
            .bind(("memory", row::memory_thing(memory)))
            .bind(("scopes", readable_scopes(reader)))
            .await?
            .check()?;
        events(response.take(0)?)
    }

    /// One event, if this reader's chain reaches the scope it happened at.
    pub async fn event(
        &self,
        reader: &ScopeChain,
        id: CurationId,
    ) -> StoreResult<Option<CurationEvent>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {CURATION_TABLE} WHERE id = $id AND scope IN $scopes"
            ))
            .bind(("id", curation_thing(id)))
            .bind(("scopes", readable_scopes(reader)))
            .await?
            .check()?;
        Ok(events(response.take(0)?)?.into_iter().next())
    }

    /// Whether some event already undoes this merge.
    async fn rolled_back(&self, merge: CurationId) -> StoreResult<bool> {
        let mut response = self
            .db
            .query(format!(
                "SELECT VALUE id FROM {CURATION_TABLE} WHERE rolls_back = $id"
            ))
            .bind(("id", curation_thing(merge)))
            .await?
            .check()?;
        let rollbacks: Value = response.take(0)?;
        Ok(!rollbacks
            .into_array()
            .map_err(|_| StoreError::Row("rollback query did not return an array".to_string()))?
            .is_empty())
    }
}

/// Bind the parameters every [`StatusMove`] statement names.
fn bind_moves<'a, C: Connection>(
    mut request: surrealdb::method::Query<'a, C>,
    moves: &[StatusMove],
) -> surrealdb::method::Query<'a, C> {
    for (index, status_move) in moves.iter().enumerate() {
        request = request
            .bind((format!("ids{index}"), status_move.ids.clone()))
            .bind((
                format!("from{index}"),
                row::status_key(status_move.from).to_string(),
            ))
            .bind((
                format!("to{index}"),
                row::status_key(status_move.to).to_string(),
            ));
    }
    request
}

#[cfg(test)]
mod tests {
    //! The transaction guard, which the public API cannot reach.
    //!
    //! `merge` refuses a drifted or invisible record in Rust before any
    //! statement runs, so the `IF ... THROW` inside the transaction is only
    //! ever the *second* refusal. This test calls the statement path with an
    //! event whose recorded prior status is a lie, which is the shape a race
    //! with a live write would take.

    use super::*;
    use crate::Store;
    use totem_core::{
        ActorId, Author, Content, Harness, MemoryCategory, MemoryRecord, RepoId, Scope, SessionId,
    };

    #[tokio::test]
    async fn a_merge_whose_recorded_status_is_stale_applies_to_nothing() {
        let store = Store::in_memory().await.expect("embedded engine connects");
        store.migrate().await.expect("migrations apply");
        let ada = ActorId::new("ada").expect("valid actor id");
        let repo = RepoId::new("srswart/totem").expect("valid repo id");
        let chain = ScopeChain::resolve(&ada, Some(&repo), &[]);
        let provenance = Provenance::new(
            Author::Curator(ada.clone()),
            Harness::Curator,
            SessionId::new("curate-1").expect("valid session id"),
            "2026-08-06T08:00:00Z".parse().expect("valid timestamp"),
        );

        let mut originals = Vec::new();
        for body in ["a fact", "a fact."] {
            let record = MemoryRecord::new(
                MemoryCategory::Knowledge,
                Scope::Project(repo.clone()),
                Content::new(body),
                provenance.clone(),
            );
            store.memories().save(&chain, &record).await.expect("saves");
            originals.push(record);
        }
        let survivor = MemoryRecord::new(
            MemoryCategory::Knowledge,
            Scope::Project(repo.clone()),
            Content::new("a fact"),
            provenance.clone(),
        );

        // Every original is active in the store; the event claims they were
        // contested, so each pinned UPDATE matches nothing.
        let mut stale: Vec<MemoryRecord> = originals.clone();
        for original in &mut stale {
            original.governance.status = MemoryStatus::Contested;
        }
        let event = CurationPolicy::new()
            .merge(&survivor, &stale, provenance)
            .expect("the policy allows it");

        let mut sql = String::from("BEGIN TRANSACTION;\n");
        sql.push_str(&format!("INSERT INTO {MEMORY_TABLE} $survivor;\n"));
        let moves = moves_from_event(&event, false);
        for (index, status_move) in moves.iter().enumerate() {
            sql.push_str(&status_move.statement(index));
        }
        sql.push_str(&format!("INSERT INTO {CURATION_TABLE} $event;\n"));
        sql.push_str("COMMIT TRANSACTION;");
        let request = store
            .connection()
            .query(sql)
            .bind(("survivor", row::to_row(&survivor)))
            .bind(("scope", event.scope.to_string()))
            .bind(("event", to_row(&event, 0)));
        let refused = bind_moves(request, &moves).await.expect("sent").check();

        assert!(
            refused.is_err(),
            "a merge against stale statuses was applied anyway: {refused:?}",
        );
        assert!(
            store
                .curation()
                .events(&chain)
                .await
                .expect("the trail reads")
                .is_empty(),
            "a refused merge left an event behind",
        );
        assert!(
            store
                .memories()
                .get(&chain, survivor.id)
                .await
                .expect("the read succeeds")
                .is_none(),
            "a refused merge inserted its survivor anyway",
        );
    }
}
