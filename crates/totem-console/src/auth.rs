//! Signing a human in through AuthKit (ADV-GATEWAY-010).
//!
//! The console is an OAuth 2.1 **public client** doing authorization-code with
//! PKCE. No client secret exists — one placed in a wasm bundle would be
//! readable by anyone who opened developer tools, and PKCE is precisely the
//! mechanism that removes the need for it.
//!
//! Just as importantly, this keeps the **gateway a pure resource server**
//! (ADV-GATEWAY-013's whole thesis): the browser talks to the authorization
//! server directly and presents the resulting token as a bearer, so the
//! gateway validates humans and agents through one code path and gains no
//! second credential model.
//!
//! # Where the token lives
//!
//! `sessionStorage`, not `localStorage`: it is scoped to the tab and cleared
//! when the tab closes, which is a smaller window than "until someone clears
//! site data". Any browser-held token is reachable by script running on the
//! page — that is inherent to a browser client, not something this module
//! can fix, so the console must stay free of third-party script.

use base64::Engine as _;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Where the console keeps its state between the redirect out and back.
/// Browser-only: the host build has no session storage, and the pure PKCE
/// helpers above it are what the host tests exercise.
#[cfg(target_arch = "wasm32")]
const VERIFIER_KEY: &str = "totem.pkce.verifier";
#[cfg(target_arch = "wasm32")]
const STATE_KEY: &str = "totem.pkce.state";
#[cfg(target_arch = "wasm32")]
const TOKEN_KEY: &str = "totem.access_token";

/// Runtime configuration, served by the gateway rather than compiled in.
///
/// The same wasm bundle then runs on a workstation and on the deployment
/// without a rebuild, and the AuthKit domain or client id can change without
/// one either.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ConsoleConfig {
    /// The authorization server's issuer URL.
    pub issuer: String,
    /// This console's OAuth client id. Public by construction.
    pub client_id: String,
    /// Where the authorization server sends the browser back.
    pub redirect_uri: String,
    /// The resource the access token must be audienced for (RFC 8707), so
    /// the gateway's audience check accepts it.
    pub resource: String,
}

/// What the console knows about the signed-in human.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    /// No token held; the UI shows a sign-in prompt rather than empty views.
    SignedOut,
    /// A bearer token to present on every API call.
    SignedIn(String),
}

fn base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A PKCE verifier and the challenge derived from it.
///
/// Split out and pure so it can be tested on the host: the flow around it
/// needs a browser, this does not.
pub fn challenge_for(verifier: &str) -> String {
    base64url(&Sha256::digest(verifier.as_bytes()))
}

/// A fresh high-entropy verifier.
pub fn new_verifier() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the browser provides randomness");
    base64url(&bytes)
}

/// Build the authorization URL to send the browser to.
///
/// `resource` is included per RFC 8707 because the gateway validates the
/// token's audience; without it the authorization server may issue a token
/// this resource server correctly refuses, and the failure would look like a
/// login problem rather than an audience mismatch.
pub fn authorize_url(config: &ConsoleConfig, challenge: &str, state: &str) -> String {
    let encode = |value: &str| {
        percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
    };
    format!(
        "{issuer}/oauth2/authorize?response_type=code&client_id={client}\
         &redirect_uri={redirect}&code_challenge={challenge}&code_challenge_method=S256\
         &state={state}&resource={resource}",
        issuer = config.issuer.trim_end_matches('/'),
        client = encode(&config.client_id),
        redirect = encode(&config.redirect_uri),
        challenge = encode(challenge),
        state = encode(state),
        resource = encode(&config.resource),
    )
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.session_storage().ok()?
    }

    fn get(key: &str) -> Option<String> {
        storage()?.get_item(key).ok()?
    }

    fn set(key: &str, value: &str) {
        if let Some(storage) = storage() {
            let _ = storage.set_item(key, value);
        }
    }

    fn remove(key: &str) {
        if let Some(storage) = storage() {
            let _ = storage.remove_item(key);
        }
    }

    /// The token held for this tab, if any.
    pub fn stored_token() -> Option<String> {
        get(TOKEN_KEY)
    }

    /// Forget the token — the sign-out path.
    pub fn clear_token() {
        remove(TOKEN_KEY);
    }

    /// Start the flow: mint a verifier and state, remember them, and send the
    /// browser to the authorization server.
    pub fn begin_sign_in(config: &ConsoleConfig) {
        let verifier = new_verifier();
        let state = new_verifier();
        set(VERIFIER_KEY, &verifier);
        set(STATE_KEY, &state);
        let url = authorize_url(config, &challenge_for(&verifier), &state);
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_href(&url);
        }
    }

    /// The `code` and `state` on the current URL, if this is a callback.
    pub fn callback_params() -> Option<(String, String)> {
        let search = web_sys::window()?.location().search().ok()?;
        let query = search.trim_start_matches('?');
        let mut code = None;
        let mut state = None;
        for pair in query.split('&') {
            match pair.split_once('=') {
                Some(("code", value)) => code = Some(value.to_string()),
                Some(("state", value)) => state = Some(value.to_string()),
                _ => {}
            }
        }
        Some((code?, state?))
    }

    /// Exchange an authorization code for an access token.
    ///
    /// The `state` is compared against the value stored before the redirect:
    /// a mismatch means this callback did not originate from this tab's
    /// sign-in, and the code is discarded rather than exchanged.
    pub async fn complete_sign_in(
        _config: &ConsoleConfig,
        code: &str,
        returned_state: &str,
    ) -> Result<String, String> {
        let expected = get(STATE_KEY).ok_or("no sign-in is in progress in this tab")?;
        if expected != returned_state {
            remove(VERIFIER_KEY);
            remove(STATE_KEY);
            return Err("the sign-in response did not match this tab's request".to_string());
        }
        let verifier = get(VERIFIER_KEY).ok_or("the sign-in verifier is missing")?;

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
        }

        // Exchanged through the gateway's own origin, not directly against
        // AuthKit (ADV-GATEWAY-010). AuthKit answers the CORS preflight with
        // `Access-Control-Allow-Origin: *` but omits the header from the
        // actual response, so the browser completes the request and then
        // refuses to let this page read it. No client-side change can fix a
        // missing response header; the relay is same-origin and therefore
        // unaffected. PKCE still proves the exchange — the verifier below
        // never leaves this tab except to our own server.
        let response = gloo_net::http::Request::post("/console/token")
            .header("content-type", "application/json")
            .body(serde_json::json!({ "code": code, "code_verifier": verifier }).to_string())
            .map_err(|error| error.to_string())?
            .send()
            .await
            .map_err(|error| error.to_string())?;

        remove(VERIFIER_KEY);
        remove(STATE_KEY);

        if !response.ok() {
            return Err(format!(
                "the token exchange was refused: {}",
                response.status()
            ));
        }
        let token: TokenResponse = response.json().await.map_err(|error| error.to_string())?;
        set(TOKEN_KEY, &token.access_token);
        Ok(token.access_token)
    }
}

#[cfg(target_arch = "wasm32")]
pub use browser::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ConsoleConfig {
        ConsoleConfig {
            issuer: "https://decent-genius-72-staging.authkit.app".to_string(),
            client_id: "client_01TEST".to_string(),
            redirect_uri: "https://totem-dev.fly.dev/callback".to_string(),
            resource: "https://totem-dev.fly.dev".to_string(),
        }
    }

    #[test]
    fn the_challenge_is_the_sha256_of_the_verifier_base64url_unpadded() {
        // RFC 7636's own worked example, so this is checked against the spec
        // rather than against itself.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            challenge_for(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_verifier_is_long_and_never_repeats() {
        let first = new_verifier();
        let second = new_verifier();
        assert_ne!(first, second, "verifiers must not repeat");
        assert!(first.len() >= 43, "too little entropy: {first}");
        assert!(
            !first.contains('+') && !first.contains('/') && !first.contains('='),
            "must be base64url without padding: {first}"
        );
    }

    #[test]
    fn the_authorize_url_carries_pkce_and_the_resource() {
        let url = authorize_url(&config(), "a-challenge", "a-state");

        for expected in [
            "response_type=code",
            "code_challenge=a%2Dchallenge",
            "code_challenge_method=S256",
            "state=a%2Dstate",
            "client_id=client%5F01TEST",
        ] {
            assert!(url.contains(expected), "missing {expected} in {url}");
        }
        assert!(
            url.contains("resource=https%3A%2F%2Ftotem%2Ddev%2Efly%2Edev"),
            "RFC 8707 resource missing — the gateway's audience check would \
             refuse the resulting token, and it would look like a login bug: {url}"
        );
    }

    #[test]
    fn the_authorize_url_never_carries_a_secret() {
        let url = authorize_url(&config(), "c", "s");
        assert!(
            !url.contains("client_secret"),
            "a public client must not send a secret: {url}"
        );
    }
}
