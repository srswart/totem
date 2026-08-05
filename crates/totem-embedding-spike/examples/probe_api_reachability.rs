//! Manual probe: is any embedding-API host reachable from this sandbox?
//! NOT part of `cargo test --workspace` — like
//! `totem-store-spike`'s `server-parity` test, a default test run must never
//! depend on live network conditions. Run manually and the output pasted
//! into docs/tech-direction/embeddings.md:
//! `cargo run -p totem-embedding-spike --example probe_api_reachability`

use std::time::{Duration, Instant};
use ureq::{Agent, AgentBuilder, Proxy};

fn build_agent() -> Agent {
    let mut builder = AgentBuilder::new().timeout(Duration::from_secs(8));
    if let Ok(proxy_url) = std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("https_proxy")) {
        match Proxy::new(&proxy_url) {
            Ok(proxy) => builder = builder.proxy(proxy),
            Err(err) => eprintln!(
                "warning: could not parse HTTPS_PROXY={proxy_url:?} as a ureq proxy: {err}"
            ),
        }
    }
    builder.build()
}

fn probe(agent: &Agent, label: &str, url: &str) {
    let start = Instant::now();
    let outcome = agent.get(url).call();
    let elapsed = start.elapsed();
    match outcome {
        Ok(response) => {
            println!(
                "  {label:<12} {url:<45} -> HTTP {} in {elapsed:?}",
                response.status()
            );
        }
        Err(ureq::Error::Status(code, _)) => {
            println!("  {label:<12} {url:<45} -> HTTP {code} in {elapsed:?}");
        }
        Err(ureq::Error::Transport(transport)) => {
            println!("  {label:<12} {url:<45} -> transport error in {elapsed:?}: {transport}");
        }
    }
}

fn main() {
    let agent = build_agent();
    println!("== embedding API host reachability (HTTPS_PROXY honored if set) ==");
    // Control: a host the sandbox's egress policy is documented to allow
    // (see /root/.ccr/README.md's noProxy list), to prove the agent and
    // network path work at all before trusting a "blocked" reading below.
    probe(&agent, "control", "https://index.crates.io/config.json");
    probe(&agent, "openai", "https://api.openai.com/v1/embeddings");
    probe(&agent, "cohere", "https://api.cohere.com/v1/embed");
    probe(&agent, "voyageai", "https://api.voyageai.com/v1/embeddings");
    probe(&agent, "huggingface", "https://huggingface.co/");
}
