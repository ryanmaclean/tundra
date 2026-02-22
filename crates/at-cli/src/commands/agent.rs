use super::run_task::{self, RunOptions};

pub(crate) fn category_for_role(role: &str) -> String {
    let r = role.to_lowercase();
    if r.contains("qa") || r.contains("review") || r.contains("test") {
        "testing".to_string()
    } else if r.contains("security") {
        "security".to_string()
    } else if r.contains("infra") || r.contains("devops") {
        "infrastructure".to_string()
    } else if r.contains("doc") {
        "documentation".to_string()
    } else if r.contains("perf") {
        "performance".to_string()
    } else if r.contains("ui") || r.contains("ux") {
        "ui_ux".to_string()
    } else if r.contains("bug") || r.contains("fix") {
        "bug_fix".to_string()
    } else {
        "feature".to_string()
    }
}

pub async fn run(
    api_url: &str,
    role: &str,
    task: &str,
    skills: Vec<String>,
    project_path: &str,
    model: Option<String>,
    max_agents: Option<u32>,
    dry_run: bool,
    emit_prompt: bool,
    json_output: bool,
    out_path: Option<String>,
) -> anyhow::Result<()> {
    let opts = RunOptions {
        task: task.to_string(),
        skills,
        project_path: project_path.to_string(),
        model,
        max_agents,
        lane: "standard".to_string(),
        category: category_for_role(role),
        priority: "medium".to_string(),
        complexity: "medium".to_string(),
        no_execute: false,
        dry_run,
        emit_prompt,
        json_output,
        out_path,
        role: Some(role.to_string()),
    };
    run_task::run(api_url, opts).await
}

#[cfg(test)]
mod tests {
    use super::category_for_role;

    #[test]
    fn role_to_category_mappings_are_stable() {
        assert_eq!(category_for_role("qa-reviewer"), "testing");
        assert_eq!(category_for_role("security-auditor"), "security");
        assert_eq!(category_for_role("infra-bot"), "infrastructure");
        assert_eq!(category_for_role("docs"), "documentation");
        assert_eq!(category_for_role("perf-tuner"), "performance");
        assert_eq!(category_for_role("ui-designer"), "ui_ux");
        assert_eq!(category_for_role("bug-fixer"), "bug_fix");
        assert_eq!(category_for_role("architect"), "feature");
    }
}
