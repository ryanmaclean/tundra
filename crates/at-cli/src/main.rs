mod commands;

use clap::{Parser, Subcommand};

/// auto-tundra CLI -- orchestrate AI agents on a Dolt-backed bead board.
#[derive(Parser)]
#[command(name = "at", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show system status (default when no subcommand is given).
    Status,

    /// Assign (sling) a bead to an agent.
    Sling {
        /// Bead title or ID to assign.
        bead: String,
        /// Target agent name.
        agent: String,
    },

    /// Pin (hook) a piece of work so an agent can claim it.
    Hook {
        /// Title for the new bead.
        title: String,
        /// Agent name to assign.
        agent: String,
    },

    /// Mark a bead as done (or failed).
    Done {
        /// Bead title or ID to complete.
        bead: String,
        /// Mark as failed instead of done.
        #[arg(long)]
        fail: bool,
    },

    /// Send a nudge message to an agent.
    Nudge {
        /// Target agent name.
        agent: String,
        /// Message to send.
        #[arg(short, long)]
        message: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Status) => {
            commands::status::run().await?;
        }
        Some(Commands::Sling { bead, agent }) => {
            println!("sling: bead={bead:?} -> agent={agent:?} (not yet implemented)");
        }
        Some(Commands::Hook { title, agent }) => {
            println!("hook: title={title:?} -> agent={agent:?} (not yet implemented)");
        }
        Some(Commands::Done { bead, fail }) => {
            let outcome = if fail { "failed" } else { "done" };
            println!("done: bead={bead:?} outcome={outcome} (not yet implemented)");
        }
        Some(Commands::Nudge { agent, message }) => {
            println!("nudge: agent={agent:?} message={message:?} (not yet implemented)");
        }
    }

    Ok(())
}
