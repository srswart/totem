//! Scope is the boundary that keeps one developer's private context out of
//! everyone else's reads. These tests pin the wire format and the resolution
//! chain in the domain layer; `totem-store` (ADV-STORE-001) enforces the same
//! rules at the persistence layer.

use totem_core::{ActorId, RepoId, Scope, ScopeChain, ScopeParseError, TeamId};

fn actor(id: &str) -> ActorId {
    ActorId::new(id).expect("valid actor id")
}

fn repo(id: &str) -> RepoId {
    RepoId::new(id).expect("valid repo id")
}

fn team(id: &str) -> TeamId {
    TeamId::new(id).expect("valid team id")
}

#[test]
fn scope_round_trips_through_its_wire_form() {
    let cases = [
        (Scope::Actor(actor("ada")), "actor:ada"),
        (
            Scope::Project(repo("srswart/totem")),
            "project:srswart/totem",
        ),
        (Scope::Team(team("058-totem")), "team:058-totem"),
        (Scope::Platform, "platform"),
    ];

    for (scope, wire) in cases {
        assert_eq!(scope.to_string(), wire);
        assert_eq!(wire.parse::<Scope>().expect("parses"), scope);
    }
}

#[test]
fn scope_serialises_as_its_wire_form() {
    let json = serde_json::to_string(&Scope::Actor(actor("ada"))).expect("serialises");
    assert_eq!(json, "\"actor:ada\"");

    let back: Scope = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(back, Scope::Actor(actor("ada")));
}

#[test]
fn scope_rejects_malformed_wire_forms() {
    assert!(matches!(
        "tenant:ada".parse::<Scope>(),
        Err(ScopeParseError::UnknownPrefix(_))
    ));
    assert!(matches!(
        "actor".parse::<Scope>(),
        Err(ScopeParseError::UnknownPrefix(_))
    ));
    assert!(matches!(
        "actor:".parse::<Scope>(),
        Err(ScopeParseError::Id(_))
    ));
    assert!(matches!(
        "platform:everyone".parse::<Scope>(),
        Err(ScopeParseError::UnexpectedId)
    ));
}

#[test]
fn identifiers_reject_empty_and_untrimmed_values() {
    assert!(ActorId::new("").is_err());
    assert!(ActorId::new("   ").is_err());
    assert!(ActorId::new(" ada").is_err());
    assert!(RepoId::new("").is_err());
    assert!(TeamId::new("058-totem ").is_err());
}

#[test]
fn resolved_chain_runs_from_most_to_least_specific() {
    let chain = ScopeChain::resolve(
        &actor("ada"),
        Some(&repo("srswart/totem")),
        &[team("058-totem")],
    );

    assert_eq!(
        chain.scopes(),
        &[
            Scope::Actor(actor("ada")),
            Scope::Project(repo("srswart/totem")),
            Scope::Team(team("058-totem")),
            Scope::Platform,
        ]
    );
}

#[test]
fn resolved_chain_never_contains_another_actors_scope() {
    let chain = ScopeChain::resolve(
        &actor("ada"),
        Some(&repo("srswart/totem")),
        &[team("058-totem")],
    );

    assert!(chain.contains(&Scope::Actor(actor("ada"))));
    assert!(!chain.contains(&Scope::Actor(actor("grace"))));
    assert_eq!(chain.precedence_of(&Scope::Actor(actor("grace"))), None);
}

#[test]
fn resolved_chain_omits_projects_and_teams_the_actor_is_not_in() {
    let chain = ScopeChain::resolve(&actor("ada"), None, &[]);

    assert_eq!(
        chain.scopes(),
        &[Scope::Actor(actor("ada")), Scope::Platform]
    );
    assert!(!chain.contains(&Scope::Project(repo("srswart/totem"))));
    assert!(!chain.contains(&Scope::Team(team("058-totem"))));
}

#[test]
fn precedence_orders_specific_scopes_ahead_of_shared_ones() {
    let chain = ScopeChain::resolve(
        &actor("ada"),
        Some(&repo("srswart/totem")),
        &[team("058-totem")],
    );

    let actor_rank = chain
        .precedence_of(&Scope::Actor(actor("ada")))
        .expect("in chain");
    let project_rank = chain
        .precedence_of(&Scope::Project(repo("srswart/totem")))
        .expect("in chain");
    let team_rank = chain
        .precedence_of(&Scope::Team(team("058-totem")))
        .expect("in chain");
    let platform_rank = chain.precedence_of(&Scope::Platform).expect("in chain");

    assert!(actor_rank < project_rank);
    assert!(project_rank < team_rank);
    assert!(team_rank < platform_rank);
}

#[test]
fn every_chain_ends_at_the_platform_scope() {
    for chain in [
        ScopeChain::resolve(&actor("ada"), None, &[]),
        ScopeChain::resolve(&actor("ada"), Some(&repo("srswart/totem")), &[]),
        ScopeChain::resolve(&actor("ada"), None, &[team("058-totem")]),
    ] {
        assert_eq!(chain.scopes().last(), Some(&Scope::Platform));
    }
}

#[test]
fn chain_deduplicates_repeated_team_membership() {
    let chain = ScopeChain::resolve(
        &actor("ada"),
        Some(&repo("srswart/totem")),
        &[team("058-totem"), team("058-totem")],
    );

    assert_eq!(chain.scopes().len(), 4);
}
