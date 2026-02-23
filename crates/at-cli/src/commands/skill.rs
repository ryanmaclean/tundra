use std::path::Path;

use anyhow::Context;
use at_core::context_engine::{ProjectContextLoader, SkillDefinition};
use serde_json::json;

fn load_skills(project_path: &str) -> anyhow::Result<Vec<SkillDefinition>> {
    let root = Path::new(project_path);
    if !root.exists() {
        anyhow::bail!("Project path does not exist: {}", root.display());
    }

    let loader = ProjectContextLoader::new(root);
    Ok(loader.load_skill_definitions())
}

pub fn list(project_path: &str, json_output: bool) -> anyhow::Result<()> {
    let skills = load_skills(project_path)?;

    if json_output {
        let payload = skills
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "description": s.description,
                    "path": s.path,
                    "allowed_tools": s.allowed_tools,
                    "references": s.references,
                })
            })
            .collect::<Vec<_>>();
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if skills.is_empty() {
        println!("No skills found under {}/.claude/skills", project_path);
        return Ok(());
    }

    println!("Skills in {}:", project_path);
    for s in skills {
        println!("- {}", s.name);
        if !s.description.trim().is_empty() {
            println!("  {}", s.description.trim());
        }
        println!("  path: {}", s.path.display());
        if !s.allowed_tools.is_empty() {
            println!("  tools: {}", s.allowed_tools.join(", "));
        }
    }
    Ok(())
}

pub fn show(
    project_path: &str,
    skill_name: &str,
    full: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    let skills = load_skills(project_path)?;
    let skill = skills
        .into_iter()
        .find(|s| s.name == skill_name)
        .with_context(|| format!("Skill not found: {skill_name}"))?;

    if json_output {
        let payload = json!({
            "name": skill.name,
            "description": skill.description,
            "path": skill.path,
            "allowed_tools": skill.allowed_tools,
            "references": skill.references,
            "body": skill.body,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!("Skill: {}", skill.name);
    println!("Path:  {}", skill.path.display());
    if !skill.description.trim().is_empty() {
        println!("Desc:  {}", skill.description.trim());
    }
    if !skill.allowed_tools.is_empty() {
        println!("Tools: {}", skill.allowed_tools.join(", "));
    }
    if !skill.references.is_empty() {
        println!("Refs:  {}", skill.references.join(", "));
    }
    println!();

    if full {
        println!("{}", skill.body);
        return Ok(());
    }

    let lines: Vec<&str> = skill.body.lines().collect();
    let preview_len = lines.len().min(40);
    for line in lines.iter().take(preview_len) {
        println!("{line}");
    }
    if lines.len() > preview_len {
        println!(
            "\n... (truncated: showing {preview_len}/{} lines, use --full to show all)",
            lines.len()
        );
    }
    Ok(())
}

pub fn validate(project_path: &str, strict: bool, json_output: bool) -> anyhow::Result<()> {
    let root = Path::new(project_path);
    if !root.exists() {
        anyhow::bail!("Project path does not exist: {}", root.display());
    }

    let loader = ProjectContextLoader::new(root);
    let loaded = loader.load_skill_definitions();
    let skills_root = root.join(".claude").join("skills");

    let mut issues = Vec::<String>::new();
    let mut warnings = Vec::<String>::new();
    let mut discovered_dirs = Vec::new();

    if !skills_root.exists() {
        issues.push(format!(
            "Missing skills directory: {}",
            skills_root.display()
        ));
    } else if let Ok(entries) = std::fs::read_dir(&skills_root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            discovered_dirs.push(dir.clone());
            let skill_md = dir.join("SKILL.md");
            if !skill_md.exists() {
                issues.push(format!("Missing SKILL.md in {}", dir.display()));
            }
        }
    } else {
        issues.push(format!("Could not read {}", skills_root.display()));
    }

    // Any directory with SKILL.md but not present in loaded list likely failed parsing.
    for dir in &discovered_dirs {
        let has_md = dir.join("SKILL.md").exists();
        if !has_md {
            continue;
        }
        let loaded_here = loaded.iter().any(|s| s.path == *dir);
        if !loaded_here {
            issues.push(format!(
                "Unparseable or invalid SKILL.md in {}",
                dir.display()
            ));
        }
    }

    use std::collections::HashMap;
    let mut name_counts = HashMap::<String, usize>::new();
    for skill in &loaded {
        *name_counts.entry(skill.name.clone()).or_insert(0) += 1;
        if skill.name.trim().is_empty() {
            issues.push(format!("Skill with empty name at {}", skill.path.display()));
        }
        if skill.body.trim().is_empty() {
            issues.push(format!(
                "Skill body is empty for '{}' at {}",
                skill.name,
                skill.path.display()
            ));
        }
        if skill.allowed_tools.is_empty() {
            warnings.push(format!(
                "Skill '{}' has no allowed_tools in frontmatter",
                skill.name
            ));
        }
    }

    for (name, count) in name_counts {
        if count > 1 {
            issues.push(format!("Duplicate skill name '{}' ({count} entries)", name));
        }
    }

    let report = json!({
        "project_path": project_path,
        "skills_root": skills_root,
        "discovered_directories": discovered_dirs.len(),
        "loaded_skills": loaded.len(),
        "issues": issues,
        "warnings": warnings,
        "ok": issues.is_empty(),
    });

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("skill validate");
        println!("{}", "-".repeat(40));
        println!("project: {}", project_path);
        println!(
            "skills: loaded {} from {} directories",
            report["loaded_skills"].as_u64().unwrap_or(0),
            report["discovered_directories"].as_u64().unwrap_or(0)
        );

        if let Some(warns) = report["warnings"].as_array() {
            if !warns.is_empty() {
                println!("warnings:");
                for w in warns {
                    println!("  - {}", w.as_str().unwrap_or_default());
                }
            }
        }
        if let Some(errs) = report["issues"].as_array() {
            if errs.is_empty() {
                println!("issues: none");
            } else {
                println!("issues:");
                for e in errs {
                    println!("  - {}", e.as_str().unwrap_or_default());
                }
            }
        }
    }

    if strict && !report["ok"].as_bool().unwrap_or(false) {
        anyhow::bail!("skill validation failed");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn validate_strict_fails_when_skills_dir_missing() {
        let root = unique_temp_dir("at-cli-skill-validate-missing");
        std::fs::create_dir_all(&root).unwrap();

        let result = validate(&root.display().to_string(), true, true);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn validate_strict_passes_for_valid_skill() {
        let root = unique_temp_dir("at-cli-skill-validate-valid");
        let skill_md = root.join(".claude/skills/test-skill/SKILL.md");
        write_file(
            &skill_md,
            r#"---
name: test-skill
description: Example skill
allowed_tools:
  - Bash
references:
  - /tmp/example
---

# Test Skill

Body text.
"#,
        );

        let result = validate(&root.display().to_string(), true, true);
        assert!(result.is_ok());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn validate_strict_fails_when_skill_markdown_missing() {
        let root = unique_temp_dir("at-cli-skill-validate-no-md");
        let skill_dir = root.join(".claude/skills/no-markdown");
        std::fs::create_dir_all(&skill_dir).unwrap();

        let result = validate(&root.display().to_string(), true, true);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(root);
    }
}
