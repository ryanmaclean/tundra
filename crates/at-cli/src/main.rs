#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod commands;

use clap::{Parser, Subcommand};

/// auto-tundra CLI -- orchestrate AI agents on a Dolt-backed bead board.
#[derive(Parser)]
#[command(name = "at", version, about)]
struct Cli {
    /// Base URL for the auto-tundra API daemon.
    #[arg(short = 'u', long, global = true)]
    api_url: Option<String>,

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

    /// Skill commands for project-local SKILL.md files.
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },

    /// Create a skill-aware task and execute it.
    Run {
        /// Task prompt/title.
        #[arg(short = 't', long)]
        task: String,
        /// Skill names to include (repeatable).
        #[arg(short = 's', long = "skill")]
        skills: Vec<String>,
        /// Project root containing .claude/skills.
        #[arg(short = 'p', long = "project-path", default_value = ".")]
        project_path: String,
        /// Optional model preference.
        #[arg(short = 'm', long)]
        model: Option<String>,
        /// Optional max agent budget hint.
        #[arg(short = 'n', long = "max-agents")]
        max_agents: Option<u32>,
        /// Lane for created bead.
        #[arg(short = 'l', long, default_value = "standard")]
        lane: String,
        /// Task category.
        #[arg(short = 'c', long, default_value = "feature")]
        category: String,
        /// Task priority.
        #[arg(short = 'P', long, default_value = "medium")]
        priority: String,
        /// Task complexity.
        #[arg(short = 'x', long, default_value = "medium")]
        complexity: String,
        /// Skip POST /api/tasks/{id}/execute.
        #[arg(long, default_value_t = false)]
        no_execute: bool,
        /// Build the task payload and prompt locally without API calls.
        #[arg(short = 'd', long, default_value_t = false)]
        dry_run: bool,
        /// Print the compiled prompt/description (useful with --dry-run).
        #[arg(short = 'e', long, default_value_t = false)]
        emit_prompt: bool,
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
        /// Write JSON artifact to this file path.
        #[arg(short = 'o', long = "out")]
        out: Option<String>,
    },

    /// Run a named role/agent task (skill-aware).
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },

    /// Environment and connectivity checks.
    Doctor {
        /// Project root containing .claude/skills.
        #[arg(short = 'p', long = "project-path", default_value = ".")]
        project_path: String,
        /// Exit non-zero if any checks fail.
        #[arg(short = 'S', long, default_value_t = false)]
        strict: bool,
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
        /// Write JSON artifact to this file path.
        #[arg(short = 'o', long = "out")]
        out: Option<String>,
    },

    /// Ideation and feature discovery.
    Ideation {
        #[command(subcommand)]
        command: IdeationCommands,
    },
}

#[derive(Subcommand)]
pub enum IdeationCommands {
    /// List all generated ideas
    List {
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
    },
    /// Generate new ideas
    Generate {
        /// Category of ideas (e.g. code-improvement, quality, ui-ux)
        #[arg(short = 'c', long, default_value = "code-improvement")]
        category: String,
        /// Context for ideation (e.g. codebase focus area)
        #[arg(short = 'C', long)]
        context: String,
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
    },
    /// Convert an idea into an executable task bead
    Convert {
        /// The UUID of the idea to convert
        idea_id: String,
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SkillCommands {
    /// List skills discovered from .claude/skills/*/SKILL.md.
    List {
        /// Project root containing .claude/skills.
        #[arg(short = 'p', long = "project-path", default_value = ".")]
        project_path: String,
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
    },
    /// Show one skill by name.
    Show {
        /// Skill name.
        #[arg(short = 's', long = "skill")]
        skill: String,
        /// Project root containing .claude/skills.
        #[arg(short = 'p', long = "project-path", default_value = ".")]
        project_path: String,
        /// Show full body (default shows preview).
        #[arg(short = 'f', long, default_value_t = false)]
        full: bool,
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
    },
    /// Validate all skills under .claude/skills.
    Validate {
        /// Project root containing .claude/skills.
        #[arg(short = 'p', long = "project-path", default_value = ".")]
        project_path: String,
        /// Exit non-zero when issues are found.
        #[arg(short = 'S', long, default_value_t = false)]
        strict: bool,
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// Run a role-scoped task.
    Run {
        /// Role name (e.g. qa-reviewer, builder, architect).
        #[arg(short = 'r', long)]
        role: String,
        /// Task prompt/title.
        #[arg(short = 't', long)]
        task: String,
        /// Skill names to include (repeatable).
        #[arg(short = 's', long = "skill")]
        skills: Vec<String>,
        /// Project root containing .claude/skills.
        #[arg(short = 'p', long = "project-path", default_value = ".")]
        project_path: String,
        /// Optional model preference.
        #[arg(short = 'm', long)]
        model: Option<String>,
        /// Optional max agent budget hint.
        #[arg(short = 'n', long = "max-agents")]
        max_agents: Option<u32>,
        /// Build the task payload and prompt locally without API calls.
        #[arg(short = 'd', long, default_value_t = false)]
        dry_run: bool,
        /// Print the compiled prompt/description (useful with --dry-run).
        #[arg(short = 'e', long, default_value_t = false)]
        emit_prompt: bool,
        /// Output JSON.
        #[arg(short = 'j', long, default_value_t = false)]
        json: bool,
        /// Write JSON artifact to this file path.
        #[arg(short = 'o', long = "out")]
        out: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let api_url = cli.api_url.unwrap_or_else(|| {
        at_core::lockfile::DaemonLockfile::read_valid()
            .map(|lock| lock.api_url())
            .unwrap_or_else(|| {
                eprintln!("warning: no running daemon found, trying http://127.0.0.1:9090");
                "http://127.0.0.1:9090".to_string()
            })
    });
    let api_url = api_url.trim_end_matches('/').to_string();

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
        Some(Commands::Skill { command }) => match command {
            SkillCommands::List { project_path, json } => {
                commands::skill::list(&project_path, json)?;
            }
            SkillCommands::Show {
                skill,
                project_path,
                full,
                json,
            } => {
                commands::skill::show(&project_path, &skill, full, json)?;
            }
            SkillCommands::Validate {
                project_path,
                strict,
                json,
            } => {
                commands::skill::validate(&project_path, strict, json)?;
            }
        },
        Some(Commands::Run {
            task,
            skills,
            project_path,
            model,
            max_agents,
            lane,
            category,
            priority,
            complexity,
            no_execute,
            dry_run,
            emit_prompt,
            json,
            out,
        }) => {
            let opts = commands::run_task::RunOptions {
                task,
                skills,
                project_path,
                model,
                max_agents,
                lane,
                category,
                priority,
                complexity,
                no_execute,
                dry_run,
                emit_prompt,
                json_output: json,
                out_path: out,
                role: None,
            };
            commands::run_task::run(&api_url, opts).await?;
        }
        Some(Commands::Agent { command }) => match command {
            AgentCommands::Run {
                role,
                task,
                skills,
                project_path,
                model,
                max_agents,
                dry_run,
                emit_prompt,
                json,
                out,
            } => {
                commands::agent::run(
                    &api_url,
                    &role,
                    &task,
                    skills,
                    &project_path,
                    model,
                    max_agents,
                    dry_run,
                    emit_prompt,
                    json,
                    out,
                )
                .await?;
            }
        },
        Some(Commands::Doctor {
            project_path,
            strict,
            json,
            out,
        }) => {
            commands::doctor::run(&api_url, &project_path, strict, json, out.as_deref()).await?;
        }
        Some(Commands::Ideation { command }) => match command {
            IdeationCommands::List { .. } => {
                commands::ideation::list(&api_url).await?;
            }
            IdeationCommands::Generate {
                category, context, ..
            } => {
                commands::ideation::generate(&api_url, &category, &context).await?;
            }
            IdeationCommands::Convert { idea_id, .. } => {
                commands::ideation::convert(&api_url, &idea_id).await?;
            }
        },
    }

    Ok(())
}
