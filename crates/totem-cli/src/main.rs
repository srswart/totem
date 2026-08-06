//! The `totem` binary: repo enrollment, landscape sync, and local scoped
//! credential issuance (ADV-CLI-001; docs/solution-intent.md §3.3).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use totem_cli::credential::CredentialStore;
use totem_cli::enroll;
use totem_cli::error::CliError;
use totem_core::{ActorId, RepoId, Scope};
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(
    name = "totem",
    about = "Totem: durable, auditable memory for AI agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Register this repo, run its first landscape sync, and install the
    /// sync hook.
    Enroll {
        /// A path inside the repo's git worktree.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Re-run the landscape sync (what the installed hook calls).
    Sync {
        /// A path inside the repo's git worktree.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Manage locally-issued scoped credentials.
    Credential {
        #[command(subcommand)]
        command: CredentialCommand,
    },
}

#[derive(Debug, Subcommand)]
enum CredentialCommand {
    /// Issue a new credential bound to a repo + scope.
    Issue {
        /// The repo the credential is bound to, e.g. `owner/name`.
        #[arg(long)]
        repo: RepoId,
        /// The scope the credential is bound to, e.g. `actor:ada`,
        /// `project:owner/name`, `team:id`, or `platform`.
        #[arg(long)]
        scope: Scope,
        /// The actor the credential is issued to.
        #[arg(long)]
        actor: ActorId,
    },
    /// List locally-issued credentials.
    List,
    /// Revoke a previously issued credential.
    Revoke {
        /// The credential id printed at issuance.
        #[arg(long)]
        id: Uuid,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Command) -> Result<(), CliError> {
    match command {
        Command::Enroll { path } => {
            let outcome = enroll::enroll(&path).await?;
            print_sync_summary(&outcome.sync);
            for (event, result) in outcome.hooks {
                println!("hook {event}: {result:?}");
            }
            Ok(())
        }
        Command::Sync { path } => {
            let summary = enroll::sync(&path).await?;
            print_sync_summary(&summary);
            Ok(())
        }
        Command::Credential { command } => run_credential(command).await,
    }
}

fn print_sync_summary(summary: &totem_store::SyncSummary) {
    println!(
        "synced {} systems, {} components, {} advances",
        summary.systems, summary.components, summary.advances
    );
}

async fn run_credential(command: CredentialCommand) -> Result<(), CliError> {
    let store = CredentialStore::open_default()?;
    match command {
        CredentialCommand::Issue { repo, scope, actor } => {
            let credential = store.issue(repo, scope, actor)?;
            println!("issued credential {}", credential.id);
            println!("secret: {}", credential.secret);
            println!("(store this now — it is not printed again)");
            Ok(())
        }
        CredentialCommand::List => {
            for credential in store.list()? {
                println!(
                    "{}  repo={}  scope={}  actor={}  issued_at={}",
                    credential.id,
                    credential.repo,
                    credential.scope,
                    credential.actor,
                    credential.issued_at
                );
            }
            Ok(())
        }
        CredentialCommand::Revoke { id } => {
            store.revoke(id)?;
            println!("revoked {id}");
            Ok(())
        }
    }
}
