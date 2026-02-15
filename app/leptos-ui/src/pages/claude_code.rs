use leptos::prelude::*;

use crate::state::use_app_state;

#[component]
pub fn ClaudeCodePage() -> impl IntoView {
    let state = use_app_state();
    let sessions = state.claude_sessions;

    view! {
        <div class="page-header">
            <h2>"Claude Code"</h2>
        </div>

        <div class="section">
            <p class="section-description">
                "Active Claude Code sessions managed by auto-tundra agents."
            </p>
        </div>

        <div class="session-grid">
            {move || sessions.get().into_iter().map(|session| {
                let status_class = match session.status.as_str() {
                    "active" => "session-active",
                    "idle" => "session-idle",
                    "stopped" => "session-stopped",
                    _ => "session-unknown",
                };
                let status_glyph_class = match session.status.as_str() {
                    "active" => "glyph-active",
                    "idle" => "glyph-idle",
                    "stopped" => "glyph-stopped",
                    _ => "glyph-unknown",
                };
                view! {
                    <div class={format!("session-card {}", status_class)}>
                        <div class="session-card-header">
                            <span class="session-name">{session.name.clone()}</span>
                            <span class={status_glyph_class}>{session.status.clone()}</span>
                        </div>
                        <div class="session-details">
                            <div class="session-detail">
                                <span class="detail-label">"Agent"</span>
                                <span class="detail-value">{session.agent.clone()}</span>
                            </div>
                            <div class="session-detail">
                                <span class="detail-label">"Model"</span>
                                <span class="detail-value">{session.model.clone()}</span>
                            </div>
                            <div class="session-detail">
                                <span class="detail-label">"Duration"</span>
                                <span class="detail-value">{session.duration.clone()}</span>
                            </div>
                            <div class="session-detail">
                                <span class="detail-label">"Tokens"</span>
                                <span class="detail-value">{format!("{}", session.tokens)}</span>
                            </div>
                        </div>
                        <div class="session-actions">
                            <button class="action-btn">"View Output"</button>
                            <button class="action-btn action-recover">"Stop"</button>
                        </div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
