mod commands;

use clap::{Parser, Subcommand};

/// auto-tundra CLI -- orchestrate AI agents on a Dolt-backed bead board.
#[derive(Parser)]
#[command(name = "at", version, about)]
struct Cli {
    /// Base URL for the auto-tundra API daemon.
    #[arg(long, global = true, default_value = "http://localhost:9090")]
    api_url: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show system status (default when no subcommand is given).
    Status,

    /// Create a new bead and put it in the "slung" state.
    Sling {
        /// Bead title.
        title: String,
        /// Lane for the bead (experimental, standard, critical).
        #[arg(long, default_value = "standard")]
        lane: String,
    },

    /// Move a bead to the "hooked" (active/assigned) state.
    Hook {
        /// Bead ID to hook.
        bead_id: String,
    },

    /// Mark a bead as done.
    Done {
        /// Bead ID to mark done.
        bead_id: String,
    },

    /// Nudge a stuck agent (send restart signal).
    Nudge {
        /// Agent ID to nudge.
        agent_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let api_url = cli.api_url.trim_end_matches('/').to_string();

    match cli.command {
        None | Some(Commands::Status) => {
            commands::status::run(&api_url).await?;
        }
        Some(Commands::Sling { title, lane }) => {
            commands::sling::run(&api_url, &title, &lane).await?;
        }
        Some(Commands::Hook { bead_id }) => {
            commands::hook::run(&api_url, &bead_id).await?;
        }
        Some(Commands::Done { bead_id }) => {
            commands::done::run(&api_url, &bead_id).await?;
        }
        Some(Commands::Nudge { agent_id }) => {
            commands::nudge::run(&api_url, &agent_id).await?;
        }
    }

    Ok(())
}
