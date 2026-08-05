//! Prints the retrieval readout and rough latency for the local hashing
//! candidate, quoted in docs/tech-direction/embeddings.md. Regenerate with:
//! `cargo run -p totem-embedding-spike --example embed_corpus`

use std::time::Instant;
use totem_embedding_spike::{
    EmbeddingProvider, HashingEmbedder, corpus, evaluate_retrieval, labeled_queries,
};

fn main() {
    let provider = HashingEmbedder::new(256);
    let corpus = corpus();
    let queries = labeled_queries();

    println!("== retrieval readout: {} ==", provider.name());
    for result in evaluate_retrieval(&provider, &corpus, &queries) {
        println!(
            "  {:<55} expected={:<32} got={:<32} correct={} rank_of_expected={:?}",
            format!("{:?}", result.query),
            result.expected,
            result.actual_top1,
            result.correct,
            result.rank_of_expected
        );
    }

    let iterations = 1000;
    let sample_texts: Vec<&str> = corpus
        .iter()
        .map(|m| m.body)
        .chain(queries.iter().map(|q| q.query))
        .collect();

    let start = Instant::now();
    for _ in 0..iterations {
        for text in &sample_texts {
            std::hint::black_box(provider.embed(text));
        }
    }
    let elapsed = start.elapsed();
    let total_calls = iterations * sample_texts.len();
    println!(
        "\n== latency: {} calls in {:?} ({:.3} µs/call average, single-threaded, no I/O) ==",
        total_calls,
        elapsed,
        elapsed.as_secs_f64() * 1_000_000.0 / total_calls as f64
    );
}
