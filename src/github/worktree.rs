use std::{collections::HashMap, path::Path, process::Command as ProcessCommand};

use serde::{Deserialize, Serialize};

use super::GitHubPullRequest;

pub(crate) type ProjectKey = String;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct WorktreeId(pub(crate) String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PrId {
    pub(crate) repository: String,
    pub(crate) number: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitCommit {
    pub(crate) sha: String,
    pub(crate) short_sha: String,
    pub(crate) subject: String,
    pub(crate) author: String,
    pub(crate) authored_at: String,
    pub(crate) files: Vec<CommitFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommitFile {
    pub(crate) status: String,
    pub(crate) path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Worktree {
    pub(crate) id: WorktreeId,
    pub(crate) path: String,
    pub(crate) branch: String,
    pub(crate) head_sha: String,
    pub(crate) upstream: Option<String>,
    pub(crate) ahead: usize,
    pub(crate) behind: usize,
    pub(crate) additions: usize,
    pub(crate) deletions: usize,
    pub(crate) files_changed: usize,
    pub(crate) is_current: bool,
}

impl Worktree {
    fn from_record(record: WorktreeRecord, current_path: &Path) -> Option<Self> {
        let path = record.path?;
        let branch = record
            .branch
            .as_deref()
            .and_then(|branch| branch.strip_prefix("refs/heads/"))
            .unwrap_or_else(|| record.branch.as_deref().unwrap_or("detached-head"))
            .to_string();
        let is_current = paths_match(Path::new(&path), current_path);
        Some(Self {
            id: WorktreeId(path.clone()),
            path,
            branch,
            head_sha: record.head_sha.unwrap_or_default(),
            upstream: None,
            ahead: 0,
            behind: 0,
            additions: 0,
            deletions: 0,
            files_changed: 0,
            is_current,
        })
    }
}

#[derive(Default)]
struct WorktreeRecord {
    path: Option<String>,
    head_sha: Option<String>,
    branch: Option<String>,
}

pub(crate) fn list_worktrees(repo_root: &Path) -> std::result::Result<Vec<Worktree>, String> {
    let output = ProcessCommand::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .map_err(|error| format!("failed to run git worktree list: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git worktree list exited with {}", output.status)
        } else {
            stderr
        });
    }
    let output = String::from_utf8(output.stdout)
        .map_err(|error| format!("git worktree output was not utf-8: {error}"))?;
    let mut worktrees = parse_worktree_porcelain(&output, repo_root);
    worktrees.retain(|worktree| Path::new(&worktree.path).is_dir());
    for worktree in &mut worktrees {
        enrich_upstream_status(worktree);
    }
    Ok(worktrees)
}

pub(crate) fn parse_worktree_porcelain(input: &str, current_path: &Path) -> Vec<Worktree> {
    let mut records = Vec::new();
    let mut current = WorktreeRecord::default();
    for line in input.lines() {
        if line.trim().is_empty() {
            if current.path.is_some() {
                records.push(current);
                current = WorktreeRecord::default();
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            if current.path.is_some() {
                records.push(current);
                current = WorktreeRecord::default();
            }
            current.path = Some(path.to_string());
        } else if let Some(head_sha) = line.strip_prefix("HEAD ") {
            current.head_sha = Some(head_sha.to_string());
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current.branch = Some(branch.to_string());
        } else if line == "detached" {
            current.branch = Some("detached-head".to_string());
        }
    }
    if current.path.is_some() {
        records.push(current);
    }

    let mut worktrees: Vec<Worktree> = records
        .into_iter()
        .filter_map(|record| Worktree::from_record(record, current_path))
        .collect();
    worktrees.sort_by_key(|worktree| (!worktree.is_current, worktree.path.clone()));
    worktrees
}

pub(crate) fn link_worktree_pr(
    worktrees: &[Worktree],
    prs: &[GitHubPullRequest],
    project: &ProjectKey,
) -> HashMap<WorktreeId, PrId> {
    let mut links = HashMap::new();
    for worktree in worktrees {
        let Some(pr) = prs
            .iter()
            .filter(|pr| pr.repository == *project && pr.head_ref_name == worktree.branch)
            .max_by_key(|pr| pr.created_at.as_str())
        else {
            continue;
        };
        links.insert(
            worktree.id.clone(),
            PrId {
                repository: pr.repository.clone(),
                number: pr.number,
            },
        );
    }
    links
}

pub(crate) fn list_branch_commits(
    repo_path: &Path,
    upstream: Option<&str>,
) -> std::result::Result<Vec<GitCommit>, String> {
    if !repo_path.is_dir() {
        return Err(format!("worktree path is missing: {}", repo_path.display()));
    }
    let mut ranges = Vec::new();
    if let Some(upstream) = upstream {
        ranges.push(format!("{upstream}..HEAD"));
    }
    ranges.push("origin/main..HEAD".to_string());
    ranges.push("origin/master..HEAD".to_string());

    for range in ranges {
        if let Ok(output) = git_stdout_result(
            repo_path,
            &[
                "log",
                "--max-count=50",
                "--format=%H%x1f%h%x1f%an%x1f%at%x1f%s",
                &range,
            ],
        ) {
            let commits = enrich_commit_files(repo_path, parse_commit_log(&output));
            if !commits.is_empty() {
                return Ok(commits);
            }
        }
    }
    let output = git_stdout_result(
        repo_path,
        &["log", "-n", "20", "--format=%H%x1f%h%x1f%an%x1f%at%x1f%s"],
    )?;
    Ok(enrich_commit_files(repo_path, parse_commit_log(&output)))
}

pub(crate) fn parse_commit_log(input: &str) -> Vec<GitCommit> {
    input
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(5, '\u{1f}');
            Some(GitCommit {
                sha: parts.next()?.to_string(),
                short_sha: parts.next()?.to_string(),
                author: parts.next()?.to_string(),
                authored_at: parts.next()?.to_string(),
                subject: parts.next()?.to_string(),
                files: Vec::new(),
            })
        })
        .collect()
}

fn enrich_commit_files(repo_path: &Path, mut commits: Vec<GitCommit>) -> Vec<GitCommit> {
    for commit in &mut commits {
        if let Ok(output) = git_stdout_result(
            repo_path,
            &["show", "--format=", "--name-status", &commit.sha],
        ) {
            commit.files = parse_commit_files(&output);
        }
    }
    commits
}

fn parse_commit_files(input: &str) -> Vec<CommitFile> {
    input
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let status = parts.next()?.trim();
            let path = parts.next_back().unwrap_or(status).trim();
            if status.is_empty() || path.is_empty() || status == path {
                return None;
            }
            Some(CommitFile {
                status: status.chars().next().unwrap_or('M').to_string(),
                path: path.to_string(),
            })
        })
        .collect()
}

fn paths_match(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn enrich_upstream_status(worktree: &mut Worktree) {
    if worktree.branch == "detached-head" {
        return;
    }
    let Some(upstream) = git_stdout(
        Path::new(&worktree.path),
        ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    ) else {
        return;
    };
    worktree.upstream = Some(upstream.clone());
    if let Some(counts) = git_stdout(
        Path::new(&worktree.path),
        [
            "rev-list",
            "--left-right",
            "--count",
            &format!("{upstream}...HEAD"),
        ],
    ) {
        let mut parts = counts.split_whitespace();
        worktree.behind = parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        worktree.ahead = parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
    }
}

fn git_stdout<const N: usize>(cwd: &Path, args: [&str; N]) -> Option<String> {
    git_stdout_result(cwd, &args).ok()
}

fn git_stdout_result(cwd: &Path, args: &[&str]) -> std::result::Result<String, String> {
    let output = ProcessCommand::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git exited with {}", output.status)
        } else {
            stderr
        });
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("git output was not utf-8: {error}"))
        .map(|value| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::WorkItemKind,
        github::models::{CheckRollupStatus, ReviewStatus},
    };

    #[test]
    fn parses_porcelain_and_marks_current_first() {
        let input = "worktree /repo/sibling\nHEAD 222\nbranch refs/heads/feature\n\nworktree /repo/main\nHEAD 111\nbranch refs/heads/main\n";

        let worktrees = parse_worktree_porcelain(input, Path::new("/repo/main"));

        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[0].path, "/repo/main");
        assert_eq!(worktrees[0].branch, "main");
        assert!(worktrees[0].is_current);
        assert_eq!(worktrees[1].branch, "feature");
        assert_eq!(worktrees[1].head_sha, "222");
    }

    #[test]
    fn branch_commits_reports_missing_worktree_path_before_spawning_git() {
        let missing = Path::new("/definitely/missing/quiver/worktree");

        let error = list_branch_commits(missing, None).unwrap_err();

        assert!(error.contains("worktree path is missing"));
        assert!(error.contains("/definitely/missing/quiver/worktree"));
    }

    #[test]
    fn matcher_links_by_project_repo_and_branch() {
        let worktrees = vec![worktree("/repo/main", "feature")];
        let prs = vec![pr("owner/repo", 7, "feature", "2026-01-01T00:00:00Z")];

        let links = link_worktree_pr(&worktrees, &prs, &"owner/repo".to_string());

        assert_eq!(links.get(&worktrees[0].id).map(|id| id.number), Some(7));
    }

    #[test]
    fn matcher_allows_fork_prs_when_base_project_and_branch_match() {
        let worktrees = vec![worktree("/repo/main", "contributor-feature")];
        let prs = vec![pr(
            "owner/repo",
            8,
            "contributor-feature",
            "2026-01-01T00:00:00Z",
        )];

        let links = link_worktree_pr(&worktrees, &prs, &"owner/repo".to_string());

        assert_eq!(links.get(&worktrees[0].id).map(|id| id.number), Some(8));
    }

    #[test]
    fn matcher_does_not_link_other_projects_or_branches() {
        let worktrees = vec![worktree("/repo/main", "feature")];
        let prs = vec![
            pr("other/repo", 1, "feature", "2026-01-01T00:00:00Z"),
            pr("owner/repo", 2, "different", "2026-01-01T00:00:00Z"),
        ];

        let links = link_worktree_pr(&worktrees, &prs, &"owner/repo".to_string());

        assert!(links.is_empty());
    }

    #[test]
    fn matcher_picks_most_recent_pr_on_same_branch() {
        let worktrees = vec![worktree("/repo/main", "feature")];
        let prs = vec![
            pr("owner/repo", 1, "feature", "2026-01-01T00:00:00Z"),
            pr("owner/repo", 2, "feature", "2026-02-01T00:00:00Z"),
        ];

        let links = link_worktree_pr(&worktrees, &prs, &"owner/repo".to_string());

        assert_eq!(links.get(&worktrees[0].id).map(|id| id.number), Some(2));
    }

    fn worktree(path: &str, branch: &str) -> Worktree {
        Worktree {
            id: WorktreeId(path.to_string()),
            path: path.to_string(),
            branch: branch.to_string(),
            head_sha: "abc".to_string(),
            upstream: None,
            ahead: 0,
            behind: 0,
            additions: 0,
            deletions: 0,
            files_changed: 0,
            is_current: false,
        }
    }

    fn pr(repository: &str, number: u32, branch: &str, created_at: &str) -> GitHubPullRequest {
        GitHubPullRequest {
            kind: WorkItemKind::RequestedPrReview,
            repository: repository.to_string(),
            author: "author".to_string(),
            head_ref_name: branch.to_string(),
            number,
            title: format!("PR {number}"),
            body: String::new(),
            additions: 0,
            deletions: 0,
            changed_files: 0,
            review_status: ReviewStatus::None,
            check_status: CheckRollupStatus::None,
            check_summary: None,
            checks: Vec::new(),
            comments: Vec::new(),
            created_at: created_at.to_string(),
        }
    }
}
