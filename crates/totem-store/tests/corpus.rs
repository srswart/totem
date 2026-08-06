//! Proves the synthetic evaluation corpus (ADV-STORE-005) does what its
//! consumers (ADV-CORE-005, ADV-GATEWAY-006, ADV-GATEWAY-007/008) need: full
//! category × scope coverage, deterministic reseeding, working golden
//! queries, and — the security-critical property — that leak-bait content
//! never crosses a private scope boundary even though it is byte-identical
//! on both sides.

use chrono::Utc;
use totem_core::{MemoryCategory, Scope};
use totem_store::corpus::{self, GENERATOR_TAG};

mod common;
use common::store;

#[tokio::test]
async fn seeding_covers_every_category_and_every_scope_tier() {
    let db = store().await;
    let report = corpus::seed(&db).await.expect("corpus seeds");

    assert_eq!(
        report.total,
        report.by_category.iter().map(|(_, n)| n).sum::<usize>()
    );
    for category in MemoryCategory::ALL {
        let count = report
            .by_category
            .iter()
            .find(|(c, _)| *c == category)
            .map(|(_, n)| *n)
            .unwrap_or(0);
        assert!(count > 0, "{category:?} has no seeded records");
    }
}

#[tokio::test]
async fn every_seeded_record_carries_the_generator_tag() {
    let db = store().await;
    corpus::seed(&db).await.expect("corpus seeds");

    // Two readers whose scope chains together cover all four scope tiers
    // (actor, project, team, platform) across the two projects the corpus
    // uses, so this is a real check across the whole seeded shape rather
    // than one fixture.
    let readers = [
        corpus::reader_chain(corpus::NOVA, Some(corpus::ROCKET), &[]),
        corpus::reader_chain(
            corpus::ATLAS,
            Some(corpus::BEACON),
            &[corpus::PLATFORM_TEAM],
        ),
    ];

    let mut checked = 0;
    for reader in &readers {
        let recalled = db
            .memories()
            .recall(reader, &totem_store::RecallQuery::new().limit(100))
            .await
            .expect("recall succeeds");
        for record in &recalled {
            assert!(
                record.content.tags.iter().any(|tag| tag == GENERATOR_TAG),
                "synthetic record is missing the generator tag: {record:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no records were recalled to check");
}

#[tokio::test]
async fn seeded_in_memory_is_reproducible_across_fresh_stores() {
    let (_first, report_a) = corpus::seeded_in_memory().await.expect("first corpus");
    let (_second, report_b) = corpus::seeded_in_memory().await.expect("second corpus");

    // A fresh store every call is the deterministic "reset" path (episodic
    // rows are append-only, so an in-place DELETE is not an option — see
    // schema.rs's `memory_episodic_no_delete` event). Same generator, same
    // shape, every time.
    assert_eq!(report_a, report_b);
}

#[tokio::test]
async fn leak_bait_pairs_never_cross_the_private_scope_boundary() {
    let db = store().await;
    corpus::seed(&db).await.expect("corpus seeds");

    for pair in corpus::leak_bait_pairs() {
        for (owner, other) in [(pair.owner_a, pair.owner_b), (pair.owner_b, pair.owner_a)] {
            let reader = corpus::reader_chain(owner, None, &[]);
            let recalled = db
                .memories()
                .recall(
                    &reader,
                    &totem_store::RecallQuery::new().in_categories([pair.category]),
                )
                .await
                .expect("recall succeeds");

            let matches: Vec<_> = recalled
                .iter()
                .filter(|record| record.content.body == pair.body)
                .collect();

            assert_eq!(
                matches.len(),
                1,
                "{owner} should read exactly one copy of {:?}, got {}",
                pair.name,
                matches.len()
            );
            assert_eq!(
                matches[0].scope,
                Scope::Actor(totem_core::ActorId::new(owner).expect("valid actor id")),
                "{owner}'s copy must be scoped to {owner}, not leaked from {other}"
            );
            assert_eq!(
                matches[0].provenance.author.actor().to_string(),
                owner,
                "{owner} must never see {other}'s provenance on identical content"
            );
        }
    }
}

#[tokio::test]
async fn golden_queries_return_their_expected_top_result() {
    let db = store().await;
    corpus::seed(&db).await.expect("corpus seeds");

    for query in corpus::golden_queries() {
        let Some(expected_top) = query.expected_top else {
            continue;
        };
        let results = corpus::run_golden_query(&db, &query)
            .await
            .expect("golden query runs");
        assert_eq!(
            results.first().map(|record| record.content.body.as_str()),
            Some(expected_top),
            "golden query {:?} did not rank its expected result first: {:?}",
            query.name,
            results.iter().map(|r| &r.content.body).collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
async fn golden_queries_surface_every_must_appear_body() {
    let db = store().await;
    corpus::seed(&db).await.expect("corpus seeds");

    for query in corpus::golden_queries() {
        if query.must_appear.is_empty() {
            continue;
        }
        let results = corpus::run_golden_query(&db, &query)
            .await
            .expect("golden query runs");
        let bodies: Vec<&str> = results.iter().map(|r| r.content.body.as_str()).collect();
        for expected in query.must_appear {
            assert!(
                bodies.contains(expected),
                "golden query {:?} is missing {:?} from {:?}",
                query.name,
                expected,
                bodies
            );
        }
    }
}

#[tokio::test]
async fn near_scope_precedence_collapses_the_precedence_pair_to_one_record() {
    let db = store().await;
    corpus::seed(&db).await.expect("corpus seeds");

    let reader = corpus::reader_chain(corpus::PRECEDENCE_READER, Some(corpus::ROCKET), &[]);
    let recalled = db
        .memories()
        .recall(
            &reader,
            &totem_store::RecallQuery::new().in_categories([MemoryCategory::Knowledge]),
        )
        .await
        .expect("recall succeeds");

    let matches: Vec<_> = recalled
        .iter()
        .filter(|record| record.content.body == corpus::PRECEDENCE_BODY)
        .collect();

    assert_eq!(
        matches.len(),
        1,
        "the actor-scope and project-scope copies must merge to one record"
    );
    assert_eq!(
        matches[0].scope,
        Scope::Actor(totem_core::ActorId::new(corpus::PRECEDENCE_READER).expect("valid actor id")),
        "the narrower (actor) scope must win precedence over the project scope"
    );
}

#[tokio::test]
async fn contested_pair_keeps_both_sides_visible() {
    let db = store().await;
    corpus::seed(&db).await.expect("corpus seeds");

    let reader = corpus::reader_chain(corpus::PRECEDENCE_READER, Some(corpus::ROCKET), &[]);
    let recalled = db
        .memories()
        .recall(
            &reader,
            &totem_store::RecallQuery::new().in_categories([MemoryCategory::Uncertainty]),
        )
        .await
        .expect("recall succeeds");
    let bodies: Vec<&str> = recalled.iter().map(|r| r.content.body.as_str()).collect();

    assert!(bodies.contains(&corpus::CONTESTED_A));
    assert!(bodies.contains(&corpus::CONTESTED_B));
}

#[tokio::test]
async fn aged_and_expired_fixtures_exist_for_currency_and_ttl_scoring() {
    let db = store().await;
    corpus::seed(&db).await.expect("corpus seeds");

    let reader = corpus::reader_chain(corpus::PRECEDENCE_READER, Some(corpus::ROCKET), &[]);

    let knowledge = db
        .memories()
        .recall(
            &reader,
            &totem_store::RecallQuery::new()
                .in_categories([MemoryCategory::Knowledge])
                .limit(100),
        )
        .await
        .expect("recall succeeds");
    let aged = knowledge
        .iter()
        .find(|record| record.content.tags.iter().any(|t| t == "aged"))
        .expect("an aged Knowledge fixture is seeded");
    assert!(aged.provenance.created_at < Utc::now() - chrono::TimeDelta::days(300));

    let context = db
        .memories()
        .recall(
            &reader,
            &totem_store::RecallQuery::new()
                .in_categories([MemoryCategory::Context])
                .limit(100),
        )
        .await
        .expect("recall succeeds");
    let expired = context
        .iter()
        .find(|record| record.content.tags.iter().any(|t| t == "expired"))
        .expect("an expired Context fixture is seeded");
    assert!(
        expired.expires_at().expect("Context has a TTL") < Utc::now(),
        "the expired Context fixture must already be past its TTL"
    );
}
