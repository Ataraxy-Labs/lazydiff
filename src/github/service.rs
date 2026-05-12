use std::{env, process::Command as ProcessCommand};

use serde::Deserialize;

use crate::app::WorkItemKind;

use super::models::{
    CheckRollupStatus, GitHubCheck, GitHubComment, GitHubPullRequest, GitHubQueue,
    GitHubQueueStatus, ReviewStatus,
};
use super::patch::{parse_pull_request_files_value, pull_request_files_to_patch};
use super::worktree::GitCommit;

pub(crate) fn github_token() -> Option<String> {
    env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| env::var("GH_TOKEN").ok())
        .filter(|token| !token.trim().is_empty())
        .or_else(gh_auth_token)
}

fn gh_auth_token() -> Option<String> {
    let output = ProcessCommand::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!token.is_empty()).then_some(token)
}

#[derive(Deserialize)]
struct RawRestComment {
    user: Option<RawRestUser>,
    body: Option<String>,
    created_at: Option<String>,
}

#[derive(Deserialize)]
struct RawRestUser {
    login: String,
}

pub(crate) fn fetch_pull_request_comments(
    repository: &str,
    number: u32,
) -> std::result::Result<Vec<GitHubComment>, String> {
    let mut comments = fetch_pull_request_comment_endpoint(repository, number, "issues")?;
    comments.extend(fetch_pull_request_comment_endpoint(
        repository, number, "pulls",
    )?);
    comments.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(comments)
}

fn fetch_pull_request_comment_endpoint(
    repository: &str,
    number: u32,
    kind: &str,
) -> std::result::Result<Vec<GitHubComment>, String> {
    let endpoint = match kind {
        "issues" => format!("repos/{repository}/issues/{number}/comments"),
        "pulls" => format!("repos/{repository}/pulls/{number}/comments"),
        _ => return Ok(Vec::new()),
    };
    let output = ProcessCommand::new("gh")
        .args(["api", "--paginate", "--slurp", &endpoint])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("gh exited with {}", output.status)
        } else {
            stderr
        });
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("failed to parse gh comments JSON: {error}"))?;
    let comments = parse_rest_comments_value(value)?;
    Ok(comments
        .into_iter()
        .map(|comment| GitHubComment {
            author: comment
                .user
                .map(|user| user.login)
                .unwrap_or_else(|| "unknown".to_string()),
            body: comment.body.unwrap_or_default(),
            created_at: comment.created_at.unwrap_or_default(),
        })
        .collect())
}

fn parse_rest_comments_value(
    value: serde_json::Value,
) -> std::result::Result<Vec<RawRestComment>, String> {
    match value {
        serde_json::Value::Array(values) => {
            let flattened = if values.first().is_some_and(|value| value.is_array()) {
                values
                    .into_iter()
                    .flat_map(|value| match value {
                        serde_json::Value::Array(page) => page,
                        other => vec![other],
                    })
                    .collect::<Vec<_>>()
            } else {
                values
            };
            flattened
                .into_iter()
                .map(serde_json::from_value)
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())
        }
        other => serde_json::from_value(other)
            .map_err(|error| format!("unexpected PR comments JSON: {error}")),
    }
}

pub(crate) fn fetch_pull_request_patch(
    repository: &str,
    number: u32,
) -> std::result::Result<String, String> {
    let output = ProcessCommand::new("gh")
        .args([
            "api",
            "--paginate",
            "--slurp",
            &format!("repos/{repository}/pulls/{number}/files"),
        ])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("gh exited with {}", output.status)
        } else {
            stderr
        });
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("failed to parse gh files JSON: {error}"))?;
    let files = parse_pull_request_files_value(value)?;
    Ok(pull_request_files_to_patch(&files))
}

pub(crate) fn fetch_commit_patch(
    repository: &str,
    sha: &str,
) -> std::result::Result<String, String> {
    let output = ProcessCommand::new("gh")
        .args(["api", &format!("repos/{repository}/commits/{sha}")])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("gh exited with {}", output.status)
        } else {
            stderr
        });
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("failed to parse gh commit JSON: {error}"))?;
    let files = parse_pull_request_files_value(value.get("files").cloned().unwrap_or_default())?;
    Ok(pull_request_files_to_patch(&files))
}

pub(crate) fn fetch_pull_request_commits(
    repository: &str,
    number: u32,
) -> std::result::Result<Vec<GitCommit>, String> {
    let output = ProcessCommand::new("gh")
        .args([
            "api",
            "--paginate",
            "--slurp",
            &format!("repos/{repository}/pulls/{number}/commits"),
        ])
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("gh exited with {}", output.status)
        } else {
            stderr
        });
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("failed to parse gh PR commits JSON: {error}"))?;
    parse_pull_request_commits_value(value)
}

#[derive(Deserialize)]
struct RawRestCommit {
    sha: String,
    commit: RawRestCommitInner,
}

#[derive(Deserialize)]
struct RawRestCommitInner {
    message: String,
    author: Option<RawRestCommitAuthor>,
}

#[derive(Deserialize)]
struct RawRestCommitAuthor {
    name: Option<String>,
    date: Option<String>,
}

fn parse_pull_request_commits_value(
    value: serde_json::Value,
) -> std::result::Result<Vec<GitCommit>, String> {
    let values = match value {
        serde_json::Value::Array(values) => {
            if values.first().is_some_and(|value| value.is_array()) {
                values
                    .into_iter()
                    .flat_map(|value| match value {
                        serde_json::Value::Array(page) => page,
                        other => vec![other],
                    })
                    .collect::<Vec<_>>()
            } else {
                values
            }
        }
        other => vec![other],
    };
    values
        .into_iter()
        .map(|value| {
            let raw: RawRestCommit =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            let subject = raw
                .commit
                .message
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            let short_sha = raw.sha.chars().take(7).collect::<String>();
            let author = raw
                .commit
                .author
                .as_ref()
                .and_then(|author| author.name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let authored_at = raw
                .commit
                .author
                .and_then(|author| author.date)
                .unwrap_or_default();
            Ok(GitCommit {
                sha: raw.sha,
                short_sha,
                subject,
                author,
                authored_at,
                files: Vec::new(),
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct GraphQlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitHubQueueData {
    viewer: GitHubViewer,
    authored: GitHubSearchConnection,
    review_requested: GitHubSearchConnection,
}

#[derive(Deserialize)]
struct GitHubViewer {
    login: String,
}

#[derive(Deserialize)]
struct GitHubSearchConnection {
    nodes: Vec<Option<RawPullRequest>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPullRequest {
    number: u32,
    title: String,
    body: String,
    is_draft: bool,
    review_decision: Option<String>,
    additions: usize,
    deletions: usize,
    changed_files: usize,
    created_at: String,
    author: RawAuthor,
    head_ref_name: String,
    repository: RawRepository,
    comments: RawIssueComments,
    review_threads: RawReviewThreads,
    status_check_rollup: Option<RawStatusCheckRollup>,
}

#[derive(Deserialize)]
struct RawAuthor {
    login: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRepository {
    name_with_owner: String,
}

#[derive(Deserialize)]
struct RawIssueComments {
    nodes: Vec<Option<RawIssueComment>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIssueComment {
    author: RawAuthor,
    body: String,
    created_at: String,
}

#[derive(Deserialize)]
struct RawReviewThreads {
    nodes: Vec<Option<RawReviewThread>>,
}

#[derive(Deserialize)]
struct RawReviewThread {
    comments: RawReviewComments,
}

#[derive(Deserialize)]
struct RawReviewComments {
    nodes: Vec<Option<RawReviewComment>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReviewComment {
    author: RawAuthor,
    body: String,
    created_at: String,
}

#[derive(Deserialize)]
struct RawStatusCheckRollup {
    contexts: RawCheckContexts,
}

#[derive(Deserialize)]
struct RawCheckContexts {
    nodes: Vec<Option<RawCheckContext>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCheckContext {
    #[serde(rename = "__typename")]
    typename: String,
    name: Option<String>,
    context: Option<String>,
    status: Option<String>,
    conclusion: Option<String>,
    state: Option<String>,
}

const GITHUB_QUEUE_QUERY: &str = r#"
query QuiverQueue($authoredQuery: String!, $reviewQuery: String!, $first: Int!) {
  viewer { login }
  authored: search(query: $authoredQuery, type: ISSUE, first: $first) {
    nodes { ... on PullRequest { ...QuiverPullRequestFields } }
  }
  reviewRequested: search(query: $reviewQuery, type: ISSUE, first: $first) {
    nodes { ... on PullRequest { ...QuiverPullRequestFields } }
  }
}

fragment QuiverPullRequestFields on PullRequest {
  number
  title
  body
  isDraft
  reviewDecision
  additions
  deletions
  changedFiles
  createdAt
  updatedAt
  author { login }
  headRefName
  repository { nameWithOwner }
  statusCheckRollup {
    contexts(first: 40) {
      nodes {
        __typename
        ... on CheckRun { name status conclusion }
        ... on StatusContext { context state }
      }
    }
  }
  comments(first: 12) {
    nodes { author { login } body createdAt }
  }
  reviewThreads(first: 12) {
    nodes {
      comments(first: 12) {
        nodes { author { login } body createdAt path line originalLine }
      }
    }
  }
}
"#;

pub(crate) fn fetch_github_queue(token: &str) -> std::result::Result<GitHubQueue, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("lazydiff")
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .post("https://api.github.com/graphql")
        .bearer_auth(token)
        .json(&serde_json::json!({
            "query": GITHUB_QUEUE_QUERY,
            "variables": {
                "authoredQuery": "author:@me is:pr is:open archived:false sort:updated-desc",
                "reviewQuery": "review-requested:@me is:pr is:open archived:false sort:updated-desc",
                "first": 30,
            }
        }))
        .send()
        .map_err(|error| error.to_string())?;

    if !response.status().is_success() {
        return Err(format!("GitHub GraphQL returned {}", response.status()));
    }

    let decoded: GraphQlResponse<GitHubQueueData> =
        response.json().map_err(|error| error.to_string())?;
    if let Some(errors) = decoded.errors.filter(|errors| !errors.is_empty()) {
        return Err(errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("; "));
    }
    let data = decoded
        .data
        .ok_or_else(|| "GitHub GraphQL returned no data".to_string())?;
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in data.review_requested.nodes.into_iter().flatten() {
        let key = (raw.repository.name_with_owner.clone(), raw.number);
        if seen.insert(key) {
            items.push(parse_github_pull_request(
                raw,
                WorkItemKind::RequestedPrReview,
            ));
        }
    }
    for raw in data.authored.nodes.into_iter().flatten() {
        let key = (raw.repository.name_with_owner.clone(), raw.number);
        if seen.insert(key) {
            items.push(parse_github_pull_request(
                raw,
                WorkItemKind::OwnedPrFeedback,
            ));
        }
    }
    Ok(GitHubQueue {
        viewer: Some(data.viewer.login),
        status: GitHubQueueStatus::Ready,
        items,
        cached_at: None,
    })
}

fn parse_github_pull_request(raw: RawPullRequest, kind: WorkItemKind) -> GitHubPullRequest {
    let checks = raw
        .status_check_rollup
        .map(|rollup| {
            rollup
                .contexts
                .nodes
                .into_iter()
                .flatten()
                .map(parse_github_check)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let (check_status, check_summary) = summarize_checks(&checks);
    let mut comments: Vec<GitHubComment> = raw
        .comments
        .nodes
        .into_iter()
        .flatten()
        .map(|comment| GitHubComment {
            author: comment.author.login,
            body: comment.body,
            created_at: comment.created_at,
        })
        .collect();
    comments.extend(
        raw.review_threads
            .nodes
            .into_iter()
            .flatten()
            .flat_map(|thread| {
                thread
                    .comments
                    .nodes
                    .into_iter()
                    .flatten()
                    .map(|comment| GitHubComment {
                        author: comment.author.login,
                        body: comment.body,
                        created_at: comment.created_at,
                    })
            }),
    );
    comments.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    GitHubPullRequest {
        kind,
        repository: raw.repository.name_with_owner,
        author: raw.author.login,
        head_ref_name: raw.head_ref_name,
        number: raw.number,
        title: raw.title,
        body: raw.body,
        additions: raw.additions,
        deletions: raw.deletions,
        changed_files: raw.changed_files,
        review_status: ReviewStatus::from_raw(raw.is_draft, raw.review_decision.as_deref()),
        check_status,
        check_summary,
        checks,
        comments,
        created_at: raw.created_at,
    }
}

fn parse_github_check(raw: RawCheckContext) -> GitHubCheck {
    let name = if raw.typename == "StatusContext" {
        raw.context.unwrap_or_else(|| "status".to_string())
    } else {
        raw.name.unwrap_or_else(|| "check".to_string())
    };
    let (status, conclusion) = if raw.typename == "StatusContext" {
        let state = raw.state.unwrap_or_else(|| "PENDING".to_string());
        let conclusion = match state.as_str() {
            "SUCCESS" => Some("SUCCESS".to_string()),
            "FAILURE" | "ERROR" => Some("FAILURE".to_string()),
            _ => None,
        };
        (
            if conclusion.is_some() {
                "COMPLETED"
            } else {
                "PENDING"
            }
            .to_string(),
            conclusion,
        )
    } else {
        (
            raw.status.unwrap_or_else(|| "PENDING".to_string()),
            raw.conclusion,
        )
    };
    GitHubCheck {
        name,
        status,
        conclusion,
    }
}

fn summarize_checks(checks: &[GitHubCheck]) -> (CheckRollupStatus, Option<String>) {
    if checks.is_empty() {
        return (CheckRollupStatus::None, None);
    }
    let mut completed = 0usize;
    let mut successful = 0usize;
    let mut pending = false;
    let mut failing = false;
    for check in checks {
        if check.status == "COMPLETED" {
            completed += 1;
        } else {
            pending = true;
        }
        match check.conclusion.as_deref() {
            Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => successful += 1,
            Some(_) => failing = true,
            None => {}
        }
    }
    let summary = Some(format!("checks {successful}/{}", checks.len()));
    if pending {
        (
            CheckRollupStatus::Pending,
            Some(format!("checks {completed}/{}", checks.len())),
        )
    } else if failing {
        (CheckRollupStatus::Failing, summary)
    } else {
        (CheckRollupStatus::Passing, summary)
    }
}
