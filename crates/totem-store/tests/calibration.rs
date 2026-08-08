//! The calibration corpus artifact must load, verify, and mean something
//! (ADV-STORE-009).
//!
//! These tests guard the two properties the advance calls non-negotiable —
//! records carry economics, and the golden queries ship with the records —
//! plus the integrity check that makes "measured against calibration-v1" a
//! statement anyone can verify rather than a claim about a file that may
//! since have been edited.

use std::path::PathBuf;

use totem_store::calibration::Corpus;

fn artifact_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpora/calibration-v1.json")
        .canonicalize()
        .expect("the calibration artifact is committed at corpora/calibration-v1.json")
}

fn corpus() -> Corpus {
    Corpus::load(artifact_path()).expect("the committed artifact loads and verifies")
}

#[test]
fn the_committed_artifact_loads_and_matches_its_checksum() {
    let corpus = corpus();
    assert_eq!(corpus.manifest.id, "calibration");
    assert_eq!(corpus.manifest.version, "v1");
    assert!(
        !corpus.records.is_empty() && !corpus.queries.is_empty(),
        "an artifact with no records or no queries calibrates nothing"
    );
}

#[test]
fn an_edited_artifact_is_refused_rather_than_warned_about() {
    // The whole point of the checksum: a corpus loaded from disk can be
    // edited until an evaluation passes, and a tolerated mismatch is the same
    // as no checksum at all.
    let mut corpus = corpus();
    corpus.records[0].body.push_str(" (tampered)");

    let error = corpus
        .verify()
        .expect_err("a corpus whose contents changed must not verify");
    let message = error.to_string();
    assert!(
        message.contains("does not match its checksum"),
        "the error must say what is wrong, not merely that something is: {message}"
    );
}

#[test]
fn every_record_carries_explicit_economics() {
    // ADV-CORE-008: a corpus whose records all have identical pristine
    // economics cannot fail, because the three non-relevance terms are then
    // constant and cancel. `eval_quality` scored a perfect 1.0 against a
    // ranker that provably ignored the query.
    let corpus = corpus();

    let distinct_value: std::collections::HashSet<String> = corpus
        .records
        .iter()
        .map(|record| format!("{:.3}", record.economics.value_score))
        .collect();
    assert!(
        distinct_value.len() > 1,
        "every record has the same value_score — the corpus cannot distinguish a ranker \
         that weighs history from one that ignores the query: {distinct_value:?}"
    );

    let with_history = corpus
        .records
        .iter()
        .filter(|record| record.economics.use_count > 0)
        .count();
    assert!(
        with_history >= 3,
        "only {with_history} records have ever been used; the value loop cannot be \
         exercised against an estate that has no history"
    );

    let ever_used = corpus
        .records
        .iter()
        .filter(|record| record.economics.last_used_at.is_some())
        .count();
    assert!(
        ever_used >= 3,
        "currency is only testable against records with a last_used_at, and {ever_used} have one"
    );
}

#[test]
fn every_golden_query_names_records_that_exist() {
    // Queries refer to records by key rather than by repeating their bodies,
    // which is what stops the two drifting apart — but it makes a typo a
    // silently-passing assertion unless something checks.
    let corpus = corpus();
    let dangling = corpus.dangling_keys();
    assert!(
        dangling.is_empty(),
        "these golden queries reference records the corpus does not define: {dangling:?}"
    );
}

#[test]
fn every_golden_query_says_what_it_is_for() {
    // A fixture whose purpose is not written down becomes untouchable the
    // moment it fails: nobody can tell a real regression from a query that
    // was always wrong.
    let corpus = corpus();
    for query in &corpus.queries {
        assert!(
            query.rationale.len() > 40,
            "{} has no meaningful rationale — when it fails, its reader needs to know \
             what it was asserting",
            query.name
        );
    }
}

#[test]
fn the_corpus_discriminates_rather_than_merely_covering() {
    // The advance's warning: a large corpus of near-identical synthetic
    // sentences calibrates nothing. What makes ranking testable is topic
    // clusters holding genuine near-misses — records that *should* lose to a
    // better match on the same subject.
    let corpus = corpus();

    let mut by_cluster: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for record in corpus.records.iter().filter_map(|r| {
        r.cluster
            .as_deref()
            .map(|cluster| (cluster, r))
            .map(|(cluster, _)| cluster)
    }) {
        *by_cluster.entry(record).or_default() += 1;
    }

    let discriminating = by_cluster.values().filter(|count| **count >= 2).count();
    assert!(
        discriminating >= 3,
        "only {discriminating} topic clusters hold more than one record; without \
         near-misses a query only tests that the embedder can tell one subject from \
         another, which it always can: {by_cluster:?}"
    );

    assert!(
        corpus
            .queries
            .iter()
            .any(|query| !query.expect_absent.is_empty()),
        "no query asserts a record must be ABSENT — the assertion a top-1 check cannot \
         make, and the reason near-misses are worth authoring"
    );
}

/// Re-stamp the artifact's checksum after editing it, in place.
///
/// ```sh
/// cargo test -p totem-store --test calibration -- --ignored restamp
/// ```
///
/// `#[ignore]`d because it *writes* — an ordinary test run must never make a
/// mismatched corpus pass by rewriting the thing it was supposed to check.
///
/// Deliberately a test rather than a `[[bin]]`: cargo auto-discovers binaries
/// as targets, so a new one would change the resolved manifest, change
/// `recipe.json`, and cost a full dependency rebuild on the next deploy
/// (ADV-INFRA-008). A tool used a few times a year is not worth that.
///
/// **Rust computes the authoritative checksum**, not whatever generated the
/// file. `serde_json` orders keys by the struct's field declarations, which no
/// external generator can be assumed to reproduce — the first attempt at this
/// artifact was stamped by a Python script and disagreed on the first run.
#[test]
#[ignore = "writes to the artifact; run explicitly with --ignored"]
fn restamp() {
    let path = artifact_path();
    let text = std::fs::read_to_string(&path).expect("the artifact is readable");
    let mut corpus: Corpus = serde_json::from_str(&text).expect("the artifact parses");

    let before = corpus.manifest.checksum.clone();
    let actual = corpus.compute_checksum();
    if before == actual {
        println!("checksum already correct: {actual}");
        return;
    }

    corpus.manifest.checksum = actual.clone();
    let mut rendered = serde_json::to_string_pretty(&corpus).expect("the corpus serializes");
    rendered.push('\n');
    std::fs::write(&path, rendered).expect("the artifact is writable");
    println!(
        "restamped {} -> {}",
        &before[..16.min(before.len())],
        &actual[..16]
    );
}

/// The corpus seeds, and its golden queries run against what was seeded.
///
/// The point of the artifact is that it can be *scored*, and an artifact that
/// parses but cannot be seeded and queried is a document rather than a test.
#[tokio::test]
async fn the_corpus_seeds_and_its_golden_queries_run() {
    let corpus = corpus();
    let store = totem_store::Store::in_memory()
        .await
        .expect("the embedded engine connects");
    store.migrate().await.expect("migrations apply");
    let embedder = totem_store::DeterministicEmbedder::new();

    let seeded = corpus
        .seed(&store, &embedder)
        .await
        .expect("every record in the artifact is seedable");
    assert_eq!(seeded, corpus.records.len());

    // Score every query, and report the whole picture rather than the first
    // failure: which queries a ranking change breaks is the signal, and a
    // test that stops at one hides the shape of a regression.
    let mut outcomes = Vec::new();
    for query in &corpus.queries {
        let results = corpus
            .run_query(&store, &embedder, query)
            .await
            .expect("a golden query runs");
        // Identify a returned record by **body and scope**, not body alone.
        //
        // The leak-bait pair is byte-identical at two private actor scopes —
        // deliberately, since that is what makes it prove isolation rather
        // than prove the embedder can read. Matching on body alone cannot
        // tell the two apart, and the first version of this harness reported
        // the reader's *own* copy as the other actor's, i.e. a scope leak
        // that had not happened. A false security alarm is worse than a
        // missed one: it burns the credibility of every later report.
        let seen: Vec<(&str, String)> = results
            .iter()
            .map(|record| (record.content.body.as_str(), record.scope.to_string()))
            .collect();
        let is = |record: &totem_store::calibration::CorpusRecord, entry: &(&str, String)| {
            entry.0 == record.body && entry.1 == record.scope
        };

        let mut failures = Vec::new();
        if let Some(key) = &query.expect_top {
            let expected = corpus.record(key).expect("checked by another test");
            match seen.first() {
                Some(top) if is(expected, top) => {}
                Some(top) => failures.push(format!("top was {:?}, wanted {key}", top.0)),
                None => failures.push(format!("no results at all, wanted {key}")),
            }
        }
        for key in &query.expect_present {
            let expected = corpus.record(key).expect("checked by another test");
            if !seen.iter().any(|entry| is(expected, entry)) {
                failures.push(format!("{key} missing"));
            }
        }
        for key in &query.expect_absent {
            let unwanted = corpus.record(key).expect("checked by another test");
            if seen.iter().any(|entry| is(unwanted, entry)) {
                failures.push(format!("{key} present but must not be"));
            }
        }
        outcomes.push((query.name.as_str(), failures));
    }

    let failed: Vec<&(&str, Vec<String>)> = outcomes
        .iter()
        .filter(|(_, failures)| !failures.is_empty())
        .collect();

    // **This corpus is expected to fail today**, and that is the point of
    // ADV-STORE-009: ADV-GATEWAY-016 measured the nearest record losing in 4
    // of 6 deployed queries, on category weight alone. A corpus that passed
    // against today's ranker would be reproducing the defect this one exists
    // to expose (ADV-CORE-008's corpus scored a perfect 1.0 against a ranker
    // that ignored the query entirely).
    //
    // So the assertion is on *executability*, not on the score. Turning these
    // green is ADV-CORE-005's job, once the ranking fix lands.
    println!(
        "calibration-v1: {}/{} queries pass",
        outcomes.len() - failed.len(),
        outcomes.len()
    );
    for (name, failures) in &failed {
        println!("  FAIL {name}: {}", failures.join("; "));
    }

    assert_eq!(
        outcomes.len(),
        corpus.queries.len(),
        "every golden query must at least execute"
    );
}
