use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ApiFileNode};
use crate::components::spinner::Spinner;

/// Count files and directories in a tree recursively.
fn count_nodes(nodes: &[ApiFileNode]) -> (usize, usize) {
    let mut files = 0usize;
    let mut dirs = 0usize;
    for node in nodes.iter() {
        if node.is_dir {
            dirs += 1;
            let (f, d) = count_nodes(&node.children);
            files += f;
            dirs += d;
        } else {
            files += 1;
        }
    }
    (files, dirs)
}

/// Get an icon for a file based on its extension.
fn file_icon(name: &str) -> &'static str {
    if let Some(ext) = name.rsplit('.').next() {
        match ext {
            "rs" => "\u{1F980}",
            "ts" | "tsx" => "\u{1F4D8}",
            "js" | "jsx" => "\u{1F4D9}",
            "css" | "scss" => "\u{1F3A8}",
            "json" => "\u{1F4CB}",
            "toml" | "yaml" | "yml" => "\u{2699}\u{FE0F}",
            "md" => "\u{1F4DD}",
            "html" => "\u{1F310}",
            "lock" => "\u{1F512}",
            _ => "\u{1F4C4}",
        }
    } else {
        "\u{1F4C4}"
    }
}

/// Flatten the tree into a list of (depth, node) pairs for rendering,
/// respecting expanded state and search filter.
fn flatten_tree(
    nodes: &[ApiFileNode],
    depth: usize,
    expanded: &[String],
    filter: &str,
) -> Vec<(usize, ApiFileNode)> {
    let mut result = Vec::new();

    // Sort: directories first, then files, alphabetically
    let mut sorted: Vec<_> = nodes.iter().cloned().collect();
    sorted.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));

    let lower_filter = filter.to_lowercase();

    for node in sorted.iter() {
        if !filter.is_empty() && !node_matches(node, &lower_filter) {
            continue;
        }
        result.push((depth, node.clone()));
        if node.is_dir && (expanded.iter().any(|d| *d == node.path) || !filter.is_empty()) {
            let children = flatten_tree(&node.children, depth + 1, expanded, filter);
            result.extend(children);
        }
    }

    result
}

fn node_matches(node: &ApiFileNode, filter: &str) -> bool {
    if node.name.to_lowercase().contains(filter) {
        return true;
    }
    if node.is_dir {
        return node.children.iter().any(|c| node_matches(c, filter));
    }
    false
}

#[component]
pub fn FileExplorer() -> impl IntoView {
    let (tree, set_tree) = signal(Vec::<ApiFileNode>::new());
    let (loading, set_loading) = signal(true);
    let (error_msg, set_error_msg) = signal(Option::<String>::None);
    let (search, set_search) = signal(String::new());
    let (selected_path, set_selected_path) = signal(Option::<String>::None);
    let (expanded_dirs, set_expanded_dirs) = signal(Vec::<String>::new());

    let do_refresh = move || {
        set_loading.set(true);
        set_error_msg.set(None);
        spawn_local(async move {
            match api::fetch_file_tree().await {
                Ok(data) => set_tree.set(data),
                Err(e) => {
                    set_error_msg.set(Some(format!("Failed to load: {e}")));
                    set_tree.set(demo_file_tree());
                }
            }
            set_loading.set(false);
        });
    };

    do_refresh();

    view! {
        <div class="file-explorer">
            <div class="file-explorer-header">
                <h3 class="file-explorer-title">"File Explorer"</h3>
                <button
                    class="file-explorer-refresh-btn"
                    on:click=move |_| do_refresh()
                    title="Refresh file tree"
                >
                    "Refresh"
                </button>
            </div>

            <div class="file-explorer-search-wrapper">
                <input
                    type="text"
                    class="file-explorer-search"
                    placeholder="Filter files..."
                    prop:value=move || search.get()
                    on:input=move |ev| set_search.set(event_target_value(&ev))
                />
            </div>

            {move || error_msg.get().map(|msg| view! {
                <div class="file-explorer-error">{msg}</div>
            })}

            {move || loading.get().then(|| view! {
                <div class="file-explorer-loading"><Spinner size="md"/></div>
            })}

            <div class="file-tree">
                {move || {
                    let filter = search.get();
                    let nodes = tree.get();
                    let exp = expanded_dirs.get();
                    let flat = flatten_tree(&nodes, 0, &exp, &filter);

                    flat.into_iter().map(|(depth, node)| {
                        let indent_px = depth * 16;
                        let path = node.path.clone();
                        let name = node.name.clone();

                        if node.is_dir {
                            let path_toggle = path.clone();
                            let path_check = path.clone();

                            let is_expanded = exp.iter().any(|d| *d == path_check);
                            let toggle_icon = if is_expanded { "\u{25BC}" } else { "\u{25B6}" };

                            let on_click = move |_| {
                                let p = path_toggle.clone();
                                set_expanded_dirs.update(|dirs| {
                                    if let Some(pos) = dirs.iter().position(|d| *d == p) {
                                        dirs.remove(pos);
                                    } else {
                                        dirs.push(p);
                                    }
                                });
                            };

                            view! {
                                <div
                                    class="file-tree-item file-tree-folder"
                                    style=format!("padding-left: {}px", indent_px + 4)
                                    on:click=on_click
                                >
                                    <span class="file-tree-toggle">{toggle_icon}</span>
                                    <span class="file-tree-icon">"\u{1F4C1}"</span>
                                    <span class="file-tree-name">{name}</span>
                                </div>
                            }.into_any()
                        } else {
                            let path_select = path.clone();
                            let path_class = path.clone();
                            let icon = file_icon(&name);

                            let is_selected = selected_path.get().as_deref() == Some(path_class.as_str());
                            let item_class = if is_selected {
                                "file-tree-item file-tree-file file-tree-selected"
                            } else {
                                "file-tree-item file-tree-file"
                            };

                            let on_click = move |_| {
                                set_selected_path.set(Some(path_select.clone()));
                            };

                            view! {
                                <div
                                    class=item_class
                                    style=format!("padding-left: {}px", indent_px + 4)
                                    on:click=on_click
                                >
                                    <span class="file-tree-indent"></span>
                                    <span class="file-tree-icon">{icon}</span>
                                    <span class="file-tree-name">{name}</span>
                                </div>
                            }.into_any()
                        }
                    }).collect::<Vec<_>>()
                }}
            </div>

            // File selection preview panel
            {move || selected_path.get().map(|path| {
                let display_path = path.clone();
                view! {
                    <div class="file-explorer-preview" style="padding: 12px; margin: 8px; background: var(--card-bg, #161b22); border: 1px solid var(--border-color, #30363d); border-radius: 6px;">
                        <div style="font-size: 12px; color: var(--text-muted, #8b949e); margin-bottom: 4px;">"Selected File"</div>
                        <div style="font-family: monospace; font-size: 13px; color: var(--text-primary, #e6edf3); word-break: break-all;">
                            {display_path}
                        </div>
                        <div style="font-size: 11px; color: var(--text-muted, #8b949e); margin-top: 8px;">
                            "Open in your IDE to view contents"
                        </div>
                    </div>
                }
            })}

            <div class="file-explorer-stats">
                {move || {
                    let nodes = tree.get();
                    let (files, dirs) = count_nodes(&nodes);
                    format!("{} files, {} directories", files, dirs)
                }}
            </div>
        </div>
    }
}

/// Fallback demo file tree when API is unavailable.
fn demo_file_tree() -> Vec<ApiFileNode> {
    vec![
        ApiFileNode {
            name: "src".to_string(),
            path: "src".to_string(),
            is_dir: true,
            children: vec![
                ApiFileNode {
                    name: "main.rs".to_string(),
                    path: "src/main.rs".to_string(),
                    is_dir: false,
                    children: vec![],
                },
                ApiFileNode {
                    name: "lib.rs".to_string(),
                    path: "src/lib.rs".to_string(),
                    is_dir: false,
                    children: vec![],
                },
                ApiFileNode {
                    name: "components".to_string(),
                    path: "src/components".to_string(),
                    is_dir: true,
                    children: vec![
                        ApiFileNode {
                            name: "mod.rs".to_string(),
                            path: "src/components/mod.rs".to_string(),
                            is_dir: false,
                            children: vec![],
                        },
                        ApiFileNode {
                            name: "nav_bar.rs".to_string(),
                            path: "src/components/nav_bar.rs".to_string(),
                            is_dir: false,
                            children: vec![],
                        },
                    ],
                },
                ApiFileNode {
                    name: "pages".to_string(),
                    path: "src/pages".to_string(),
                    is_dir: true,
                    children: vec![
                        ApiFileNode {
                            name: "mod.rs".to_string(),
                            path: "src/pages/mod.rs".to_string(),
                            is_dir: false,
                            children: vec![],
                        },
                        ApiFileNode {
                            name: "beads.rs".to_string(),
                            path: "src/pages/beads.rs".to_string(),
                            is_dir: false,
                            children: vec![],
                        },
                    ],
                },
            ],
        },
        ApiFileNode {
            name: "Cargo.toml".to_string(),
            path: "Cargo.toml".to_string(),
            is_dir: false,
            children: vec![],
        },
        ApiFileNode {
            name: "style.css".to_string(),
            path: "style.css".to_string(),
            is_dir: false,
            children: vec![],
        },
    ]
}
