//! `totem`: the enrollment and credential CLI (docs/solution-intent.md §3.3;
//! ADV-CLI-001).

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Totem CLI: repo enrollment and actor credential issuance.
#[derive(Debug, Parser)]
#[command(name = "totem", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Register this repo's ARRIVE landscape with a Totem gateway, run the
    /// initial ingestion, and install the sync hook.
    Enroll {
        /// The repo's root directory (containing `arrive/`).
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        /// The gateway's base URL, e.g. `http://127.0.0.1:8787`.
        #[arg(long)]
        gateway_url: String,
        /// Recorded as this sync run's provenance.
        #[arg(long, default_value = "cli:enroll")]
        source: String,
        /// Skip installing the `post-commit` sync hook.
        #[arg(long)]
        no_hook: bool,
        /// The bearer credential to present. Falls back to `TOTEM_TOKEN`,
        /// then to a credential for this repo in the local store
        /// (ADV-CLI-002).
        #[arg(long)]
        token: Option<String>,
        /// The repo this enrollment speaks for, used to pick a stored
        /// credential. Defaults to the repo the landscape names.
        #[arg(long)]
        repo: Option<String>,
    },
    /// Actor credential commands.
    Credential {
        #[command(subcommand)]
        command: CredentialCommand,
    },
}

#[derive(Debug, Subcommand)]
enum CredentialCommand {
    /// Issue a credential bound to one repo, scope, and actor, and store it
    /// locally.
    Create {
        /// The repo this credential is bound to (`owner/name`).
        #[arg(long)]
        repo: String,
        /// The single scope this credential is bound to (e.g.
        /// `project:owner/name`, `actor:ada`, `team:id`, `platform`).
        #[arg(long)]
        scope: String,
        /// The actor identity this credential authenticates as.
        #[arg(long)]
        actor: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Enroll {
            repo_root,
            gateway_url,
            source,
            no_hook,
            token,
            repo,
        } => {
            let arrive_root = repo_root.join("arrive");

            // Resolve the credential *before* doing any work: failing after a
            // landscape parse would waste the effort and bury the actionable
            // message under output that looks like progress.
            let home = totem_cli::home_dir()?;
            let store_path = totem_cli::credential::default_store_path(&home);
            let repo_id = match repo {
                Some(repo) => repo,
                None => totem_cli::enroll::repo_id_of(&arrive_root)?,
            };
            let credential = totem_cli::auth::resolve_token(
                token.as_deref(),
                std::env::var("TOTEM_TOKEN").ok().as_deref(),
                &store_path,
                &repo_id,
            )?;
            eprintln!(
                "authenticating as the credential from {}",
                credential.source
            );

            let client = reqwest::Client::new();
            let summary = totem_cli::enroll::enroll(
                &client,
                &gateway_url,
                &arrive_root,
                &source,
                &credential,
            )
            .await?;
            println!(
                "synced {} system(s), {} component(s), {} advance(s)",
                summary.systems, summary.components, summary.advances
            );

            if !no_hook {
                let hook_path = totem_cli::hook::install(&repo_root, &gateway_url)?;
                println!("installed sync hook at {}", hook_path.display());
            }
        }
        Command::Credential {
            command: CredentialCommand::Create { repo, scope, actor },
        } => {
            let credential = totem_cli::credential::issue(&repo, &scope, &actor)?;
            let home = totem_cli::home_dir()?;
            let path = totem_cli::credential::default_store_path(&home);
            totem_cli::credential::store(&path, &credential)?;

            println!("issued credential for {actor} at {scope} (repo {repo})");
            println!("token: {}", credential.token);
            println!("stored at {}", path.display());
            println!(
                "note: this credential is not yet verified by the gateway \
                 (server-side enforcement is ADV-GATEWAY-003's job)"
            );
        }
    }

    Ok(())
}
