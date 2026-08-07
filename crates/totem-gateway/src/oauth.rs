//! Totem as an OAuth 2.1 resource server (ADV-GATEWAY-013).
//!
//! Per the MCP authorization spec (2025-06-18), an MCP server acts as a
//! *resource server*; the authorization server "may be hosted with the
//! resource server or a separate entity" and its implementation is out of
//! scope. Totem therefore **issues no tokens, runs no login UI, and holds no
//! client secret** — it publishes protected-resource metadata naming a
//! third-party authorization server (WorkOS AuthKit) and validates what that
//! server issues.
//!
//! ADV-GATEWAY-011 established why this exists: claude.ai connectors offer no
//! static-bearer field at all (MCP-013), so the deployed gateway is
//! unreachable from a scheduled routine without it.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;
use totem_core::{ActorId, RepoId, Scope};

use crate::auth::{AuthError, TokenGrant};

/// Where the metadata document lives. RFC 9728 fixes this path, and the 401's
/// `resource_metadata` parameter points at it.
pub const PROTECTED_RESOURCE_METADATA_PATH: &str = "/.well-known/oauth-protected-resource";

/// Signing keys, and where they come from.
enum Keys {
    /// Production: fetched from the authorization server's JWKS and cached by
    /// key id. Refreshed when a token arrives with an unknown `kid`, which is
    /// how key rotation is survived without a restart.
    Jwks {
        uri: String,
        cache: RwLock<HashMap<String, DecodingKey>>,
    },
    /// Tests: one key, one algorithm, no network.
    Fixed {
        key: DecodingKey,
        algorithm: Algorithm,
    },
}

/// Validates access tokens issued by the configured authorization server.
pub struct OAuthVerifier {
    issuer: String,
    /// Every audience value that identifies *this* server. Both the origin
    /// and the `/mcp` path are accepted because the MCP spec allows a client
    /// either form, and a mismatch would fail for a reason invisible from
    /// outside (ADV-GATEWAY-011's Resource Indicator note).
    audiences: Vec<String>,
    repo: String,
    scope: String,
    keys: Keys,
}

impl std::fmt::Debug for OAuthVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No key material, ever.
        f.debug_struct("OAuthVerifier")
            .field("issuer", &self.issuer)
            .field("audiences", &self.audiences)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    #[allow(dead_code)]
    iss: String,
}

#[derive(Debug, Deserialize)]
struct JwksDocument {
    keys: Vec<JwkEntry>,
}

#[derive(Debug, Deserialize)]
struct JwkEntry {
    kid: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

impl OAuthVerifier {
    /// Production: validate RS256 tokens against the authorization server's
    /// JWKS.
    pub fn new(
        issuer: String,
        audiences: Vec<String>,
        repo: String,
        scope: String,
        jwks_uri: String,
    ) -> Self {
        Self {
            issuer,
            audiences,
            repo,
            scope,
            keys: Keys::Jwks {
                uri: jwks_uri,
                cache: RwLock::new(HashMap::new()),
            },
        }
    }

    /// Tests: one fixed key, no network. Never constructed by the binary.
    pub fn with_fixed_key(
        issuer: String,
        audiences: Vec<String>,
        repo: String,
        scope: String,
        key: DecodingKey,
        algorithm: Algorithm,
    ) -> Self {
        Self {
            issuer,
            audiences,
            repo,
            scope,
            keys: Keys::Fixed { key, algorithm },
        }
    }

    /// The RFC 9728 protected-resource metadata document.
    ///
    /// Served unauthenticated: a client that cannot read it cannot discover
    /// how to authenticate at all (MCP-014).
    pub fn metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "resource": self.audiences.first().cloned().unwrap_or_default(),
            "authorization_servers": [self.issuer.clone()],
            "bearer_methods_supported": ["header"],
        })
    }

    /// The authorization server this deployment trusts.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// The canonical resource identifier a token must be audienced for.
    pub fn resource(&self) -> &str {
        self.audiences
            .first()
            .map(String::as_str)
            .unwrap_or_default()
    }

    /// Where this server's metadata document lives, absolute — the value
    /// RFC 9728 §5.1 wants in the `resource_metadata` parameter of a 401.
    pub fn metadata_url(&self) -> String {
        let resource = self.audiences.first().cloned().unwrap_or_default();
        let origin = resource
            .split_once("://")
            .and_then(|(scheme, rest)| {
                rest.split_once('/')
                    .map(|(host, _)| format!("{scheme}://{host}"))
            })
            .unwrap_or(resource);
        format!("{origin}{PROTECTED_RESOURCE_METADATA_PATH}")
    }

    /// Validate a presented token and map its claims onto a grant.
    ///
    /// Audience validation is the load-bearing check: a resource server that
    /// accepts any well-signed token from its issuer becomes a confused
    /// deputy for every other service using that authorization server.
    pub async fn verify(&self, token: &str, now: DateTime<Utc>) -> Result<TokenGrant, AuthError> {
        let (key, algorithm) = self.key_for(token).await?;

        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&self.audiences);
        validation.leeway = 0;
        let data = decode::<Claims>(token, &key, &validation)
            .map_err(|error| AuthError::InvalidToken(error.to_string()))?;

        let actor = ActorId::new(&data.claims.sub)
            .map_err(|error| AuthError::InvalidBinding(error.to_string()))?;
        let repo = RepoId::new(&self.repo)
            .map_err(|error| AuthError::InvalidBinding(error.to_string()))?;
        let scope: Scope = self
            .scope
            .parse()
            .map_err(|error: totem_core::ScopeParseError| {
                AuthError::InvalidBinding(error.to_string())
            })?;

        let _ = now;
        Ok(TokenGrant {
            repo,
            scope,
            actor,
            // Expiry is the token's own `exp`, enforced by `decode` above; a
            // grant derived from it carries no separate deadline.
            expires_at: None,
        })
    }

    async fn key_for(&self, token: &str) -> Result<(DecodingKey, Algorithm), AuthError> {
        match &self.keys {
            Keys::Fixed { key, algorithm } => Ok((key.clone(), *algorithm)),
            Keys::Jwks { uri, cache } => {
                let header = decode_header(token).map_err(|error| {
                    AuthError::InvalidToken(format!("unreadable header: {error}"))
                })?;
                let kid = header
                    .kid
                    .ok_or_else(|| AuthError::InvalidToken("no key id".to_string()))?;

                if let Some(key) = cache.read().await.get(&kid) {
                    return Ok((key.clone(), Algorithm::RS256));
                }

                // Unknown key id: refresh once. This is also how a rotation
                // at the authorization server is survived without a restart.
                let refreshed = fetch_jwks(uri).await?;
                let mut cache = cache.write().await;
                *cache = refreshed;
                cache
                    .get(&kid)
                    .cloned()
                    .map(|key| (key, Algorithm::RS256))
                    .ok_or_else(|| {
                        AuthError::InvalidToken(format!(
                            "no signing key {kid} at the authorization server"
                        ))
                    })
            }
        }
    }
}

async fn fetch_jwks(uri: &str) -> Result<HashMap<String, DecodingKey>, AuthError> {
    let document: JwksDocument = reqwest::get(uri)
        .await
        .map_err(|error| AuthError::InvalidBinding(format!("JWKS unreachable: {error}")))?
        .json()
        .await
        .map_err(|error| AuthError::InvalidBinding(format!("JWKS unreadable: {error}")))?;

    let mut keys = HashMap::new();
    for entry in document.keys {
        if let (Some(kid), Some(n), Some(e)) = (entry.kid, entry.n, entry.e)
            && let Ok(key) = DecodingKey::from_rsa_components(&n, &e)
        {
            keys.insert(kid, key);
        }
    }
    Ok(keys)
}

/// Build a verifier from the environment, if OAuth is configured at all.
///
/// Every value is deployment configuration, never compiled in: the same
/// binary runs on a workstation with no OAuth and on Fly with WorkOS.
pub fn from_env() -> Option<Arc<OAuthVerifier>> {
    let issuer = std::env::var("TOTEM_OAUTH_ISSUER")
        .ok()
        .filter(|v| !v.is_empty())?;
    let resource = std::env::var("TOTEM_OAUTH_RESOURCE")
        .ok()
        .filter(|v| !v.is_empty())?;
    let repo = std::env::var("TOTEM_OAUTH_REPO")
        .ok()
        .filter(|v| !v.is_empty())?;
    let scope = std::env::var("TOTEM_OAUTH_SCOPE")
        .ok()
        .filter(|v| !v.is_empty())?;
    let jwks_uri = std::env::var("TOTEM_OAUTH_JWKS_URI")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| format!("{}/oauth2/jwks", issuer.trim_end_matches('/')));

    // Accept both the path form and the bare origin: the MCP spec permits a
    // client either, and the mismatch is invisible from outside.
    let origin = resource
        .split_once("://")
        .and_then(|(scheme, rest)| {
            rest.split_once('/')
                .map(|(host, _)| format!("{scheme}://{host}"))
        })
        .unwrap_or_else(|| resource.clone());
    let mut audiences = vec![resource];
    if !audiences.contains(&origin) {
        audiences.push(origin);
    }

    Some(Arc::new(OAuthVerifier::new(
        issuer, audiences, repo, scope, jwks_uri,
    )))
}
