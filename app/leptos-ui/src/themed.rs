use crate::state::DisplayMode;

/// Keys for themed UI prompts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    // Status bar
    StatusAgentsActive,
    StatusBeadsInPipeline,
    StatusNoAgents,
    StatusUptime,

    // Empty states
    EmptyBacklog,
    EmptyAgents,
    EmptyTerminals,
    EmptyKpi,

    // Actions / Events
    AgentSpawned,
    AgentStopped,
    AgentError,
    BeadCompleted,
    BeadFailed,
    BeadCreated,

    // Loading
    Loading,
    Connecting,

    // Lane labels
    LaneCritical,
    LaneStandard,
    LaneExperimental,

    // Card labels (for foil rarity)
    RarityLegendary,
    RarityRare,
    RarityCommon,
}

/// Get the themed string for a prompt in the given display mode.
pub fn themed(mode: DisplayMode, prompt: Prompt) -> &'static str {
    match (mode, prompt) {
        // =====================================================================
        // STANDARD MODE — Professional, clear
        // =====================================================================

        // Status
        (DisplayMode::Standard, Prompt::StatusAgentsActive) => "agents active",
        (DisplayMode::Standard, Prompt::StatusBeadsInPipeline) => "beads in pipeline",
        (DisplayMode::Standard, Prompt::StatusNoAgents) => "No agents running",
        (DisplayMode::Standard, Prompt::StatusUptime) => "uptime",

        // Empty states
        (DisplayMode::Standard, Prompt::EmptyBacklog) => {
            "No beads in backlog. Create a task to get started."
        }
        (DisplayMode::Standard, Prompt::EmptyAgents) => {
            "No agents are running. Start a bead to spawn one."
        }
        (DisplayMode::Standard, Prompt::EmptyTerminals) => {
            "No terminal sessions. Open one to get started."
        }
        (DisplayMode::Standard, Prompt::EmptyKpi) => "No metrics available yet.",

        // Actions
        (DisplayMode::Standard, Prompt::AgentSpawned) => "Agent started",
        (DisplayMode::Standard, Prompt::AgentStopped) => "Agent stopped",
        (DisplayMode::Standard, Prompt::AgentError) => "Agent encountered an error",
        (DisplayMode::Standard, Prompt::BeadCompleted) => "Bead completed successfully",
        (DisplayMode::Standard, Prompt::BeadFailed) => "Bead failed",
        (DisplayMode::Standard, Prompt::BeadCreated) => "New bead created",

        // Loading
        (DisplayMode::Standard, Prompt::Loading) => "Loading...",
        (DisplayMode::Standard, Prompt::Connecting) => "Connecting...",

        // Lanes
        (DisplayMode::Standard, Prompt::LaneCritical) => "CRITICAL",
        (DisplayMode::Standard, Prompt::LaneStandard) => "STANDARD",
        (DisplayMode::Standard, Prompt::LaneExperimental) => "EXPERIMENTAL",

        // Rarity
        (DisplayMode::Standard, Prompt::RarityLegendary) => "Critical",
        (DisplayMode::Standard, Prompt::RarityRare) => "Experimental",
        (DisplayMode::Standard, Prompt::RarityCommon) => "Standard",

        // =====================================================================
        // FOIL MODE — Casino / collectible card energy
        // =====================================================================

        // Status
        (DisplayMode::Foil, Prompt::StatusAgentsActive) => "agents dealt",
        (DisplayMode::Foil, Prompt::StatusBeadsInPipeline) => "cards in play",
        (DisplayMode::Foil, Prompt::StatusNoAgents) => "The table is empty",
        (DisplayMode::Foil, Prompt::StatusUptime) => "session time",

        // Empty states
        (DisplayMode::Foil, Prompt::EmptyBacklog) => "The deck is empty. Deal a new hand.",
        (DisplayMode::Foil, Prompt::EmptyAgents) => "No players at the table. Ante up!",
        (DisplayMode::Foil, Prompt::EmptyTerminals) => {
            "No consoles active. Insert coin to continue."
        }
        (DisplayMode::Foil, Prompt::EmptyKpi) => "No score yet. Play a round to see your stats.",

        // Actions
        (DisplayMode::Foil, Prompt::AgentSpawned) => "A wild agent appeared!",
        (DisplayMode::Foil, Prompt::AgentStopped) => "Agent has left the table",
        (DisplayMode::Foil, Prompt::AgentError) => "Agent busted!",
        (DisplayMode::Foil, Prompt::BeadCompleted) => "JACKPOT! Bead scored!",
        (DisplayMode::Foil, Prompt::BeadFailed) => "Bad beat \u{2014} bead folded",
        (DisplayMode::Foil, Prompt::BeadCreated) => "New card drawn from the deck",

        // Loading
        (DisplayMode::Foil, Prompt::Loading) => "Shuffling the deck...",
        (DisplayMode::Foil, Prompt::Connecting) => "Finding your table...",

        // Lanes
        (DisplayMode::Foil, Prompt::LaneCritical) => "\u{2605} LEGENDARY \u{2605}",
        (DisplayMode::Foil, Prompt::LaneStandard) => "UNCOMMON",
        (DisplayMode::Foil, Prompt::LaneExperimental) => "\u{2726} RARE \u{2726}",

        // Rarity
        (DisplayMode::Foil, Prompt::RarityLegendary) => "\u{2605} LEGENDARY \u{2605}",
        (DisplayMode::Foil, Prompt::RarityRare) => "\u{2726} RARE \u{2726}",
        (DisplayMode::Foil, Prompt::RarityCommon) => "COMMON",

        // =====================================================================
        // VT100 MODE — Mainframe operator energy
        // =====================================================================

        // Status
        (DisplayMode::Vt100, Prompt::StatusAgentsActive) => "PROC ACTIVE",
        (DisplayMode::Vt100, Prompt::StatusBeadsInPipeline) => "JOBS QUEUED",
        (DisplayMode::Vt100, Prompt::StatusNoAgents) => "NO ACTIVE PROCESSES",
        (DisplayMode::Vt100, Prompt::StatusUptime) => "UPTIME",

        // Empty states
        (DisplayMode::Vt100, Prompt::EmptyBacklog) => "NO JOBS IN QUEUE. SUBMIT JOB WITH ^N",
        (DisplayMode::Vt100, Prompt::EmptyAgents) => "NO PROCESSES RUNNING. FORK NEW PROC WITH ^F",
        (DisplayMode::Vt100, Prompt::EmptyTerminals) => "NO TTY SESSIONS. ATTACH WITH ^T",
        (DisplayMode::Vt100, Prompt::EmptyKpi) => "AWAITING TELEMETRY DATA...",

        // Actions
        (DisplayMode::Vt100, Prompt::AgentSpawned) => "FORK: NEW PROCESS STARTED",
        (DisplayMode::Vt100, Prompt::AgentStopped) => "SIGTERM: PROCESS TERMINATED",
        (DisplayMode::Vt100, Prompt::AgentError) => "SEGFAULT: PROCESS CRASHED",
        (DisplayMode::Vt100, Prompt::BeadCompleted) => "JOB TERMINATED WITH EXIT CODE 0",
        (DisplayMode::Vt100, Prompt::BeadFailed) => "JOB TERMINATED WITH EXIT CODE 1",
        (DisplayMode::Vt100, Prompt::BeadCreated) => "JOB SUBMITTED TO BATCH QUEUE",

        // Loading
        (DisplayMode::Vt100, Prompt::Loading) => "LOADING.......... ",
        (DisplayMode::Vt100, Prompt::Connecting) => "ESTABLISHING LINK...",

        // Lanes
        (DisplayMode::Vt100, Prompt::LaneCritical) => "*** PRIORITY: CRITICAL ***",
        (DisplayMode::Vt100, Prompt::LaneStandard) => "PRIORITY: NORMAL",
        (DisplayMode::Vt100, Prompt::LaneExperimental) => "PRIORITY: LOW (EXPERIMENTAL)",

        // Rarity (not really applicable, just map to priority)
        (DisplayMode::Vt100, Prompt::RarityLegendary) => "PRI-0",
        (DisplayMode::Vt100, Prompt::RarityRare) => "PRI-2",
        (DisplayMode::Vt100, Prompt::RarityCommon) => "PRI-1",
    }
}

/// Format a status line with agent count and bead count.
pub fn format_status(mode: DisplayMode, agents: usize, beads: usize) -> String {
    match mode {
        DisplayMode::Standard => {
            format!(
                "{} {} | {} {}",
                agents,
                themed(mode, Prompt::StatusAgentsActive),
                beads,
                themed(mode, Prompt::StatusBeadsInPipeline)
            )
        }
        DisplayMode::Foil => {
            format!(
                "{} {} | {} {}",
                agents,
                themed(mode, Prompt::StatusAgentsActive),
                beads,
                themed(mode, Prompt::StatusBeadsInPipeline)
            )
        }
        DisplayMode::Vt100 => {
            format!(
                "SYS: {} {} | {} {}",
                agents,
                themed(mode, Prompt::StatusAgentsActive),
                beads,
                themed(mode, Prompt::StatusBeadsInPipeline)
            )
        }
    }
}

/// Format a status line with uptime.
pub fn format_status_full(
    mode: DisplayMode,
    agents: usize,
    beads: usize,
    uptime_str: &str,
) -> String {
    match mode {
        DisplayMode::Standard => {
            format!(
                "{} agents active | {} beads in pipeline | uptime {}",
                agents, beads, uptime_str
            )
        }
        DisplayMode::Foil => {
            format!(
                "{} agents dealt | {} cards in play | session {}",
                agents, beads, uptime_str
            )
        }
        DisplayMode::Vt100 => {
            format!(
                "SYS: {} PROC ACTIVE | {} JOBS QUEUED | UPTIME {}",
                agents,
                beads,
                uptime_str.to_uppercase()
            )
        }
    }
}
