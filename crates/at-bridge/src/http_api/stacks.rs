//! `GET /api/stacks` -- stacked-diff view of tasks.
//!
//! A stack is a root task (no `parent_task_id`) plus every task that
//! descends from it through `parent_task_id`, ordered by `stack_position`.
//! Response shape is [`at_api_types::ApiStack`], the type both the Leptos UI
//! and the TUI deserialize.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use at_api_types::{ApiStack, ApiStackNode};
use at_core::types::Task;
use axum::{extract::State, Json};
use uuid::Uuid;

use super::state::ApiState;

fn node(task: &Task) -> ApiStackNode {
    let phase = serde_json::to_value(&task.phase)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    ApiStackNode {
        id: task.id.to_string(),
        title: task.title.clone(),
        phase,
        git_branch: task.git_branch.clone(),
        pr_number: task.pr_number,
        stack_position: task.stack_position.unwrap_or(0),
    }
}

/// Group tasks into stacks. Only roots with at least one descendant form a
/// stack; standalone tasks are not stacks. Output is deterministic: stacks
/// ordered by root `created_at`, members by `(stack_position, created_at)`.
pub(crate) fn build_stacks<'a>(tasks: impl IntoIterator<Item = &'a Task>) -> Vec<ApiStack> {
    let tasks: Vec<&Task> = tasks.into_iter().collect();
    let by_id: HashMap<Uuid, &Task> = tasks.iter().map(|t| (t.id, *t)).collect();
    let mut children: HashMap<Uuid, Vec<&Task>> = HashMap::new();
    for t in &tasks {
        if let Some(parent) = t.parent_task_id {
            // A dangling parent id cannot be walked from any root.
            if by_id.contains_key(&parent) {
                children.entry(parent).or_default().push(t);
            }
        }
    }

    let mut roots: Vec<&Task> = tasks
        .iter()
        .copied()
        .filter(|t| t.parent_task_id.is_none() && children.contains_key(&t.id))
        .collect();
    roots.sort_by_key(|t| (t.created_at, t.id));

    roots
        .into_iter()
        .map(|root| {
            // Walk all descendants; `seen` guards against parent cycles.
            let mut seen: HashSet<Uuid> = HashSet::from([root.id]);
            let mut members: Vec<&Task> = Vec::new();
            let mut frontier = vec![root.id];
            while let Some(id) = frontier.pop() {
                for child in children.get(&id).into_iter().flatten() {
                    if seen.insert(child.id) {
                        members.push(child);
                        frontier.push(child.id);
                    }
                }
            }
            members.sort_by_key(|t| (t.stack_position.unwrap_or(0), t.created_at, t.id));
            let children: Vec<ApiStackNode> = members.into_iter().map(node).collect();
            ApiStack {
                root: node(root),
                total: children.len() as u32 + 1,
                children,
            }
        })
        .collect()
}

/// GET /api/stacks -- every task stack, as `Vec<ApiStack>`.
pub(crate) async fn list_stacks(State(state): State<Arc<ApiState>>) -> Json<Vec<ApiStack>> {
    let tasks = state.tasks.read().await;
    Json(build_stacks(tasks.values()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::EventBus;
    use at_core::types::{TaskCategory, TaskComplexity, TaskPriority};

    fn task(title: &str, parent: Option<Uuid>, pos: Option<u32>) -> Task {
        let mut t = Task::new(
            title,
            Uuid::new_v4(),
            TaskCategory::Feature,
            TaskPriority::Medium,
            TaskComplexity::Small,
        );
        t.parent_task_id = parent;
        t.stack_position = pos;
        t
    }

    #[test]
    fn groups_descendants_under_root_in_position_order() {
        let root = task("root", None, None);
        let c2 = task("c2", Some(root.id), Some(2));
        let c1 = task("c1", Some(root.id), Some(1));
        let grandchild = task("c3", Some(c2.id), Some(3));
        let lone = task("lone", None, None);
        let mut with_pr = c1.clone();
        with_pr.pr_number = Some(41);
        with_pr.git_branch = Some("feat/c1".into());

        let stacks = build_stacks([&lone, &grandchild, &c2, &with_pr, &root]);
        assert_eq!(stacks.len(), 1, "standalone tasks are not stacks");
        let s = &stacks[0];
        assert_eq!(s.root.title, "root");
        assert_eq!(s.total, 4);
        let titles: Vec<_> = s.children.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, ["c1", "c2", "c3"]);
        assert_eq!(s.children[0].pr_number, Some(41));
        assert_eq!(s.children[0].git_branch.as_deref(), Some("feat/c1"));
        assert_eq!(s.children[0].stack_position, 1);
        assert_eq!(s.root.phase, "discovery");
    }

    #[tokio::test]
    async fn get_api_stacks_is_served_and_matches_api_types() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use tower::ServiceExt;

        let state = Arc::new(ApiState::new(EventBus::new()));
        let root = task("root", None, None);
        let child = task("child", Some(root.id), Some(1));
        {
            let mut tasks = state.tasks.write().await;
            tasks.insert(root.id, root.clone());
            tasks.insert(child.id, child.clone());
        }
        let app = crate::http_api::api_router(state);
        let resp = app
            .oneshot(Request::get("/api/stacks").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let stacks: Vec<ApiStack> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].root.id, root.id.to_string());
        assert_eq!(stacks[0].children[0].id, child.id.to_string());
    }
}
