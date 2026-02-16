use leptos::prelude::*;

#[derive(Debug, Clone, PartialEq)]
enum IdeaCategory {
    All,
    CodeImprovement,
    Quality,
    Documentation,
    Performance,
    Security,
    UiUx,
}

impl IdeaCategory {
    fn label(&self) -> &'static str {
        match self {
            IdeaCategory::All => "All",
            IdeaCategory::CodeImprovement => "Code Improvement",
            IdeaCategory::Quality => "Quality",
            IdeaCategory::Documentation => "Documentation",
            IdeaCategory::Performance => "Performance",
            IdeaCategory::Security => "Security",
            IdeaCategory::UiUx => "UI/UX",
        }
    }

    fn css_class(&self) -> &'static str {
        match self {
            IdeaCategory::All => "idea-cat-all",
            IdeaCategory::CodeImprovement => "idea-cat-code",
            IdeaCategory::Quality => "idea-cat-quality",
            IdeaCategory::Documentation => "idea-cat-docs",
            IdeaCategory::Performance => "idea-cat-perf",
            IdeaCategory::Security => "idea-cat-security",
            IdeaCategory::UiUx => "idea-cat-uiux",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "CodeImprovement" => IdeaCategory::CodeImprovement,
            "Quality" => IdeaCategory::Quality,
            "Documentation" => IdeaCategory::Documentation,
            "Performance" => IdeaCategory::Performance,
            "Security" => IdeaCategory::Security,
            "UiUx" => IdeaCategory::UiUx,
            _ => IdeaCategory::All,
        }
    }
}

#[derive(Debug, Clone)]
struct IdeaItem {
    id: String,
    title: String,
    category: IdeaCategory,
    impact: String,  // High, Medium, Low
    effort: String,  // High, Medium, Low
    description: String,
    #[allow(dead_code)]
    expanded: bool,
}

fn demo_ideas() -> Vec<IdeaItem> {
    vec![
        IdeaItem {
            id: "idea-001".into(),
            title: "Auto-retry failed beads with backoff".into(),
            category: IdeaCategory::Quality,
            impact: "High".into(),
            effort: "Medium".into(),
            description: "Automatically retry beads that fail due to transient errors with exponential backoff. Track retry count, log each attempt, and alert after max retries exceeded. This would significantly reduce manual intervention for flaky network or API issues.".into(),
            expanded: false,
        },
        IdeaItem {
            id: "idea-002".into(),
            title: "Agent skill specialization routing".into(),
            category: IdeaCategory::CodeImprovement,
            impact: "High".into(),
            effort: "High".into(),
            description: "Let agents declare skill profiles so tasks are routed to the best-fit agent. For example, an agent specialized in Rust could handle compilation tasks while a documentation specialist handles README updates. This reduces token waste and improves output quality.".into(),
            expanded: false,
        },
        IdeaItem {
            id: "idea-003".into(),
            title: "Token usage heatmap visualization".into(),
            category: IdeaCategory::UiUx,
            impact: "Medium".into(),
            effort: "Low".into(),
            description: "Add a visual heatmap showing token usage over time per agent. Color-code by model type and highlight cost spikes. Helps identify optimization opportunities and budget planning.".into(),
            expanded: false,
        },
        IdeaItem {
            id: "idea-004".into(),
            title: "Automated security scanning".into(),
            category: IdeaCategory::Security,
            impact: "High".into(),
            effort: "Medium".into(),
            description: "Integrate cargo-audit and custom security rules into the agent pipeline. Automatically scan PRs for dependency vulnerabilities, unsafe code usage, and credential exposure before merging.".into(),
            expanded: false,
        },
        IdeaItem {
            id: "idea-005".into(),
            title: "Performance profiling dashboard".into(),
            category: IdeaCategory::Performance,
            impact: "Medium".into(),
            effort: "Medium".into(),
            description: "Add a dashboard section showing build times, test execution duration, and compilation benchmarks. Track improvements over time and flag regressions automatically.".into(),
            expanded: false,
        },
        IdeaItem {
            id: "idea-006".into(),
            title: "Auto-generate API documentation".into(),
            category: IdeaCategory::Documentation,
            impact: "Medium".into(),
            effort: "Low".into(),
            description: "Use agents to automatically generate and update API documentation from code comments, type signatures, and endpoint definitions. Keep docs in sync with implementation.".into(),
            expanded: false,
        },
    ]
}

#[component]
pub fn IdeationPage() -> impl IntoView {
    let (ideas, _set_ideas) = signal(demo_ideas());
    let (filter_category, set_filter_category) = signal("All".to_string());
    let (expanded_id, set_expanded_id) = signal(Option::<String>::None);

    let filtered_ideas = move || {
        let cat = filter_category.get();
        let all_ideas = ideas.get();
        if cat == "All" {
            all_ideas
        } else {
            let target = IdeaCategory::from_str(&cat);
            all_ideas.into_iter().filter(|i| i.category == target).collect()
        }
    };

    let on_generate = move |_| {
        web_sys::console::log_1(&"Generate Ideas clicked".into());
    };

    let impact_class = |level: &str| -> &'static str {
        match level {
            "High" => "idea-level-high",
            "Medium" => "idea-level-medium",
            "Low" => "idea-level-low",
            _ => "idea-level-medium",
        }
    };

    view! {
        <div class="page-header">
            <h2>"Ideation"</h2>
            <div class="page-header-actions">
                <button class="action-btn action-start" on:click=on_generate>
                    "Generate Ideas"
                </button>
                <select
                    class="filter-select"
                    prop:value=move || filter_category.get()
                    on:change=move |ev| set_filter_category.set(event_target_value(&ev))
                >
                    <option value="All">"All Categories"</option>
                    <option value="CodeImprovement">"Code Improvement"</option>
                    <option value="Quality">"Quality"</option>
                    <option value="Documentation">"Documentation"</option>
                    <option value="Performance">"Performance"</option>
                    <option value="Security">"Security"</option>
                    <option value="UiUx">"UI/UX"</option>
                </select>
            </div>
        </div>

        <div class="ideation-grid">
            {move || filtered_ideas().into_iter().map(|idea| {
                let iid = idea.id.clone();
                let iid_click = idea.id.clone();
                let is_expanded = move || expanded_id.get().as_deref() == Some(&iid);
                let title = idea.title.clone();
                let desc_snippet = if idea.description.len() > 120 {
                    format!("{}...", &idea.description[..120])
                } else {
                    idea.description.clone()
                };
                let full_desc = idea.description.clone();
                let cat_label = idea.category.label();
                let cat_class = idea.category.css_class();
                let impact = idea.impact.clone();
                let effort = idea.effort.clone();
                let impact_cls = impact_class(&impact);
                let effort_cls = impact_class(&effort);

                view! {
                    <div
                        class="ideation-card"
                        on:click=move |_| {
                            if expanded_id.get().as_deref() == Some(&iid_click) {
                                set_expanded_id.set(None);
                            } else {
                                set_expanded_id.set(Some(iid_click.clone()));
                            }
                        }
                    >
                        <div class="ideation-card-header">
                            <span class="ideation-card-title">{title}</span>
                            <span class={format!("ideation-cat-badge {}", cat_class)}>
                                {cat_label}
                            </span>
                        </div>
                        <div class="ideation-card-levels">
                            <span class={format!("ideation-level-badge {}", impact_cls)}>
                                {format!("Impact: {}", impact)}
                            </span>
                            <span class={format!("ideation-level-badge {}", effort_cls)}>
                                {format!("Effort: {}", effort)}
                            </span>
                        </div>
                        <div class="ideation-card-desc">
                            {move || if is_expanded() { full_desc.clone() } else { desc_snippet.clone() }}
                        </div>
                        <div class="ideation-card-actions">
                            <button
                                class="action-btn action-start"
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    web_sys::console::log_1(&"Convert to Task clicked".into());
                                }
                            >
                                "Convert to Task"
                            </button>
                        </div>
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }
}
