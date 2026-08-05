//! ADV-GATEWAY-005 investigation spike.
//!
//! One `Echo` tool server, served over both transports `totem-gateway` needs
//! (Solution Intent §3.1): stdio for desktop harnesses, streamable HTTP for
//! cloud ones. `src/bin/echo_stdio.rs` and `src/bin/echo_streamhttp.rs` run
//! it; `tests/` drive a real rmcp client against each.

use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EchoParams {
    #[schemars(description = "Text to echo back unchanged")]
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Echo;

#[tool_router(server_handler)]
impl Echo {
    #[tool(description = "Echo the given text back unchanged")]
    fn echo(&self, Parameters(EchoParams { text }): Parameters<EchoParams>) -> String {
        text
    }
}
