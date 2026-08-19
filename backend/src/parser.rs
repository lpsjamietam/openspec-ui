use crate::config::{GitContext, Source};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path, process::Command};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
pub struct TaskStats {
    pub total: usize,
    pub done: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Draft,
    Todo,
    InProgress,
    Done,
    Archived,
}

/// State of one artifact in a change, using the same vocabulary as
/// `openspec status`: complete (written), ready (deps met, not written yet),
/// blocked (waiting on other artifacts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactState {
    Complete,
    Ready,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusSource {
    Filesystem,
    FilesystemFallback,
    Cli,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub state: ArtifactState,
    /// Artifacts this one waits for, when blocked
    pub missing_deps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub id: String,
    pub name: String,
    pub source_id: String,
    pub status: ChangeStatus,
    pub has_proposal: bool,
    pub has_specs: bool,
    pub has_tasks: bool,
    pub has_design: bool,
    pub task_stats: Option<TaskStats>,
    /// Workflow schema from .openspec.yaml (OpenSpec >= 1.0), e.g. "spec-driven"
    pub schema: Option<String>,
    /// Per-artifact progress through the change's workflow
    pub artifacts: Vec<Artifact>,
    pub status_source: StatusSource,
    pub git: Option<GitContext>,
    pub track: Option<String>,
    pub target_branch: Option<String>,
    pub duplicate_count: usize,
    pub duplicate_sources: Vec<String>,
    #[serde(skip)]
    pub fingerprint: String,
    #[serde(skip)]
    pub is_archived: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeDetail {
    pub id: String,
    pub name: String,
    pub source_id: String,
    pub status: ChangeStatus,
    pub proposal: Option<String>,
    pub design: Option<String>,
    pub specs: Vec<SpecContent>,
    pub tasks: Option<TasksContent>,
    pub schema: Option<String>,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpecContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TasksContent {
    pub raw: String,
    pub stats: TaskStats,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub id: String,
    pub source_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpecDetail {
    pub id: String,
    pub source_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Idea {
    pub id: String,
    pub source_id: String,
    pub project_id: Option<String>,
    pub title: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdeaFrontmatter {
    id: String,
    #[serde(default)]
    project_id: Option<String>,
    created_at: String,
    updated_at: String,
}

fn parse_idea_frontmatter(content: &str) -> Option<IdeaFrontmatter> {
    let lines: Vec<&str> = content.lines().collect();
    
    if !lines.first().map(|l| l.trim() == "---").unwrap_or(false) {
        return None;
    }
    
    let mut frontmatter_lines = Vec::new();
    let mut i = 1;
    
    while i < lines.len() {
        let line = lines[i].trim();
        if line == "---" {
            break;
        }
        frontmatter_lines.push(lines[i]);
        i += 1;
    }
    
    serde_yaml::from_str::<IdeaFrontmatter>(&frontmatter_lines.join("\n")).ok()
}

fn extract_idea_title_and_description(content: &str) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();
    
    let mut frontmatter_end = 0;
    for (i, line) in lines.iter().enumerate() {
        if i > 0 && line.trim() == "---" {
            frontmatter_end = i + 1;
            break;
        }
    }
    
    let content_lines: Vec<&str> = lines.iter().skip(frontmatter_end).copied().collect();
    
    // Find the first H1 header to use as title
    let title_idx = content_lines
        .iter()
        .position(|l| l.starts_with("# "));
        
    let title = title_idx
        .map(|i| content_lines[i].trim_start_matches("# ").trim().to_string())
        .unwrap_or_else(|| "Untitled Idea".to_string());
    
    // Description is everything after the title
    // If no title found, it's everything
    let start_idx = title_idx.map(|i| i + 1).unwrap_or(0);
    
    let description = content_lines
        .iter()
        .skip(start_idx)
        .skip_while(|l| l.trim().is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    
    (title, description)
}

/// Parse tasks.md content and count [x] vs [ ] checkboxes
pub fn parse_task_stats(content: &str) -> TaskStats {
    let done_re = Regex::new(r"- \[x\]").unwrap();
    let todo_re = Regex::new(r"- \[ \]").unwrap();

    let done = done_re.find_iter(content).count();
    let todo = todo_re.find_iter(content).count();

    TaskStats {
        total: done + todo,
        done,
    }
}

/// Read the workflow schema from a change's `.openspec.yaml`.
///
/// OpenSpec >= 1.0 marks every change directory with this file the moment it is
/// created, before any artifact is written. It is the only reliable signal that a
/// directory is a change rather than stray content.
fn read_change_schema(change_path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(change_path.join(".openspec.yaml")).ok()?;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("schema:") {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    // Present but without a readable schema key: still a change.
    Some("spec-driven".to_string())
}

/// Build the artifact chain for the spec-driven workflow: proposal gates design
/// and specs, which together gate tasks. Computed from the files on disk so the
/// dashboard stays read-only and needs no OpenSpec install per repo.
fn compute_artifacts(
    has_proposal: bool,
    has_design: bool,
    has_specs: bool,
    has_tasks: bool,
) -> Vec<Artifact> {
    let state = |present: bool, missing: Vec<&str>| {
        if present {
            (ArtifactState::Complete, Vec::new())
        } else if missing.is_empty() {
            (ArtifactState::Ready, Vec::new())
        } else {
            (
                ArtifactState::Blocked,
                missing.into_iter().map(String::from).collect(),
            )
        }
    };

    let blocked_by_proposal = if has_proposal { vec![] } else { vec!["proposal"] };
    let mut blocked_by_both = Vec::new();
    if !has_design {
        blocked_by_both.push("design");
    }
    if !has_specs {
        blocked_by_both.push("specs");
    }

    [
        ("proposal", state(has_proposal, vec![])),
        ("design", state(has_design, blocked_by_proposal.clone())),
        ("specs", state(has_specs, blocked_by_proposal)),
        ("tasks", state(has_tasks, blocked_by_both)),
    ]
    .into_iter()
    .map(|(id, (state, missing_deps))| Artifact {
        id: id.to_string(),
        state,
        missing_deps,
    })
    .collect()
}

/// Compute change status from artifacts and task stats
fn compute_status(has_tasks: bool, task_stats: &Option<TaskStats>, is_archived: bool) -> ChangeStatus {
    if is_archived {
        return ChangeStatus::Archived;
    }

    match (has_tasks, task_stats) {
        (false, _) => ChangeStatus::Draft,
        (true, Some(stats)) if stats.done == 0 => ChangeStatus::Todo,
        (true, Some(stats)) if stats.done == stats.total && stats.total > 0 => ChangeStatus::Done,
        (true, Some(_)) => ChangeStatus::InProgress,
        (true, None) => ChangeStatus::Draft,
    }
}

fn update_hash(hash: &mut u64, bytes: &[u8]) {
    const FNV_PRIME: u64 = 0x100000001b3;
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn change_fingerprint(change_path: &Path, name: &str) -> String {
    let mut files = WalkDir::new(change_path)
        .min_depth(1)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    files.sort();

    let mut hash = 0xcbf29ce484222325;
    update_hash(&mut hash, name.as_bytes());
    for path in files {
        if let Ok(relative) = path.strip_prefix(change_path) {
            update_hash(&mut hash, relative.to_string_lossy().as_bytes());
        }
        if let Ok(content) = std::fs::read(&path) {
            update_hash(&mut hash, &content);
        }
    }
    format!("{hash:016x}")
}

/// Scan a single change directory and return Change
fn scan_change(change_path: &Path, source_id: &str, is_archived: bool) -> Option<Change> {
    let name = change_path.file_name()?.to_str()?;

    // Skip if not a directory
    if !change_path.is_dir() {
        return None;
    }

    let proposal_path = change_path.join("proposal.md");
    let tasks_path = change_path.join("tasks.md");
    let design_path = change_path.join("design.md");
    let specs_path = change_path.join("specs");

    let has_proposal = proposal_path.exists();
    let has_tasks = tasks_path.exists();
    let has_design = design_path.exists();
    let has_specs = specs_path.exists() && specs_path.is_dir();
    let schema = read_change_schema(change_path);

    // A directory is a change if OpenSpec marked it (.openspec.yaml, >= 1.0) or it
    // already has a proposal (pre-1.0 layout). Requiring a proposal alone hid every
    // freshly created change, since OpenSpec writes the marker first and the
    // proposal only once the agent drafts it.
    if schema.is_none() && !has_proposal {
        return None;
    }

    let task_stats = if has_tasks {
        std::fs::read_to_string(&tasks_path)
            .ok()
            .map(|content| parse_task_stats(&content))
    } else {
        None
    };

    let status = compute_status(has_tasks, &task_stats, is_archived);

    Some(Change {
        id: format!("{}/{}", source_id, name),
        name: name.to_string(),
        source_id: source_id.to_string(),
        status,
        has_proposal,
        has_specs,
        has_tasks,
        has_design,
        task_stats,
        schema,
        artifacts: compute_artifacts(has_proposal, has_design, has_specs, has_tasks),
        status_source: StatusSource::Filesystem,
        git: None,
        track: None,
        target_branch: None,
        duplicate_count: 1,
        duplicate_sources: vec![source_id.to_string()],
        fingerprint: change_fingerprint(change_path, name),
        is_archived,
    })
}

/// Scan changes/ directory for all changes
pub fn scan_changes(source_path: &Path, source_id: &str) -> Vec<Change> {
    let mut changes = Vec::new();

    let changes_path = source_path.join("changes");
    if !changes_path.exists() {
        return changes;
    }

    // Scan active changes
    for entry in std::fs::read_dir(&changes_path).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip archive directory
        if name == "archive" {
            continue;
        }

        if let Some(change) = scan_change(&path, source_id, false) {
            changes.push(change);
        }
    }

    // Scan archived changes
    let archive_path = changes_path.join("archive");
    if archive_path.exists() {
        for entry in std::fs::read_dir(&archive_path).into_iter().flatten().flatten() {
            let path = entry.path();
            if let Some(change) = scan_change(&path, source_id, true) {
                changes.push(change);
            }
        }
    }

    changes
}

pub fn attach_source_context(changes: &mut [Change], source: &Source) {
    for change in changes {
        change.git = source.git.clone();
        change.track = source.track.clone();
        change.target_branch = source.target_branch.clone();
    }
}

pub fn deduplicate_changes(changes: Vec<Change>) -> Vec<Change> {
    let mut grouped = Vec::<Change>::new();
    let mut indexes = HashMap::<(String, String, bool), usize>::new();

    for change in changes {
        let key = (
            change.name.clone(),
            change.fingerprint.clone(),
            change.is_archived,
        );
        if let Some(index) = indexes.get(&key).copied() {
            let representative = &mut grouped[index];
            if !representative.duplicate_sources.contains(&change.source_id) {
                representative.duplicate_sources.push(change.source_id);
                representative.duplicate_count = representative.duplicate_sources.len();
            }
        } else {
            indexes.insert(key, grouped.len());
            grouped.push(change);
        }
    }

    grouped
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliStatus {
    schema_name: String,
    artifacts: Vec<CliArtifact>,
}

#[derive(Debug, Deserialize)]
struct CliArtifact {
    id: String,
    status: String,
    #[serde(default)]
    requires: Vec<String>,
}

fn apply_cli_status(change: &mut Change, status: CliStatus) {
    let completed = status
        .artifacts
        .iter()
        .filter(|artifact| matches!(artifact.status.as_str(), "done" | "skipped"))
        .map(|artifact| artifact.id.clone())
        .collect::<Vec<_>>();

    change.artifacts = status
        .artifacts
        .into_iter()
        .map(|artifact| {
            let state = match artifact.status.as_str() {
                "done" => ArtifactState::Complete,
                "ready" => ArtifactState::Ready,
                "skipped" => ArtifactState::Skipped,
                _ => ArtifactState::Blocked,
            };
            let missing_deps = if state == ArtifactState::Blocked {
                artifact
                    .requires
                    .into_iter()
                    .filter(|required| !completed.contains(required))
                    .collect()
            } else {
                Vec::new()
            };
            Artifact {
                id: artifact.id,
                state,
                missing_deps,
            }
        })
        .collect();
    change.schema = Some(status.schema_name);
    change.status_source = StatusSource::Cli;
}

pub fn enrich_change_from_cli(
    change: &mut Change,
    source_path: &Path,
    openspec_command: &str,
) -> Result<(), String> {
    if change.is_archived {
        return Ok(());
    }

    change.status_source = StatusSource::FilesystemFallback;

    let working_directory = source_path.parent().unwrap_or(source_path);
    let output = Command::new(openspec_command)
        .args(["status", "--change", &change.name, "--json"])
        .current_dir(working_directory)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let status: CliStatus = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid OpenSpec status JSON: {error}"))?;
    apply_cli_status(change, status);
    Ok(())
}

/// Get full details for a specific change
pub fn get_change_detail(source_path: &Path, source_id: &str, change_name: &str) -> Option<ChangeDetail> {
    // Try active changes first
    let mut change_path = source_path.join("changes").join(change_name);
    let mut is_archived = false;

    if !change_path.exists() {
        // Try archive - need to search for name ending
        let archive_path = source_path.join("changes").join("archive");
        if archive_path.exists() {
            for entry in std::fs::read_dir(&archive_path).into_iter().flatten().flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.ends_with(change_name) || name_str == change_name {
                    change_path = entry.path();
                    is_archived = true;
                    break;
                }
            }
        }
    }

    if !change_path.exists() {
        return None;
    }

    let proposal_path = change_path.join("proposal.md");
    let tasks_path = change_path.join("tasks.md");
    let design_path = change_path.join("design.md");
    let specs_path = change_path.join("specs");

    let proposal = std::fs::read_to_string(&proposal_path).ok();
    let design = std::fs::read_to_string(&design_path).ok();

    let tasks = std::fs::read_to_string(&tasks_path).ok().map(|raw| {
        let stats = parse_task_stats(&raw);
        TasksContent { raw, stats }
    });

    let has_tasks = tasks.is_some();
    let task_stats = tasks.as_ref().map(|t| t.stats.clone());
    let status = compute_status(has_tasks, &task_stats, is_archived);

    // Scan specs within the change
    let mut specs = Vec::new();
    if specs_path.exists() {
        for entry in WalkDir::new(&specs_path).min_depth(1).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                let relative = path.strip_prefix(&specs_path).unwrap_or(path);
                if let Ok(content) = std::fs::read_to_string(path) {
                    specs.push(SpecContent {
                        path: relative.display().to_string(),
                        content,
                    });
                }
            }
        }
    }

    let name = change_path.file_name()?.to_str()?.to_string();
    let artifacts = compute_artifacts(
        proposal.is_some(),
        design.is_some(),
        !specs.is_empty(),
        has_tasks,
    );

    Some(ChangeDetail {
        id: format!("{}/{}", source_id, change_name),
        name,
        source_id: source_id.to_string(),
        status,
        proposal,
        design,
        specs,
        tasks,
        schema: read_change_schema(&change_path),
        artifacts,
    })
}

/// Scan specs/ directory for source-of-truth specs
pub fn scan_specs(source_path: &Path, source_id: &str) -> Vec<Spec> {
    let mut specs = Vec::new();
    let specs_path = source_path.join("specs");

    // Include root-level markdown files
    for entry in std::fs::read_dir(source_path).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip common change files and changes directory
            if name != "proposal.md" && name != "tasks.md" && name != "design.md" && name != "changes" {
                let id = format!("{}/{}", source_id, name.replace(".md", ""));
                specs.push(Spec {
                    id,
                    source_id: source_id.to_string(),
                    path: name.to_string(),
                });
            }
        }
    }

    // Scan specs/ directory
    if specs_path.exists() {
        for entry in WalkDir::new(&specs_path).min_depth(1).into_iter().flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                let relative = path.strip_prefix(&specs_path).unwrap_or(path);
                let path_str = relative.display().to_string();
                let id = format!("{}/{}", source_id, path_str.replace("/spec.md", "").replace(".md", ""));
                specs.push(Spec {
                    id,
                    source_id: source_id.to_string(),
                    path: path_str,
                });
            }
        }
    }

    specs
}

/// Get content for a specific spec
pub fn get_spec_detail(source_path: &Path, source_id: &str, spec_path: &str) -> Option<SpecDetail> {
    // First try root-level file
    let mut full_path = source_path.join(spec_path);

    if !full_path.exists() {
        // Then try specs/ directory
        full_path = source_path.join("specs").join(spec_path);
        if !full_path.exists() {
            return None;
        }
    }

    let content = std::fs::read_to_string(&full_path).ok()?;
    let id = format!("{}/{}", source_id, spec_path.replace("/spec.md", "").replace(".md", ""));

    Some(SpecDetail {
        id,
        source_id: source_id.to_string(),
        path: spec_path.to_string(),
        content,
    })
}

/// Scan ideas/ directory for all ideas
pub fn scan_ideas(source_path: &Path, source_id: &str) -> Vec<Idea> {
    let mut ideas = Vec::new();
    let ideas_path = source_path.join("ideas");

    if !ideas_path.exists() || !ideas_path.is_dir() {
        return ideas;
    }

    for entry in std::fs::read_dir(&ideas_path).into_iter().flatten().flatten() {
        let path = entry.path();
        
        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(frontmatter) = parse_idea_frontmatter(&content) {
                    let (title, description) = extract_idea_title_and_description(&content);
                    ideas.push(Idea {
                        id: format!("{}/{}", source_id, frontmatter.id),
                        source_id: source_id.to_string(),
                        project_id: frontmatter.project_id,
                        title,
                        description,
                        created_at: frontmatter.created_at,
                        updated_at: frontmatter.updated_at,
                    });
                }
            }
        }
    }

    ideas.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    ideas
}

/// Save idea to file system
pub fn save_idea(source_path: &Path, source_id: &str, id: &str, title: &str, description: &str, project_id: Option<&str>) -> std::io::Result<Idea> {
    let ideas_path = source_path.join("ideas");
    
    if !ideas_path.exists() {
        std::fs::create_dir_all(&ideas_path)?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    
    let project_id_line = if let Some(pid) = project_id {
        format!("projectId: {}", pid)
    } else {
        String::new()
    };
    
    let content = format!(
        r#"---
id: {}
{}
createdAt: {}
updatedAt: {}
---

# {}

{}
"#,
        id, project_id_line, now, now, title, description
    );

    let idea_path = ideas_path.join(format!("{}.md", id));
    std::fs::write(&idea_path, content)?;

    Ok(Idea {
        id: format!("{}/{}", source_id, id),
        source_id: source_id.to_string(),
        project_id: project_id.map(|s| s.to_string()),
        title: title.to_string(),
        description: description.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

/// Delete idea from file system
pub fn delete_idea(source_path: &Path, id: &str) -> std::io::Result<()> {
    let idea_path = source_path.join("ideas").join(format!("{}.md", id));

    if idea_path.exists() {
        std::fs::remove_file(idea_path)?;
    }

    Ok(())
}

/// Update idea in file system
pub fn update_idea(source_path: &Path, source_id: &str, id: &str, title: &str, description: &str) -> std::io::Result<Idea> {
    let idea_path = source_path.join("ideas").join(format!("{}.md", id));

    if !idea_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Idea file not found"
        ));
    }

    let existing_content = std::fs::read_to_string(&idea_path)?;
    let frontmatter = parse_idea_frontmatter(&existing_content).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid idea file format"
        )
    })?;

    let now = chrono::Utc::now().to_rfc3339();

    let project_id_line = if let Some(ref pid) = frontmatter.project_id {
        format!("projectId: {}", pid)
    } else {
        String::new()
    };

    let content = format!(
        r#"---
id: {}
{}
createdAt: {}
updatedAt: {}
---

# {}

{}
"#,
        id, project_id_line, frontmatter.created_at, now, title, description
    );

    std::fs::write(&idea_path, content)?;

    Ok(Idea {
        id: format!("{}/{}", source_id, id),
        source_id: source_id.to_string(),
        project_id: frontmatter.project_id,
        title: title.to_string(),
        description: description.to_string(),
        created_at: frontmatter.created_at,
        updated_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_idea_frontmatter_with_empty_line() {
        let content = r#"---
id: idea-123

createdAt: 2026-01-01T00:00:00+00:00
updatedAt: 2026-01-01T00:00:00+00:00
---

# Title
Description"#;
        
        let frontmatter = parse_idea_frontmatter(content);
        assert!(frontmatter.is_some());
        let f = frontmatter.unwrap();
        assert_eq!(f.id, "idea-123");
        assert!(f.project_id.is_none());
    }

    #[test]
    fn test_parse_task_stats() {
        let content = r#"
# Tasks

## 1. Backend
- [x] 1.1 Done task
- [ ] 1.2 Todo task
- [x] 1.3 Another done

## 2. Frontend
- [ ] 2.1 Pending
- [ ] 2.2 Also pending
"#;
        let stats = parse_task_stats(content);
        assert_eq!(stats.total, 5);
        assert_eq!(stats.done, 2);
    }

    #[test]
    fn test_compute_artifacts_empty_change() {
        // A change OpenSpec just created: marker only, nothing written yet.
        let artifacts = compute_artifacts(false, false, false, false);
        let by_id = |id: &str| artifacts.iter().find(|a| a.id == id).unwrap().clone();

        assert_eq!(by_id("proposal").state, ArtifactState::Ready);
        assert_eq!(by_id("design").state, ArtifactState::Blocked);
        assert_eq!(by_id("design").missing_deps, vec!["proposal"]);
        assert_eq!(by_id("tasks").state, ArtifactState::Blocked);
        assert_eq!(by_id("tasks").missing_deps, vec!["design", "specs"]);
    }

    #[test]
    fn test_compute_artifacts_unblocks_after_proposal() {
        let artifacts = compute_artifacts(true, false, false, false);
        let by_id = |id: &str| artifacts.iter().find(|a| a.id == id).unwrap().clone();

        assert_eq!(by_id("proposal").state, ArtifactState::Complete);
        assert_eq!(by_id("design").state, ArtifactState::Ready);
        assert_eq!(by_id("specs").state, ArtifactState::Ready);
        // tasks still waits for both
        assert_eq!(by_id("tasks").state, ArtifactState::Blocked);
        assert_eq!(by_id("tasks").missing_deps, vec!["design", "specs"]);
    }

    #[test]
    fn test_compute_artifacts_all_written() {
        let artifacts = compute_artifacts(true, true, true, true);
        assert!(artifacts.iter().all(|a| a.state == ArtifactState::Complete));
        assert!(artifacts.iter().all(|a| a.missing_deps.is_empty()));
    }

    #[test]
    fn test_read_change_schema() {
        let dir = std::env::temp_dir().join(format!("ospec-ui-schema-{}", std::process::id()));
        let change = dir.join("changes").join("some-change");
        std::fs::create_dir_all(&change).unwrap();

        // No marker: not recognised as a change by itself
        assert_eq!(read_change_schema(&change), None);

        std::fs::write(
            change.join(".openspec.yaml"),
            "schema: spec-driven\ncreated: 2026-07-26\n",
        )
        .unwrap();
        assert_eq!(read_change_schema(&change).as_deref(), Some("spec-driven"));

        // Marker without a readable schema key still counts as a change
        std::fs::write(change.join(".openspec.yaml"), "created: 2026-07-26\n").unwrap();
        assert_eq!(read_change_schema(&change).as_deref(), Some("spec-driven"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_scan_change_finds_marker_only_change() {
        let dir = std::env::temp_dir().join(format!("ospec-ui-scan-{}", std::process::id()));
        let change = dir.join("changes").join("fresh-change");
        std::fs::create_dir_all(&change).unwrap();
        std::fs::write(change.join(".openspec.yaml"), "schema: spec-driven\n").unwrap();

        let found = scan_change(&change, "repo", false).expect("marker-only change must be visible");
        assert_eq!(found.name, "fresh-change");
        assert_eq!(found.status, ChangeStatus::Draft);
        assert!(!found.has_proposal);
        assert_eq!(found.schema.as_deref(), Some("spec-driven"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn identical_worktree_changes_are_grouped() {
        let dir = std::env::temp_dir().join(format!("ospec-ui-dedupe-{}", std::process::id()));
        let first = dir.join("one").join("same-change");
        let second = dir.join("two").join("same-change");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("proposal.md"), "# Same proposal\n").unwrap();
        std::fs::write(second.join("proposal.md"), "# Same proposal\n").unwrap();

        let grouped = deduplicate_changes(vec![
            scan_change(&first, "worktree-one", false).unwrap(),
            scan_change(&second, "worktree-two", false).unwrap(),
        ]);

        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].source_id, "worktree-one");
        assert_eq!(grouped[0].duplicate_count, 2);
        assert_eq!(
            grouped[0].duplicate_sources,
            vec!["worktree-one", "worktree-two"]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn divergent_worktree_changes_remain_separate() {
        let dir = std::env::temp_dir().join(format!("ospec-ui-divergent-{}", std::process::id()));
        let first = dir.join("one").join("same-change");
        let second = dir.join("two").join("same-change");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("proposal.md"), "# First proposal\n").unwrap();
        std::fs::write(second.join("proposal.md"), "# Diverged proposal\n").unwrap();

        let grouped = deduplicate_changes(vec![
            scan_change(&first, "worktree-one", false).unwrap(),
            scan_change(&second, "worktree-two", false).unwrap(),
        ]);

        assert_eq!(grouped.len(), 2);
        assert!(grouped.iter().all(|change| change.duplicate_count == 1));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cli_status_maps_ready_blocked_and_skipped_artifacts() {
        let dir = std::env::temp_dir().join(format!("ospec-ui-cli-{}", std::process::id()));
        let change_path = dir.join("cli-change");
        std::fs::create_dir_all(&change_path).unwrap();
        std::fs::write(change_path.join("proposal.md"), "# Proposal\n").unwrap();
        let mut change = scan_change(&change_path, "repo", false).unwrap();
        let status: CliStatus = serde_json::from_str(
            r#"{
                "schemaName": "spec-driven",
                "artifacts": [
                    {"id":"proposal","status":"done","requires":[]},
                    {"id":"design","status":"skipped","requires":["proposal"]},
                    {"id":"specs","status":"ready","requires":["proposal"]},
                    {"id":"tasks","status":"blocked","requires":["specs"]}
                ]
            }"#,
        )
        .unwrap();

        apply_cli_status(&mut change, status);

        assert_eq!(change.status_source, StatusSource::Cli);
        assert_eq!(change.schema.as_deref(), Some("spec-driven"));
        assert_eq!(change.artifacts[0].state, ArtifactState::Complete);
        assert_eq!(change.artifacts[1].state, ArtifactState::Skipped);
        assert_eq!(change.artifacts[2].state, ArtifactState::Ready);
        assert_eq!(change.artifacts[3].state, ArtifactState::Blocked);
        assert_eq!(change.artifacts[3].missing_deps, vec!["specs"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unavailable_cli_preserves_filesystem_status() {
        let dir = std::env::temp_dir().join(format!("ospec-ui-fallback-{}", std::process::id()));
        let source_path = dir.join("openspec");
        let change_path = source_path.join("changes").join("fallback-change");
        std::fs::create_dir_all(&change_path).unwrap();
        std::fs::write(change_path.join("proposal.md"), "# Proposal\n").unwrap();
        let mut change = scan_change(&change_path, "repo", false).unwrap();
        let original_artifacts = change.artifacts.clone();

        let result = enrich_change_from_cli(
            &mut change,
            &source_path,
            "openspec-ui-command-that-does-not-exist",
        );

        assert!(result.is_err());
        assert_eq!(change.status_source, StatusSource::FilesystemFallback);
        assert_eq!(change.artifacts, original_artifacts);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_compute_status() {
        // No tasks = Draft
        assert_eq!(compute_status(false, &None, false), ChangeStatus::Draft);

        // Tasks with 0 done = Todo
        assert_eq!(
            compute_status(true, &Some(TaskStats { total: 5, done: 0 }), false),
            ChangeStatus::Todo
        );

        // Tasks partially done = InProgress
        assert_eq!(
            compute_status(true, &Some(TaskStats { total: 5, done: 2 }), false),
            ChangeStatus::InProgress
        );

        // All tasks done = Done
        assert_eq!(
            compute_status(true, &Some(TaskStats { total: 5, done: 5 }), false),
            ChangeStatus::Done
        );

        // Archived = Archived regardless
        assert_eq!(
            compute_status(false, &None, true),
            ChangeStatus::Archived
        );

        // Archived even if all tasks done
        assert_eq!(
            compute_status(true, &Some(TaskStats { total: 5, done: 5 }), true),
            ChangeStatus::Archived
        );
    }
}
