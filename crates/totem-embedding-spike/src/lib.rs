//! ADV-STORE-003 investigation spike: throwaway code comparing embedding
//! candidates for Totem memory records. Not production code — no dependent
//! crate should import this one. See
//! `docs/tech-direction/embeddings.md` for the findings this spike produced.

/// One memory-sized text from the toy corpus, tagged with the category it
/// would carry as a real Totem record (Solution Intent §2.1).
#[derive(Debug, Clone, Copy)]
pub struct MemoryText {
    pub id: &'static str,
    pub category: &'static str,
    pub body: &'static str,
}

/// A hand-labeled query: which corpus entry should rank first.
///
/// `lexical_overlap` marks whether the query shares surface words with its
/// expected match. Queries with `false` are deliberately paraphrased so a
/// purely lexical embedder has nothing to key off — real agent recall
/// queries look like these, not like keyword search.
#[derive(Debug, Clone, Copy)]
pub struct LabeledQuery {
    pub query: &'static str,
    pub expected_top1: &'static str,
    pub lexical_overlap: bool,
}

/// Ten memory-sized texts spanning three categories, standing in for real
/// Totem records until real usage data exists (tracked as a risk below).
pub fn corpus() -> Vec<MemoryText> {
    vec![
        MemoryText {
            id: "knowledge-pnpm",
            category: "knowledge",
            body: "The team prefers pnpm over npm for all JavaScript projects.",
        },
        MemoryText {
            id: "knowledge-kvmem",
            category: "knowledge",
            body: "SurrealDB's embedded kv-mem engine is used for all workspace \
                   tests; no server is available in the cloud sandbox.",
        },
        MemoryText {
            id: "instructions-fmt",
            category: "instructions",
            body: "Always run cargo fmt --check before opening a pull request.",
        },
        MemoryText {
            id: "instructions-no-master-push",
            category: "instructions",
            body: "Never push directly to master; open a pull request from an \
                   advance branch instead.",
        },
        MemoryText {
            id: "context-embedding-investigation",
            category: "context",
            body: "Currently investigating the embedding provider decision for \
                   ADV-STORE-002.",
        },
        MemoryText {
            id: "context-gateway-not-created",
            category: "context",
            body: "The gateway crate has not been created yet; it is scheduled \
                   for ADV-GATEWAY-001.",
        },
        MemoryText {
            id: "knowledge-scope-isolation",
            category: "knowledge",
            body: "Scope isolation must be enforced at the store layer, not \
                   filtered in the gateway.",
        },
        MemoryText {
            id: "instructions-episodic-append-only",
            category: "instructions",
            body: "Episodic records are append-only; no code path may update \
                   or delete one.",
        },
        MemoryText {
            id: "knowledge-edition-2024",
            category: "knowledge",
            body: "The workspace uses Rust edition 2024 and forbids unsafe code.",
        },
        MemoryText {
            id: "context-ci-clippy-failure",
            category: "context",
            body: "The last CI run failed because clippy flagged an unused \
                   import in totem-core.",
        },
    ]
}

/// Five queries. The first four share vocabulary with their expected match
/// (`lexical_overlap: true`); the fifth is a genuine paraphrase of
/// `knowledge-scope-isolation` with no shared content words, marked
/// `lexical_overlap: false`.
pub fn labeled_queries() -> Vec<LabeledQuery> {
    vec![
        LabeledQuery {
            query: "Should JavaScript projects use npm or pnpm?",
            expected_top1: "knowledge-pnpm",
            lexical_overlap: true,
        },
        LabeledQuery {
            query: "Is a live SurrealDB server available for testing here?",
            expected_top1: "knowledge-kvmem",
            lexical_overlap: true,
        },
        LabeledQuery {
            query: "Why shouldn't I push directly to master?",
            expected_top1: "instructions-no-master-push",
            lexical_overlap: true,
        },
        LabeledQuery {
            query: "Can episodic memory be edited?",
            expected_top1: "instructions-episodic-append-only",
            lexical_overlap: true,
        },
        LabeledQuery {
            query: "How do we keep private data out of shared scopes?",
            expected_top1: "knowledge-scope-isolation",
            lexical_overlap: false,
        },
    ]
}

/// A candidate embedding provider under comparison.
pub trait EmbeddingProvider {
    fn name(&self) -> &'static str;
    fn embed(&self, text: &str) -> Vec<f32>;
}

/// Local candidate: a hashing-trick character-trigram bag, L2-normalized.
/// Zero network calls, zero cost, deterministic — a purely lexical baseline
/// with no semantic model behind it.
#[derive(Debug, Clone, Copy)]
pub struct HashingEmbedder {
    pub dims: usize,
}

impl HashingEmbedder {
    /// `dims` is clamped to at least 1 here, not in `embed`, so the stored
    /// field always matches the length of every vector this embedder
    /// produces.
    pub fn new(dims: usize) -> Self {
        Self { dims: dims.max(1) }
    }
}

impl EmbeddingProvider for HashingEmbedder {
    fn name(&self) -> &'static str {
        "local-hashing-trigram"
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0f32; self.dims];
        let lower = text.to_lowercase();
        let chars: Vec<char> = lower.chars().collect();

        if chars.len() < 3 {
            bump(&mut vector, fnv1a(&lower));
        } else {
            for window in chars.windows(3) {
                let trigram: String = window.iter().collect();
                bump(&mut vector, fnv1a(&trigram));
            }
        }

        l2_normalize(&mut vector);
        vector
    }
}

fn bump(vector: &mut [f32], hash: u64) {
    let len = vector.len() as u64;
    let index = (hash % len) as usize;
    vector[index] += 1.0;
}

/// FNV-1a: no external dependency needed for a throwaway hashing trick.
fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn l2_normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in vector.iter_mut() {
            *x /= norm;
        }
    }
}

/// Cosine similarity. Both inputs are expected L2-normalized (as
/// `HashingEmbedder::embed` produces), in which case this is a plain dot
/// product.
///
/// `zip` would otherwise silently truncate to the shorter vector on a
/// dimension mismatch and return a score that looks valid but isn't.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(
        a.len(),
        b.len(),
        "cosine similarity between vectors of different dimensionality ({} vs {}) is meaningless",
        a.len(),
        b.len()
    );
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Outcome of ranking the whole corpus against one labeled query.
#[derive(Debug, Clone)]
pub struct RetrievalResult {
    pub query: &'static str,
    pub expected: &'static str,
    pub actual_top1: String,
    pub correct: bool,
    /// 1-based rank of the expected id in the similarity-sorted corpus, if present.
    pub rank_of_expected: Option<usize>,
}

/// Embeds the corpus and every query with `provider`, ranks the corpus by
/// cosine similarity for each query, and reports whether the expected match
/// landed first.
pub fn evaluate_retrieval(
    provider: &dyn EmbeddingProvider,
    corpus: &[MemoryText],
    queries: &[LabeledQuery],
) -> Vec<RetrievalResult> {
    let corpus_embeddings: Vec<(&'static str, Vec<f32>)> = corpus
        .iter()
        .map(|memory| (memory.id, provider.embed(memory.body)))
        .collect();

    queries
        .iter()
        .map(|labeled| {
            let query_vector = provider.embed(labeled.query);
            let mut scored: Vec<(&'static str, f32)> = corpus_embeddings
                .iter()
                .map(|(id, vector)| (*id, cosine(&query_vector, vector)))
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            let rank_of_expected = scored
                .iter()
                .position(|(id, _)| *id == labeled.expected_top1)
                .map(|zero_based| zero_based + 1);
            let actual_top1 = scored
                .first()
                .map(|(id, _)| (*id).to_string())
                .unwrap_or_default();

            RetrievalResult {
                query: labeled.query,
                expected: labeled.expected_top1,
                correct: actual_top1 == labeled.expected_top1,
                actual_top1,
                rank_of_expected,
            }
        })
        .collect()
}
