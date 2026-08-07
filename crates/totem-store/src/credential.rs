//! Durable credential grants (ADV-GATEWAY-012).
//!
//! The gateway's verifying registry was in-memory until this advance: every
//! grant except the env-seeded bootstrap vanished on restart, which a
//! deployed instance does on every deploy. These rows are what survive.
//!
//! **Only fingerprints live here.** A grant records what a credential is
//! *bound to* — repo, scope, actor, expiry — never the token that hashes to
//! it. The gateway hashes a presented token and looks the fingerprint up;
//! nothing in this crate can reverse that.

use chrono::{DateTime, Utc};
use surrealdb::types::SurrealValue;
use surrealdb::{Connection, Surreal};

use crate::error::{StoreError, StoreResult};

const CREDENTIAL_TABLE: &str = "credential";

/// One credential's binding, as stored. Deliberately has no token field —
/// see the module docs, and `tests/credentials.rs` which asserts it.
#[derive(Debug, Clone, PartialEq, Eq, SurrealValue)]
pub struct CredentialGrantRow {
    /// SHA-256 of the token text, hex-encoded. The lookup key.
    pub fingerprint: String,
    /// The repo this credential may name (`owner/name`).
    pub repo: String,
    /// The single scope it is bound to, in wire form.
    pub scope: String,
    /// The actor identity it authenticates as.
    pub actor: String,
    /// When it stops being valid. `None` never expires.
    pub expires_at: Option<DateTime<Utc>>,
    /// Revocation tombstone. Revoked rows are kept, never deleted, so a
    /// replayed fingerprint cannot resurrect a credential by re-creating a
    /// row that would otherwise be absent.
    pub revoked: bool,
}

/// Reads and writes credential grants.
pub struct CredentialRepository<'a, C: Connection> {
    pub(crate) db: &'a Surreal<C>,
}

impl<C: Connection> CredentialRepository<'_, C> {
    /// Persist a grant.
    ///
    /// Refused if the fingerprint was previously revoked: re-recording it
    /// would undo the revocation, which is the one thing an operator racing
    /// an incident must be able to rely on.
    pub async fn record(&self, grant: &CredentialGrantRow) -> StoreResult<()> {
        if let Some(existing) = self.find(&grant.fingerprint).await?
            && existing.revoked
        {
            return Err(StoreError::CredentialRevoked(grant.fingerprint.clone()));
        }

        // Written by field rather than by an explicit record id: the UNIQUE
        // index on `fingerprint` is what keeps one row per credential, and
        // building record ids from bound variables proved to be a syntax
        // trap not worth carrying (`type::thing` does not exist in SurrealDB
        // 3, and id interpolation does not bind).
        if self.find(&grant.fingerprint).await?.is_some() {
            self.db
                .query(format!(
                    "UPDATE {CREDENTIAL_TABLE} SET repo = $repo, scope = $scope, \
                     actor = $actor, expires_at = $expires_at, revoked = false \
                     WHERE fingerprint = $fingerprint"
                ))
                .bind(("fingerprint", grant.fingerprint.clone()))
                .bind(("repo", grant.repo.clone()))
                .bind(("scope", grant.scope.clone()))
                .bind(("actor", grant.actor.clone()))
                .bind(("expires_at", grant.expires_at))
                .await?
                .check()?;
        } else {
            self.db
                .query(format!("CREATE {CREDENTIAL_TABLE} CONTENT $row"))
                .bind(("row", grant.clone()))
                .await?
                .check()?;
        }
        Ok(())
    }

    /// Every grant that may currently authenticate: not revoked, not expired.
    pub async fn active(&self) -> StoreResult<Vec<CredentialGrantRow>> {
        let now = Utc::now();
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {CREDENTIAL_TABLE} WHERE revoked = false"
            ))
            .await?
            .check()?;
        let rows: Vec<CredentialGrantRow> = response.take(0)?;
        Ok(rows
            .into_iter()
            .filter(|row| row.expires_at.is_none_or(|expiry| expiry > now))
            .collect())
    }

    /// Revoke a credential permanently.
    ///
    /// An unknown fingerprint is an error, not a no-op: a revocation that
    /// silently does nothing reads as success to whoever is trying to cut off
    /// a leaked credential.
    pub async fn revoke(&self, fingerprint: &str) -> StoreResult<()> {
        if self.find(fingerprint).await?.is_none() {
            return Err(StoreError::CredentialNotFound(fingerprint.to_string()));
        }
        self.db
            .query(format!(
                "UPDATE {CREDENTIAL_TABLE} SET revoked = true WHERE fingerprint = $fingerprint"
            ))
            .bind(("fingerprint", fingerprint.to_string()))
            .await?
            .check()?;
        Ok(())
    }

    async fn find(&self, fingerprint: &str) -> StoreResult<Option<CredentialGrantRow>> {
        let mut response = self
            .db
            .query(format!(
                "SELECT * FROM {CREDENTIAL_TABLE} WHERE fingerprint = $fingerprint LIMIT 1"
            ))
            .bind(("fingerprint", fingerprint.to_string()))
            .await?
            .check()?;
        let rows: Vec<CredentialGrantRow> = response.take(0)?;
        Ok(rows.into_iter().next())
    }
}
