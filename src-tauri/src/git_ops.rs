use serde::{Deserialize, Serialize};
use std::process::Command;

// ── Data Types ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeStatus {
    pub workspace_id: String,
    pub base_branch: String,
    pub head_sha: String,
    pub base_sha: String,
    pub ahead: u32,
    pub behind: u32,
    pub files_changed: u32,
    pub total_additions: u32,
    pub total_deletions: u32,
    pub files: Vec<FileChange>,
    pub has_conflicts: bool,
    pub conflict_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub success: bool,
    pub merge_sha: Option<String>,
    pub conflicts: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    Merge,
    Squash,
    Rebase,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileVersion {
    Base,
    Working,
}

// ── Helper: run a git command and return stdout ──

fn git(worktree_path: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(worktree_path)
        .output()
        .map_err(|e| format!("git exec error: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git error: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_allow_empty(worktree_path: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(worktree_path)
        .output()
        .map_err(|e| format!("git exec error: {e}"))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ── Resolve the merge-base SHA between HEAD and the base branch ──

fn resolve_merge_base(worktree_path: &str, base_branch: &str) -> Result<String, String> {
    let sha = git(worktree_path, &["merge-base", "HEAD", base_branch])?;
    Ok(sha.trim().to_string())
}

// ── Detect the base branch for a worktree ──

pub fn detect_base_branch(repo_path: &str) -> String {
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
        .current_dir(repo_path)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let branch = String::from_utf8_lossy(&o.stdout).trim().to_string();
            branch
                .strip_prefix("origin/")
                .unwrap_or(&branch)
                .to_string()
        }
        _ => "main".to_string(),
    }
}

// ── Parse unified diff output into FileDiff structs ──

fn parse_status_letter(letter: &str) -> FileStatus {
    match letter {
        "A" => FileStatus::Added,
        "D" => FileStatus::Deleted,
        "M" => FileStatus::Modified,
        s if s.starts_with('R') => FileStatus::Renamed,
        s if s.starts_with('C') => FileStatus::Copied,
        _ => FileStatus::Modified,
    }
}

fn parse_hunk_header(header: &str) -> Option<(u32, u32, u32, u32)> {
    // Parse "@@ -old_start,old_count +new_start,new_count @@"
    let header = header.strip_prefix("@@ ")?;
    let at_end = header.find(" @@")?;
    let range_part = &header[..at_end];

    let parts: Vec<&str> = range_part.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let old_range = parts[0].strip_prefix('-')?;
    let new_range = parts[1].strip_prefix('+')?;

    let (old_start, old_count) = parse_range(old_range);
    let (new_start, new_count) = parse_range(new_range);

    Some((old_start, old_count, new_start, new_count))
}

fn parse_range(range: &str) -> (u32, u32) {
    if let Some((start, count)) = range.split_once(',') {
        (
            start.parse().unwrap_or(0),
            count.parse().unwrap_or(0),
        )
    } else {
        (range.parse().unwrap_or(0), 1)
    }
}

fn parse_unified_diff(diff_output: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let lines: Vec<&str> = diff_output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // Look for "diff --git a/... b/..."
        if !lines[i].starts_with("diff --git ") {
            i += 1;
            continue;
        }

        let diff_header = lines[i];
        i += 1;

        // Extract paths from diff header: "diff --git a/path b/path"
        let (old_path_raw, new_path_raw) = parse_diff_git_header(diff_header);

        let mut status = FileStatus::Modified;
        let mut old_path: Option<String> = None;
        let mut binary = false;
        let mut hunks: Vec<DiffHunk> = Vec::new();

        // Parse extended headers (index, old mode, new mode, similarity, rename, etc.)
        while i < lines.len() && !lines[i].starts_with("diff --git ") {
            let line = lines[i];

            if line.starts_with("new file mode") {
                status = FileStatus::Added;
                i += 1;
            } else if line.starts_with("deleted file mode") {
                status = FileStatus::Deleted;
                i += 1;
            } else if line.starts_with("rename from ") {
                old_path = Some(line.strip_prefix("rename from ").unwrap().to_string());
                status = FileStatus::Renamed;
                i += 1;
            } else if line.starts_with("rename to ") {
                i += 1;
            } else if line.starts_with("copy from ") {
                old_path = Some(line.strip_prefix("copy from ").unwrap().to_string());
                status = FileStatus::Copied;
                i += 1;
            } else if line.starts_with("copy to ") {
                i += 1;
            } else if line.starts_with("similarity index")
                || line.starts_with("dissimilarity index")
                || line.starts_with("index ")
                || line.starts_with("old mode")
                || line.starts_with("new mode")
            {
                i += 1;
            } else if line == "Binary files differ"
                || line.starts_with("Binary files ")
                || line.contains("Binary files")
            {
                binary = true;
                i += 1;
            } else if line.starts_with("GIT binary patch") {
                binary = true;
                // Skip binary patch data until next diff or end
                i += 1;
                while i < lines.len() && !lines[i].starts_with("diff --git ") {
                    i += 1;
                }
            } else if line.starts_with("--- ") {
                // Start of actual diff content — skip --- and +++ lines
                i += 1; // skip ---
                if i < lines.len() && lines[i].starts_with("+++ ") {
                    i += 1; // skip +++
                }
            } else if line.starts_with("@@ ") {
                // Parse hunk
                let header = line.to_string();
                let parsed = parse_hunk_header(line);
                let (old_start, old_count, new_start, new_count) =
                    parsed.unwrap_or((0, 0, 0, 0));

                i += 1;

                let mut hunk_lines: Vec<DiffLine> = Vec::new();
                let mut old_lineno = old_start;
                let mut new_lineno = new_start;

                while i < lines.len() {
                    let l = lines[i];
                    if l.starts_with("diff --git ")
                        || l.starts_with("@@ ")
                    {
                        break;
                    }

                    if let Some(content) = l.strip_prefix('+') {
                        hunk_lines.push(DiffLine {
                            kind: "add".to_string(),
                            old_lineno: None,
                            new_lineno: Some(new_lineno),
                            content: content.to_string(),
                        });
                        new_lineno += 1;
                    } else if let Some(content) = l.strip_prefix('-') {
                        hunk_lines.push(DiffLine {
                            kind: "delete".to_string(),
                            old_lineno: Some(old_lineno),
                            new_lineno: None,
                            content: content.to_string(),
                        });
                        old_lineno += 1;
                    } else if l.starts_with(' ') || l.is_empty() {
                        let content = if l.is_empty() {
                            String::new()
                        } else {
                            l[1..].to_string()
                        };
                        hunk_lines.push(DiffLine {
                            kind: "context".to_string(),
                            old_lineno: Some(old_lineno),
                            new_lineno: Some(new_lineno),
                            content,
                        });
                        old_lineno += 1;
                        new_lineno += 1;
                    } else if l == "\\ No newline at end of file" {
                        // Skip this marker
                        i += 1;
                        continue;
                    } else {
                        // Unknown line format — stop hunk parsing
                        break;
                    }

                    i += 1;
                }

                hunks.push(DiffHunk {
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    header,
                    lines: hunk_lines,
                });
            } else {
                i += 1;
            }
        }

        let mut additions: u32 = 0;
        let mut deletions: u32 = 0;
        for hunk in &hunks {
            for line in &hunk.lines {
                match line.kind.as_str() {
                    "add" => additions += 1,
                    "delete" => deletions += 1,
                    _ => {}
                }
            }
        }

        let path = new_path_raw.clone();
        if status == FileStatus::Renamed && old_path.is_none() {
            old_path = Some(old_path_raw);
        }

        files.push(FileDiff {
            path,
            old_path,
            status,
            binary,
            hunks,
            additions,
            deletions,
        });
    }

    files
}

fn parse_diff_git_header(header: &str) -> (String, String) {
    // "diff --git a/path b/path"
    let rest = header.strip_prefix("diff --git ").unwrap_or(header);

    // Handle paths with spaces: split on " b/" but be careful with paths containing " b/"
    // The safest approach: a/ prefix and b/ prefix are always present
    if let Some(a_rest) = rest.strip_prefix("a/") {
        // Find " b/" separator
        if let Some(b_idx) = a_rest.find(" b/") {
            let old = a_rest[..b_idx].to_string();
            let new = a_rest[b_idx + 3..].to_string();
            return (old, new);
        }
    }

    // Fallback: split on whitespace
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() == 2 {
        let old = parts[0].strip_prefix("a/").unwrap_or(parts[0]).to_string();
        let new = parts[1].strip_prefix("b/").unwrap_or(parts[1]).to_string();
        (old, new)
    } else {
        (String::new(), String::new())
    }
}

// ── Public API ──

pub fn compute_diff(
    worktree_path: &str,
    base_branch: &str,
    paths: Option<&[String]>,
) -> Result<Vec<FileDiff>, String> {
    let base_sha = resolve_merge_base(worktree_path, base_branch)?;

    let mut args = vec![
        "diff".to_string(),
        format!("{base_sha}...HEAD"),
        "--unified=3".to_string(),
        "-M".to_string(),  // detect renames
        "-C".to_string(),  // detect copies
    ];

    if let Some(file_paths) = paths {
        args.push("--".to_string());
        for p in file_paths {
            args.push(p.clone());
        }
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = git_allow_empty(worktree_path, &arg_refs)?;

    Ok(parse_unified_diff(&output))
}

pub fn compute_status(
    worktree_path: &str,
    workspace_id: &str,
    base_branch: &str,
) -> Result<WorktreeStatus, String> {
    let base_sha = resolve_merge_base(worktree_path, base_branch)?;
    let head_sha = git(worktree_path, &["rev-parse", "HEAD"])?
        .trim()
        .to_string();

    // ahead/behind
    let rev_list = git(
        worktree_path,
        &["rev-list", "--left-right", "--count", &format!("{base_sha}...HEAD")],
    )?;
    let counts: Vec<&str> = rev_list.trim().split('\t').collect();
    let behind: u32 = counts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let ahead: u32 = counts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);

    // file list with numstat
    let numstat_output = git_allow_empty(
        worktree_path,
        &["diff", "--numstat", "-M", "-C", &format!("{base_sha}...HEAD")],
    )?;

    // Also get name-status for accurate status letters
    let name_status_output = git_allow_empty(
        worktree_path,
        &["diff", "--name-status", "-M", "-C", &format!("{base_sha}...HEAD")],
    )?;

    // Parse name-status into a map: path -> (status, old_path)
    let mut status_map: std::collections::HashMap<String, (FileStatus, Option<String>)> =
        std::collections::HashMap::new();

    for line in name_status_output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }
        let status_letter = parts[0];
        match parts.len() {
            2 => {
                let path = parts[1].to_string();
                status_map.insert(path, (parse_status_letter(status_letter), None));
            }
            3 => {
                // Rename/copy: status\told_path\tnew_path
                let old = parts[1].to_string();
                let new_path = parts[2].to_string();
                status_map.insert(
                    new_path,
                    (parse_status_letter(status_letter), Some(old)),
                );
            }
            _ => {}
        }
    }

    // Parse numstat
    let mut files: Vec<FileChange> = Vec::new();
    let mut total_additions: u32 = 0;
    let mut total_deletions: u32 = 0;

    for line in numstat_output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }

        let binary = parts[0] == "-" && parts[1] == "-";
        let additions: u32 = parts[0].parse().unwrap_or(0);
        let deletions: u32 = parts[1].parse().unwrap_or(0);

        // For renames, numstat shows "old => new" or "{old => new}/rest"
        // but with -M -C it shows the new path in the third field
        // When there's a rename, there might be a 4th field
        let path = if parts.len() >= 4 {
            // Rename: additions\tdeletions\told_path\tnew_path
            parts[3].to_string()
        } else {
            parts[2].to_string()
        };

        // Handle arrow notation in path: "old_name => new_name"
        let resolved_path = if path.contains(" => ") {
            resolve_arrow_path(&path)
        } else {
            path.clone()
        };

        let (status, old_path) = status_map
            .remove(&resolved_path)
            .unwrap_or((FileStatus::Modified, None));

        total_additions += additions;
        total_deletions += deletions;

        files.push(FileChange {
            path: resolved_path,
            old_path,
            status,
            additions,
            deletions,
            binary,
        });
    }

    // Check for merge conflicts
    let conflict_output = git_allow_empty(worktree_path, &["ls-files", "-u"])?;
    let conflict_files: Vec<String> = conflict_output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            parts.get(1).map(|s| s.to_string())
        })
        .collect();
    let conflict_files = deduplicate(conflict_files);
    let has_conflicts = !conflict_files.is_empty();

    Ok(WorktreeStatus {
        workspace_id: workspace_id.to_string(),
        base_branch: base_branch.to_string(),
        head_sha,
        base_sha,
        ahead,
        behind,
        files_changed: files.len() as u32,
        total_additions,
        total_deletions,
        files,
        has_conflicts,
        conflict_files,
    })
}

fn resolve_arrow_path(path: &str) -> String {
    // Handle git's rename notation: "{old => new}/rest" or "dir/{old => new}"
    if let Some(start) = path.find('{') {
        if let Some(end) = path.find('}') {
            let prefix = &path[..start];
            let suffix = &path[end + 1..];
            let inner = &path[start + 1..end];
            if let Some((_old, new)) = inner.split_once(" => ") {
                return format!("{prefix}{new}{suffix}");
            }
        }
    }
    // Simple "old => new"
    if let Some((_old, new)) = path.split_once(" => ") {
        return new.to_string();
    }
    path.to_string()
}

fn deduplicate(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

pub fn read_file_at_version(
    worktree_path: &str,
    path: &str,
    version: &FileVersion,
    base_branch: &str,
) -> Result<String, String> {
    match version {
        FileVersion::Working => {
            let full_path = std::path::Path::new(worktree_path).join(path);
            std::fs::read_to_string(&full_path).map_err(|e| format!("read error: {e}"))
        }
        FileVersion::Base => {
            let base_sha = resolve_merge_base(worktree_path, base_branch)?;
            git(worktree_path, &["show", &format!("{base_sha}:{path}")])
        }
    }
}

pub fn merge_branch(
    repo_path: &str,
    worktree_branch: &str,
    base_branch: &str,
    strategy: &MergeStrategy,
    commit_message: Option<&str>,
) -> Result<MergeResult, String> {
    match strategy {
        MergeStrategy::Merge => {
            // Checkout base branch in main repo
            git(repo_path, &["checkout", base_branch])?;

            let output = Command::new("git")
                .args(["merge", worktree_branch])
                .current_dir(repo_path)
                .output()
                .map_err(|e| format!("git exec error: {e}"))?;

            if output.status.success() {
                let sha = git(repo_path, &["rev-parse", "HEAD"])?
                    .trim()
                    .to_string();
                Ok(MergeResult {
                    success: true,
                    merge_sha: Some(sha),
                    conflicts: vec![],
                    message: format!("Merged {worktree_branch} into {base_branch}"),
                })
            } else {
                let conflicts = collect_conflicts(repo_path);
                // Abort the failed merge
                let _ = git(repo_path, &["merge", "--abort"]);
                Ok(MergeResult {
                    success: false,
                    merge_sha: None,
                    conflicts,
                    message: "Merge failed due to conflicts".to_string(),
                })
            }
        }
        MergeStrategy::Squash => {
            git(repo_path, &["checkout", base_branch])?;

            let output = Command::new("git")
                .args(["merge", "--squash", worktree_branch])
                .current_dir(repo_path)
                .output()
                .map_err(|e| format!("git exec error: {e}"))?;

            if output.status.success() {
                let default_msg = format!("Squash merge {worktree_branch}");
                let msg = commit_message.unwrap_or(&default_msg);
                let commit_output = Command::new("git")
                    .args(["commit", "-m", msg])
                    .current_dir(repo_path)
                    .output()
                    .map_err(|e| format!("git exec error: {e}"))?;

                if commit_output.status.success() {
                    let sha = git(repo_path, &["rev-parse", "HEAD"])?
                        .trim()
                        .to_string();
                    Ok(MergeResult {
                        success: true,
                        merge_sha: Some(sha),
                        conflicts: vec![],
                        message: format!("Squash-merged {worktree_branch} into {base_branch}"),
                    })
                } else {
                    let stderr = String::from_utf8_lossy(&commit_output.stderr);
                    // Reset the squash state
                    let _ = git(repo_path, &["reset", "HEAD"]);
                    Err(format!("commit after squash failed: {stderr}"))
                }
            } else {
                let conflicts = collect_conflicts(repo_path);
                let _ = git(repo_path, &["reset", "HEAD"]);
                let _ = git(repo_path, &["checkout", "."]);
                Ok(MergeResult {
                    success: false,
                    merge_sha: None,
                    conflicts,
                    message: "Squash merge failed due to conflicts".to_string(),
                })
            }
        }
        MergeStrategy::Rebase => {
            // Rebase is done in the worktree, then fast-forward in main repo
            // We need the worktree path — for rebase we run from the repo perspective
            // First: find the worktree path from the branch
            let worktree_list = git(repo_path, &["worktree", "list", "--porcelain"])?;
            let worktree_path = find_worktree_path(&worktree_list, worktree_branch);

            if let Some(wt_path) = worktree_path {
                let output = Command::new("git")
                    .args(["rebase", base_branch])
                    .current_dir(&wt_path)
                    .output()
                    .map_err(|e| format!("git exec error: {e}"))?;

                if !output.status.success() {
                    let _ = Command::new("git")
                        .args(["rebase", "--abort"])
                        .current_dir(&wt_path)
                        .output();
                    let conflicts = collect_conflicts(&wt_path);
                    return Ok(MergeResult {
                        success: false,
                        merge_sha: None,
                        conflicts,
                        message: "Rebase failed due to conflicts".to_string(),
                    });
                }

                // Fast-forward merge in main repo
                git(repo_path, &["checkout", base_branch])?;
                git(repo_path, &["merge", "--ff-only", worktree_branch])?;

                let sha = git(repo_path, &["rev-parse", "HEAD"])?
                    .trim()
                    .to_string();
                Ok(MergeResult {
                    success: true,
                    merge_sha: Some(sha),
                    conflicts: vec![],
                    message: format!("Rebased and merged {worktree_branch} into {base_branch}"),
                })
            } else {
                Err(format!(
                    "could not find worktree for branch {worktree_branch}"
                ))
            }
        }
    }
}

fn find_worktree_path(porcelain_output: &str, branch: &str) -> Option<String> {
    let mut current_path: Option<String> = None;
    for line in porcelain_output.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            current_path = Some(path.to_string());
        } else if line.starts_with("branch ") && line.contains(branch) {
            return current_path;
        } else if line.is_empty() {
            current_path = None;
        }
    }
    None
}

fn collect_conflicts(repo_path: &str) -> Vec<String> {
    let output = git_allow_empty(repo_path, &["diff", "--name-only", "--diff-filter=U"]);
    match output {
        Ok(text) => text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect(),
        Err(_) => vec![],
    }
}

pub fn discard_worktree(
    repo_path: &str,
    worktree_path: &str,
    branch_name: &str,
) -> Result<(), String> {
    // Remove the worktree
    let output = Command::new("git")
        .args(["worktree", "remove", "--force", worktree_path])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git worktree remove error: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("worktree remove warning: {stderr}");
        // Force-cleanup directory
        let _ = std::fs::remove_dir_all(worktree_path);
    }

    // Delete the branch
    let _ = Command::new("git")
        .args(["branch", "-D", branch_name])
        .current_dir(repo_path)
        .output();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_status_letter tests ──

    #[test]
    fn test_parse_status_letter_added() {
        assert_eq!(parse_status_letter("A"), FileStatus::Added);
    }

    #[test]
    fn test_parse_status_letter_deleted() {
        assert_eq!(parse_status_letter("D"), FileStatus::Deleted);
    }

    #[test]
    fn test_parse_status_letter_modified() {
        assert_eq!(parse_status_letter("M"), FileStatus::Modified);
    }

    #[test]
    fn test_parse_status_letter_renamed() {
        assert_eq!(parse_status_letter("R100"), FileStatus::Renamed);
        assert_eq!(parse_status_letter("R050"), FileStatus::Renamed);
    }

    #[test]
    fn test_parse_status_letter_copied() {
        assert_eq!(parse_status_letter("C100"), FileStatus::Copied);
    }

    #[test]
    fn test_parse_status_letter_unknown_falls_back_to_modified() {
        assert_eq!(parse_status_letter("X"), FileStatus::Modified);
        assert_eq!(parse_status_letter(""), FileStatus::Modified);
    }

    // ── parse_hunk_header tests ──

    #[test]
    fn test_parse_hunk_header_standard() {
        let result = parse_hunk_header("@@ -10,5 +20,8 @@ fn foo()");
        assert_eq!(result, Some((10, 5, 20, 8)));
    }

    #[test]
    fn test_parse_hunk_header_single_line() {
        // When count is omitted, it defaults to 1
        let result = parse_hunk_header("@@ -1 +1 @@");
        assert_eq!(result, Some((1, 1, 1, 1)));
    }

    #[test]
    fn test_parse_hunk_header_zero_count() {
        let result = parse_hunk_header("@@ -0,0 +1,3 @@");
        assert_eq!(result, Some((0, 0, 1, 3)));
    }

    #[test]
    fn test_parse_hunk_header_invalid() {
        assert_eq!(parse_hunk_header("not a hunk"), None);
        assert_eq!(parse_hunk_header("@@ invalid @@"), None);
    }

    // ── parse_range tests ──

    #[test]
    fn test_parse_range_with_comma() {
        assert_eq!(parse_range("10,5"), (10, 5));
    }

    #[test]
    fn test_parse_range_without_comma() {
        assert_eq!(parse_range("42"), (42, 1));
    }

    #[test]
    fn test_parse_range_invalid() {
        assert_eq!(parse_range("abc"), (0, 1));
        assert_eq!(parse_range("abc,def"), (0, 0));
    }

    // ── parse_diff_git_header tests ──

    #[test]
    fn test_parse_diff_git_header_simple() {
        let (old, new) = parse_diff_git_header("diff --git a/src/main.rs b/src/main.rs");
        assert_eq!(old, "src/main.rs");
        assert_eq!(new, "src/main.rs");
    }

    #[test]
    fn test_parse_diff_git_header_different_paths() {
        let (old, new) = parse_diff_git_header("diff --git a/old/file.rs b/new/file.rs");
        assert_eq!(old, "old/file.rs");
        assert_eq!(new, "new/file.rs");
    }

    #[test]
    fn test_parse_diff_git_header_with_spaces() {
        let (old, new) = parse_diff_git_header("diff --git a/my file.txt b/my file.txt");
        assert_eq!(old, "my file.txt");
        assert_eq!(new, "my file.txt");
    }

    // ── resolve_arrow_path tests ──

    #[test]
    fn test_resolve_arrow_path_braces() {
        assert_eq!(
            resolve_arrow_path("dir/{old.rs => new.rs}"),
            "dir/new.rs"
        );
    }

    #[test]
    fn test_resolve_arrow_path_braces_with_prefix_suffix() {
        assert_eq!(
            resolve_arrow_path("src/{old => new}/file.rs"),
            "src/new/file.rs"
        );
    }

    #[test]
    fn test_resolve_arrow_path_simple_rename() {
        assert_eq!(resolve_arrow_path("old.rs => new.rs"), "new.rs");
    }

    #[test]
    fn test_resolve_arrow_path_no_arrow() {
        assert_eq!(resolve_arrow_path("plain/file.rs"), "plain/file.rs");
    }

    // ── deduplicate tests ──

    #[test]
    fn test_deduplicate() {
        let input = vec![
            "b".to_string(),
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "a".to_string(),
        ];
        assert_eq!(
            deduplicate(input),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn test_deduplicate_empty() {
        let input: Vec<String> = vec![];
        let result: Vec<String> = vec![];
        assert_eq!(deduplicate(input), result);
    }

    // ── parse_unified_diff tests ──

    #[test]
    fn test_parse_unified_diff_empty() {
        let files = parse_unified_diff("");
        assert!(files.is_empty());
    }

    #[test]
    fn test_parse_unified_diff_simple_modification() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc1234..def5678 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
-    println!(\"hello\");
+    println!(\"hello world\");
+    println!(\"goodbye\");
 }
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.path, "src/main.rs");
        assert_eq!(f.status, FileStatus::Modified);
        assert!(!f.binary);
        assert_eq!(f.additions, 2);
        assert_eq!(f.deletions, 1);
        assert_eq!(f.hunks.len(), 1);

        let hunk = &f.hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.old_count, 3);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.new_count, 4);

        // Verify line kinds
        let kinds: Vec<&str> = hunk.lines.iter().map(|l| l.kind.as_str()).collect();
        assert_eq!(kinds, vec!["context", "delete", "add", "add", "context"]);
    }

    #[test]
    fn test_parse_unified_diff_new_file() {
        let diff = "\
diff --git a/new_file.txt b/new_file.txt
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/new_file.txt
@@ -0,0 +1,2 @@
+line one
+line two
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Added);
        assert_eq!(files[0].additions, 2);
        assert_eq!(files[0].deletions, 0);
    }

    #[test]
    fn test_parse_unified_diff_deleted_file() {
        let diff = "\
diff --git a/old_file.txt b/old_file.txt
deleted file mode 100644
index abc1234..0000000
--- a/old_file.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-line one
-line two
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Deleted);
        assert_eq!(files[0].additions, 0);
        assert_eq!(files[0].deletions, 2);
    }

    #[test]
    fn test_parse_unified_diff_renamed_file() {
        let diff = "\
diff --git a/old_name.rs b/new_name.rs
similarity index 95%
rename from old_name.rs
rename to new_name.rs
index abc1234..def5678 100644
--- a/old_name.rs
+++ b/new_name.rs
@@ -1,3 +1,3 @@
 fn foo() {
-    old();
+    new();
 }
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Renamed);
        assert_eq!(files[0].path, "new_name.rs");
        assert_eq!(files[0].old_path, Some("old_name.rs".to_string()));
    }

    #[test]
    fn test_parse_unified_diff_binary_file() {
        let diff = "\
diff --git a/image.png b/image.png
new file mode 100644
index 0000000..abc1234
Binary files /dev/null and b/image.png differ
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert!(files[0].binary);
        assert_eq!(files[0].hunks.len(), 0);
    }

    #[test]
    fn test_parse_unified_diff_git_binary_patch() {
        let diff = "\
diff --git a/data.bin b/data.bin
index abc1234..def5678 100644
GIT binary patch
literal 1234
some binary patch data here
more data

diff --git a/other.txt b/other.txt
index 111..222 100644
--- a/other.txt
+++ b/other.txt
@@ -1 +1 @@
-old
+new
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 2);
        assert!(files[0].binary);
        assert_eq!(files[0].path, "data.bin");
        assert!(!files[1].binary);
        assert_eq!(files[1].path, "other.txt");
    }

    #[test]
    fn test_parse_unified_diff_multiple_hunks() {
        let diff = "\
diff --git a/file.rs b/file.rs
index abc..def 100644
--- a/file.rs
+++ b/file.rs
@@ -1,3 +1,3 @@
 fn a() {
-    old_a();
+    new_a();
 }
@@ -10,3 +10,3 @@
 fn b() {
-    old_b();
+    new_b();
 }
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks.len(), 2);
        assert_eq!(files[0].hunks[0].old_start, 1);
        assert_eq!(files[0].hunks[1].old_start, 10);
    }

    #[test]
    fn test_parse_unified_diff_multiple_files() {
        let diff = "\
diff --git a/a.txt b/a.txt
index 111..222 100644
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-old a
+new a
diff --git a/b.txt b/b.txt
new file mode 100644
index 0000000..333
--- /dev/null
+++ b/b.txt
@@ -0,0 +1 @@
+new file
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(files[0].status, FileStatus::Modified);
        assert_eq!(files[1].path, "b.txt");
        assert_eq!(files[1].status, FileStatus::Added);
    }

    #[test]
    fn test_parse_unified_diff_no_newline_marker() {
        let diff = "\
diff --git a/file.txt b/file.txt
index abc..def 100644
--- a/file.txt
+++ b/file.txt
@@ -1 +1 @@
-old line
\\ No newline at end of file
+new line
\\ No newline at end of file
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].additions, 1);
        assert_eq!(files[0].deletions, 1);
    }

    #[test]
    fn test_parse_unified_diff_line_numbers() {
        let diff = "\
diff --git a/f.rs b/f.rs
index abc..def 100644
--- a/f.rs
+++ b/f.rs
@@ -5,4 +5,5 @@
 context line
-deleted line
+added line 1
+added line 2
 another context
";
        let files = parse_unified_diff(diff);
        let hunk = &files[0].hunks[0];

        // First line: context at old_lineno=5, new_lineno=5
        assert_eq!(hunk.lines[0].kind, "context");
        assert_eq!(hunk.lines[0].old_lineno, Some(5));
        assert_eq!(hunk.lines[0].new_lineno, Some(5));

        // Deleted line: old_lineno=6, no new_lineno
        assert_eq!(hunk.lines[1].kind, "delete");
        assert_eq!(hunk.lines[1].old_lineno, Some(6));
        assert_eq!(hunk.lines[1].new_lineno, None);

        // Added lines: no old_lineno, new_lineno=6, 7
        assert_eq!(hunk.lines[2].kind, "add");
        assert_eq!(hunk.lines[2].old_lineno, None);
        assert_eq!(hunk.lines[2].new_lineno, Some(6));

        assert_eq!(hunk.lines[3].kind, "add");
        assert_eq!(hunk.lines[3].old_lineno, None);
        assert_eq!(hunk.lines[3].new_lineno, Some(7));

        // Context: old_lineno=7, new_lineno=8
        assert_eq!(hunk.lines[4].kind, "context");
        assert_eq!(hunk.lines[4].old_lineno, Some(7));
        assert_eq!(hunk.lines[4].new_lineno, Some(8));
    }

    #[test]
    fn test_parse_unified_diff_copied_file() {
        let diff = "\
diff --git a/original.rs b/copy.rs
similarity index 100%
copy from original.rs
copy to copy.rs
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Copied);
        assert_eq!(files[0].old_path, Some("original.rs".to_string()));
        assert_eq!(files[0].path, "copy.rs");
    }

    // ── find_worktree_path tests ──

    #[test]
    fn test_find_worktree_path_found() {
        let output = "\
worktree /home/user/repo
branch refs/heads/main
HEAD abc123

worktree /home/user/repo/.worktrees/feature-abc
branch refs/heads/feature-abc
HEAD def456

";
        assert_eq!(
            find_worktree_path(output, "feature-abc"),
            Some("/home/user/repo/.worktrees/feature-abc".to_string())
        );
    }

    #[test]
    fn test_find_worktree_path_not_found() {
        let output = "\
worktree /home/user/repo
branch refs/heads/main
HEAD abc123

";
        assert_eq!(find_worktree_path(output, "nonexistent"), None);
    }

    #[test]
    fn test_find_worktree_path_empty() {
        assert_eq!(find_worktree_path("", "branch"), None);
    }

    // ── Additional parser and serde tests ──

    #[test]
    fn test_parse_diff_only_additions_new_file() {
        let diff = "\
diff --git a/brand_new.rs b/brand_new.rs
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/brand_new.rs
@@ -0,0 +1,5 @@
+fn main() {
+    println!(\"hello\");
+    println!(\"world\");
+    let x = 1;
+    let y = 2;
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Added);
        assert_eq!(files[0].additions, 5);
        assert_eq!(files[0].deletions, 0);
        assert_eq!(files[0].hunks.len(), 1);
        // All lines in the hunk should be "add"
        for line in &files[0].hunks[0].lines {
            assert_eq!(line.kind, "add");
            assert!(line.new_lineno.is_some());
            assert!(line.old_lineno.is_none());
        }
    }

    #[test]
    fn test_parse_diff_only_deletions_deleted_file() {
        let diff = "\
diff --git a/removed.rs b/removed.rs
deleted file mode 100644
index abc1234..0000000
--- a/removed.rs
+++ /dev/null
@@ -1,4 +0,0 @@
-fn old() {
-    println!(\"going away\");
-    let a = 1;
-}
";
        let files = parse_unified_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, FileStatus::Deleted);
        assert_eq!(files[0].additions, 0);
        assert_eq!(files[0].deletions, 4);
        assert_eq!(files[0].hunks.len(), 1);
        // All lines should be "delete"
        for line in &files[0].hunks[0].lines {
            assert_eq!(line.kind, "delete");
            assert!(line.old_lineno.is_some());
            assert!(line.new_lineno.is_none());
        }
    }

    #[test]
    fn test_parse_empty_diff_output() {
        let files = parse_unified_diff("");
        assert!(files.is_empty());

        let files2 = parse_unified_diff("\n\n\n");
        assert!(files2.is_empty());

        let files3 = parse_unified_diff("some random text that is not a diff");
        assert!(files3.is_empty());
    }

    #[test]
    fn test_file_status_serde_roundtrip_all_variants() {
        let variants = vec![
            FileStatus::Added,
            FileStatus::Modified,
            FileStatus::Deleted,
            FileStatus::Renamed,
            FileStatus::Copied,
        ];
        for status in variants {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: FileStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_file_status_serde_snake_case() {
        assert_eq!(serde_json::to_string(&FileStatus::Added).unwrap(), "\"added\"");
        assert_eq!(serde_json::to_string(&FileStatus::Modified).unwrap(), "\"modified\"");
        assert_eq!(serde_json::to_string(&FileStatus::Deleted).unwrap(), "\"deleted\"");
        assert_eq!(serde_json::to_string(&FileStatus::Renamed).unwrap(), "\"renamed\"");
        assert_eq!(serde_json::to_string(&FileStatus::Copied).unwrap(), "\"copied\"");
    }

    #[test]
    fn test_merge_strategy_serde_roundtrip() {
        let variants = vec![
            MergeStrategy::Merge,
            MergeStrategy::Squash,
            MergeStrategy::Rebase,
        ];
        for strategy in variants {
            let json = serde_json::to_string(&strategy).unwrap();
            let parsed: MergeStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, strategy);
        }
    }

    #[test]
    fn test_merge_strategy_serde_snake_case() {
        assert_eq!(serde_json::to_string(&MergeStrategy::Merge).unwrap(), "\"merge\"");
        assert_eq!(serde_json::to_string(&MergeStrategy::Squash).unwrap(), "\"squash\"");
        assert_eq!(serde_json::to_string(&MergeStrategy::Rebase).unwrap(), "\"rebase\"");
    }

    #[test]
    fn test_file_version_serde_roundtrip() {
        let variants = vec![FileVersion::Base, FileVersion::Working];
        for version in variants {
            let json = serde_json::to_string(&version).unwrap();
            let parsed: FileVersion = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, version);
        }
    }

    #[test]
    fn test_file_version_serde_snake_case() {
        assert_eq!(serde_json::to_string(&FileVersion::Base).unwrap(), "\"base\"");
        assert_eq!(serde_json::to_string(&FileVersion::Working).unwrap(), "\"working\"");
    }

    #[test]
    fn test_merge_result_serde_roundtrip_success() {
        let result = MergeResult {
            success: true,
            merge_sha: Some("abc123def456".to_string()),
            conflicts: vec![],
            message: "Merged feature into main".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: MergeResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.success);
        assert_eq!(parsed.merge_sha, Some("abc123def456".to_string()));
        assert!(parsed.conflicts.is_empty());
        assert_eq!(parsed.message, "Merged feature into main");
    }

    #[test]
    fn test_merge_result_serde_roundtrip_failure() {
        let result = MergeResult {
            success: false,
            merge_sha: None,
            conflicts: vec!["file1.rs".to_string(), "file2.rs".to_string()],
            message: "Merge failed due to conflicts".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: MergeResult = serde_json::from_str(&json).unwrap();
        assert!(!parsed.success);
        assert!(parsed.merge_sha.is_none());
        assert_eq!(parsed.conflicts.len(), 2);
        assert!(parsed.conflicts.contains(&"file1.rs".to_string()));
    }

    #[test]
    fn test_worktree_status_serde_roundtrip() {
        let status = WorktreeStatus {
            workspace_id: "ws-123".to_string(),
            base_branch: "main".to_string(),
            head_sha: "abc123".to_string(),
            base_sha: "def456".to_string(),
            ahead: 3,
            behind: 1,
            files_changed: 5,
            total_additions: 42,
            total_deletions: 10,
            files: vec![
                FileChange {
                    path: "src/main.rs".to_string(),
                    old_path: None,
                    status: FileStatus::Modified,
                    additions: 20,
                    deletions: 5,
                    binary: false,
                },
                FileChange {
                    path: "new_file.txt".to_string(),
                    old_path: None,
                    status: FileStatus::Added,
                    additions: 10,
                    deletions: 0,
                    binary: false,
                },
            ],
            has_conflicts: true,
            conflict_files: vec!["conflict.rs".to_string()],
        };

        let json = serde_json::to_string(&status).unwrap();
        let parsed: WorktreeStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.workspace_id, "ws-123");
        assert_eq!(parsed.base_branch, "main");
        assert_eq!(parsed.head_sha, "abc123");
        assert_eq!(parsed.base_sha, "def456");
        assert_eq!(parsed.ahead, 3);
        assert_eq!(parsed.behind, 1);
        assert_eq!(parsed.files_changed, 5);
        assert_eq!(parsed.total_additions, 42);
        assert_eq!(parsed.total_deletions, 10);
        assert_eq!(parsed.files.len(), 2);
        assert!(parsed.has_conflicts);
        assert_eq!(parsed.conflict_files, vec!["conflict.rs"]);
    }

    #[test]
    fn test_worktree_status_serde_empty_state() {
        let status = WorktreeStatus {
            workspace_id: "ws-empty".to_string(),
            base_branch: "main".to_string(),
            head_sha: "aaa".to_string(),
            base_sha: "bbb".to_string(),
            ahead: 0,
            behind: 0,
            files_changed: 0,
            total_additions: 0,
            total_deletions: 0,
            files: vec![],
            has_conflicts: false,
            conflict_files: vec![],
        };

        let json = serde_json::to_string(&status).unwrap();
        let parsed: WorktreeStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.ahead, 0);
        assert_eq!(parsed.behind, 0);
        assert!(parsed.files.is_empty());
        assert!(!parsed.has_conflicts);
        assert!(parsed.conflict_files.is_empty());
    }

    #[test]
    fn test_file_change_serde_roundtrip() {
        let change = FileChange {
            path: "renamed.rs".to_string(),
            old_path: Some("original.rs".to_string()),
            status: FileStatus::Renamed,
            additions: 5,
            deletions: 3,
            binary: false,
        };
        let json = serde_json::to_string(&change).unwrap();
        let parsed: FileChange = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, "renamed.rs");
        assert_eq!(parsed.old_path, Some("original.rs".to_string()));
        assert_eq!(parsed.status, FileStatus::Renamed);
        assert_eq!(parsed.additions, 5);
        assert_eq!(parsed.deletions, 3);
        assert!(!parsed.binary);
    }

    #[test]
    fn test_file_change_binary_serde() {
        let change = FileChange {
            path: "image.png".to_string(),
            old_path: None,
            status: FileStatus::Added,
            additions: 0,
            deletions: 0,
            binary: true,
        };
        let json = serde_json::to_string(&change).unwrap();
        let parsed: FileChange = serde_json::from_str(&json).unwrap();
        assert!(parsed.binary);
    }

    #[test]
    fn test_diff_hunk_serde_roundtrip() {
        let hunk = DiffHunk {
            old_start: 10,
            old_count: 5,
            new_start: 12,
            new_count: 7,
            header: "@@ -10,5 +12,7 @@ fn test()".to_string(),
            lines: vec![
                DiffLine {
                    kind: "context".to_string(),
                    old_lineno: Some(10),
                    new_lineno: Some(12),
                    content: "existing line".to_string(),
                },
                DiffLine {
                    kind: "add".to_string(),
                    old_lineno: None,
                    new_lineno: Some(13),
                    content: "new line".to_string(),
                },
            ],
        };
        let json = serde_json::to_string(&hunk).unwrap();
        let parsed: DiffHunk = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.old_start, 10);
        assert_eq!(parsed.new_count, 7);
        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(parsed.lines[1].kind, "add");
    }

    #[test]
    fn test_file_diff_serde_roundtrip() {
        let diff = FileDiff {
            path: "src/lib.rs".to_string(),
            old_path: None,
            status: FileStatus::Modified,
            binary: false,
            hunks: vec![],
            additions: 10,
            deletions: 3,
        };
        let json = serde_json::to_string(&diff).unwrap();
        let parsed: FileDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, "src/lib.rs");
        assert_eq!(parsed.status, FileStatus::Modified);
        assert_eq!(parsed.additions, 10);
        assert_eq!(parsed.deletions, 3);
    }

    #[test]
    fn test_deduplicate_single_element() {
        let input = vec!["only".to_string()];
        assert_eq!(deduplicate(input), vec!["only".to_string()]);
    }

    #[test]
    fn test_deduplicate_all_same() {
        let input = vec!["a".to_string(), "a".to_string(), "a".to_string()];
        assert_eq!(deduplicate(input), vec!["a".to_string()]);
    }

    #[test]
    fn test_resolve_arrow_path_empty_new() {
        // Edge case: "old => " (empty new)
        assert_eq!(resolve_arrow_path("old => "), "");
    }

    #[test]
    fn test_parse_hunk_header_large_numbers() {
        let result = parse_hunk_header("@@ -1000,200 +2000,300 @@ fn large()");
        assert_eq!(result, Some((1000, 200, 2000, 300)));
    }
}
