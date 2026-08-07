//! Minimal `text/event-stream` framing: one function, so every relay
//! endpoint this gateway grows (`GET /landscape/:repo/events`,
//! ADV-CONSOLE-003, and whatever comes after it) formats frames identically
//! instead of hand-rolling the `event:`/`data:`/blank-line shape per call
//! site.

use axum::body::Bytes;
use serde::Serialize;

/// One SSE frame naming `event` and carrying `payload` as its `data:` field.
///
/// `payload` is JSON-encoded on a single line: `serde_json::to_string` never
/// emits a literal newline (a string's own `\n` is escaped), so nothing this
/// gateway ever serializes can produce a `data:` line SSE would refuse to
/// parse — no per-field escaping is needed here.
pub(crate) fn frame(event: &str, payload: &impl Serialize) -> Bytes {
    let json = serde_json::to_string(payload).expect("payload serialises");
    Bytes::from(format!("event: {event}\ndata: {json}\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_names_its_event_and_carries_json_data_terminated_by_a_blank_line() {
        let bytes = frame("landscape", &serde_json::json!({ "repo": "058-totem" }));
        let text = String::from_utf8(bytes.to_vec()).expect("frame is utf-8");
        assert_eq!(text, "event: landscape\ndata: {\"repo\":\"058-totem\"}\n\n");
    }
}
