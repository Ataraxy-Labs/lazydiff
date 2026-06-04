use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    path::PathBuf,
    process::Command as ProcessCommand,
    thread,
    time::Duration,
};

use convex::{ConvexClient, FunctionResult, Value};
use lazydiff_diffs::{DiffLineRangeTarget, DiffSide};
use serde::{Deserialize, Serialize};

use crate::app::WorkItemKind;
use crate::forge::credentials;
use crate::forge::{Forge, PullRequestFileSources};

use super::models::{
    CheckRollupStatus, GitHubCheck, GitHubComment, GitHubPullRequest, GitHubQueue,
    GitHubQueueStatus, ReviewStatus,
};
use super::patch::{parse_pull_request_files_value, pull_request_files_to_patch};
use super::worktree::GitCommit;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GitHubAuthStatus {
    Authenticated,
    Checking,
    MissingLogin,
}

impl GitHubAuthStatus {
    pub(crate) fn can_load_github(&self) -> bool {
        matches!(self, Self::Authenticated)
    }

    pub(crate) fn summary(&self) -> &'static str {
        match self {
            Self::Authenticated => "GitHub signed in",
            Self::Checking => "GitHub syncing…",
            Self::MissingLogin => "Sign in to load GitHub PRs",
        }
    }

    pub(crate) fn notice(&self) -> &'static str {
        match self {
            Self::Authenticated => "GitHub signed in",
            Self::Checking => "GitHub syncing…",
            Self::MissingLogin => "Sign in to load GitHub PRs · press l",
        }
    }

    pub(crate) fn error(&self) -> Option<String> {
        (!self.can_load_github()).then(|| self.notice().to_string())
    }
}

pub(crate) fn github_auth_status() -> GitHubAuthStatus {
    if env_token("GITHUB_TOKEN").is_some()
        || env_token("GH_TOKEN").is_some()
        || auth_file().exists()
    {
        GitHubAuthStatus::Authenticated
    } else {
        GitHubAuthStatus::MissingLogin
    }
}

pub(crate) fn github_token() -> Option<String> {
    env_token("GITHUB_TOKEN")
        .or_else(|| env_token("GH_TOKEN"))
        .or_else(|| credentials::load_token("lazydiff-github"))
}

fn env_token(name: &str) -> Option<String> {
    env::var(name).ok().filter(|token| !token.trim().is_empty())
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct GitHubUser {
    pub(crate) login: String,
    pub(crate) avatar_url: String,
    pub(crate) name: Option<String>,
    pub(crate) email: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DeviceFlowStart {
    pub(crate) user_code: String,
    pub(crate) device_code: String,
    pub(crate) verification_uri: String,
    pub(crate) interval: u64,
    pub(crate) expires_in: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct DeviceFlowTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    interval: Option<u64>,
}

const GITHUB_CLIENT_ID: &str = "Ov23lioE75FJYz4Mn7ZH";
const QUIVER_CONVEX_URL: &str = match option_env!("LAZYDIFF_CONVEX_URL") {
    Some(url) => url,
    None => "https://polished-kingfisher-268.convex.cloud",
};
const QUIVER_CONVEX_HTTP_URL: &str = match option_env!("LAZYDIFF_CONVEX_HTTP_URL") {
    Some(url) => url,
    None => "https://polished-kingfisher-268.convex.site",
};

pub(crate) fn login_github() -> std::result::Result<GitHubUser, String> {
    if let Ok(user) = connect_existing_github_login() {
        return Ok(user);
    }

    let flow = start_device_flow()?;
    open_external(&flow.verification_uri);
    println!();
    println!("Sign in to lazydiff");
    println!("Open: {}", flow.verification_uri);
    println!("Code: {}", flow.user_code);
    println!();
    println!("Waiting for GitHub to confirm sign-in…");

    let token = poll_device_flow(&flow)?;
    let user = fetch_user(&token)?;
    persist_auth(&token, &user)?;
    let user_clone = user.clone();
    thread::spawn(move || sync_user_to_convex_best_effort(&user_clone));
    Ok(user)
}

pub(crate) fn connect_existing_github_login() -> std::result::Result<GitHubUser, String> {
    let token = github_token().ok_or_else(|| "no existing GitHub token".to_string())?;
    let user = fetch_user(&token)?;
    write_json(auth_file(), &user)?;
    // Fire-and-forget: don't block auth status on Convex sync
    let user_clone = user.clone();
    thread::spawn(move || sync_user_to_convex_best_effort(&user_clone));
    Ok(user)
}

fn sync_user_to_convex_best_effort(user: &GitHubUser) {
    if let Err(error) = sync_user_to_convex(user) {
        eprintln!("[lazydiff] GitHub sign-in succeeded; Convex user sync failed: {error}");
    }
}

pub(crate) fn logout_github() -> std::result::Result<bool, String> {
    let mut removed = credentials::delete_token("lazydiff-github")?;
    match fs::remove_file(convex_user_file()) {
        Ok(()) => removed = true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "failed to remove {}: {error}",
                convex_user_file().display()
            ));
        }
    }
    Ok(removed)
}

fn start_device_flow() -> std::result::Result<DeviceFlowStart, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("lazydiff")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": GITHUB_CLIENT_ID,
            "scope": "repo read:user user:email",
        }))
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub device code request failed: {}",
            response.status()
        ));
    }
    response.json().map_err(|error| error.to_string())
}

fn poll_device_flow(flow: &DeviceFlowStart) -> std::result::Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("lazydiff")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let mut interval = flow.interval.max(1);
    let max_polls = flow.expires_in.saturating_div(interval).max(1);
    for _ in 0..max_polls {
        thread::sleep(Duration::from_secs(interval));
        let response = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "client_id": GITHUB_CLIENT_ID,
                "device_code": flow.device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            }))
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("GitHub token poll failed: {}", response.status()));
        }
        let decoded: DeviceFlowTokenResponse =
            response.json().map_err(|error| error.to_string())?;
        if let Some(token) = decoded.access_token.filter(|token| !token.is_empty()) {
            return Ok(token);
        }
        match decoded.error.as_deref() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval = decoded.interval.unwrap_or(interval + 5),
            Some("expired_token") => return Err("GitHub sign-in code expired".to_string()),
            Some("access_denied") => return Err("GitHub sign-in was denied".to_string()),
            Some(error) => return Err(format!("GitHub sign-in failed: {error}")),
            None => return Err("GitHub sign-in returned no token".to_string()),
        }
    }
    Err("GitHub sign-in timed out".to_string())
}

fn fetch_user(token: &str) -> std::result::Result<GitHubUser, String> {
    github_get_json("user", token, "GitHub user")
}

fn sync_user_to_convex(user: &GitHubUser) -> std::result::Result<String, String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let convex_user_id = runtime.block_on(async {
        let mut client = ConvexClient::new(QUIVER_CONVEX_URL)
            .await
            .map_err(|error| error.to_string())?;
        let mut args = BTreeMap::from([
            (
                "githubAvatarUrl".to_string(),
                Value::String(user.avatar_url.clone()),
            ),
            ("githubLogin".to_string(), Value::String(user.login.clone())),
        ]);
        if let Some(name) = &user.name {
            args.insert("githubName".to_string(), Value::String(name.clone()));
        }
        match client
            .mutation("users:upsert", args)
            .await
            .map_err(|error| error.to_string())?
        {
            FunctionResult::Value(Value::String(id)) => Ok(id),
            FunctionResult::Value(other) => Err(format!(
                "Convex user sync returned unexpected value: {other:?}"
            )),
            FunctionResult::ErrorMessage(error) => Err(format!("Convex user sync failed: {error}")),
            FunctionResult::ConvexError(error) => {
                Err(format!("Convex user sync failed: {}", error.message))
            }
        }
    })?;
    persist_convex_user(&convex_user_id, &user.login)?;
    Ok(convex_user_id)
}

fn github_get_json<T: for<'de> Deserialize<'de>>(
    endpoint: &str,
    token: &str,
    label: &str,
) -> std::result::Result<T, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("lazydiff")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(format!("https://api.github.com/{endpoint}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("{label} request failed: {}", response.status()));
    }
    response.json().map_err(|error| error.to_string())
}

fn github_post_json(
    endpoint: &str,
    token: &str,
    payload: serde_json::Value,
    label: &str,
) -> std::result::Result<serde_json::Value, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("lazydiff")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!("https://api.github.com/{endpoint}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .json(&payload)
        .send()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let details = response.text().unwrap_or_default();
        let details = details.trim();
        return Err(if details.is_empty() {
            format!("{label} request failed: {status}")
        } else {
            format!("{label} request failed: {status}: {details}")
        });
    }
    response.json().map_err(|error| error.to_string())
}

fn persist_auth(token: &str, user: &GitHubUser) -> std::result::Result<(), String> {
    credentials::store_token("lazydiff-github", token)?;
    write_json(auth_file(), user)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedConvexUser<'a> {
    convex_user_id: &'a str,
    github_login: &'a str,
    deployment_url: &'a str,
    http_actions_url: &'a str,
}

fn persist_convex_user(
    convex_user_id: &str,
    github_login: &str,
) -> std::result::Result<(), String> {
    write_json(
        convex_user_file(),
        &PersistedConvexUser {
            convex_user_id,
            github_login,
            deployment_url: QUIVER_CONVEX_URL,
            http_actions_url: QUIVER_CONVEX_HTTP_URL,
        },
    )
}

fn write_json<T: Serialize>(path: PathBuf, value: &T) -> std::result::Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| error.to_string())
}

fn auth_file() -> PathBuf {
    data_dir().join("github-auth.json")
}

fn convex_user_file() -> PathBuf {
    data_dir().join("convex-user.json")
}

fn data_dir() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lazydiff")
}

fn open_external(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = ProcessCommand::new(opener).arg(url).spawn();
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

pub(crate) fn post_pull_request_comment(
    repository: &str,
    number: u32,
    target: &DiffLineRangeTarget,
    body: &str,
) -> std::result::Result<GitHubComment, String> {
    let token =
        github_token().ok_or_else(|| "sign in to Quiver to post GitHub comments".to_string())?;
    let commit_id = fetch_pull_request_head_sha(repository, number, &token)?;
    let side = match target.side() {
        DiffSide::Left => "LEFT",
        DiffSide::Right => "RIGHT",
    };
    let start_line = target.start.line.min(target.end.line);
    let end_line = target.start.line.max(target.end.line);
    let mut payload = serde_json::json!({
        "body": body.trim(),
        "commit_id": commit_id,
        "path": target.path(),
        "line": end_line,
        "side": side,
    });
    if start_line != end_line {
        payload["start_line"] = serde_json::json!(start_line);
        payload["start_side"] = serde_json::json!(side);
    }

    let value: serde_json::Value = github_post_json(
        &format!("repos/{repository}/pulls/{number}/comments"),
        &token,
        payload,
        "GitHub PR comment",
    )?;
    let raw: RawRestComment = serde_json::from_value(value).map_err(|error| error.to_string())?;
    Ok(GitHubComment {
        author: raw
            .user
            .map(|user| user.login)
            .unwrap_or_else(|| "unknown".to_string()),
        body: raw.body.unwrap_or_default(),
        created_at: raw.created_at.unwrap_or_default(),
    })
}

fn fetch_pull_request_head_sha(
    repository: &str,
    number: u32,
    token: &str,
) -> std::result::Result<String, String> {
    let value: serde_json::Value = github_get_json(
        &format!("repos/{repository}/pulls/{number}"),
        token,
        "GitHub PR",
    )?;
    value
        .get("head")
        .and_then(|head| head.get("sha"))
        .and_then(|sha| sha.as_str())
        .filter(|sha| !sha.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "GitHub PR returned no head SHA".to_string())
}

fn fetch_pull_request_comment_endpoint(
    repository: &str,
    number: u32,
    kind: &str,
) -> std::result::Result<Vec<GitHubComment>, String> {
    let token =
        github_token().ok_or_else(|| "sign in to Quiver to load GitHub comments".to_string())?;
    let endpoint = match kind {
        "issues" => format!("repos/{repository}/issues/{number}/comments"),
        "pulls" => format!("repos/{repository}/pulls/{number}/comments"),
        _ => return Ok(Vec::new()),
    };
    let value: serde_json::Value = github_get_json(&endpoint, &token, "GitHub comments")?;
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
    let token =
        github_token().ok_or_else(|| "sign in to Quiver to load GitHub diffs".to_string())?;
    let value: serde_json::Value = github_get_json(
        &format!("repos/{repository}/pulls/{number}/files?per_page=100"),
        &token,
        "GitHub PR files",
    )?;
    let files = parse_pull_request_files_value(value)?;
    Ok(pull_request_files_to_patch(&files))
}

#[derive(Debug, Deserialize)]
struct GitHubPullRequestRefs {
    base: GitHubPullRequestRef,
    head: GitHubPullRequestRef,
}

#[derive(Debug, Deserialize)]
struct GitHubPullRequestRef {
    sha: String,
}

pub(crate) fn fetch_pull_request_file_sources(
    repository: &str,
    number: u32,
    paths: &[String],
) -> std::result::Result<HashMap<String, PullRequestFileSources>, String> {
    let token = github_token()
        .ok_or_else(|| "sign in to Quiver to load GitHub diff context".to_string())?;
    let refs: GitHubPullRequestRefs = github_get_json(
        &format!("repos/{repository}/pulls/{number}"),
        &token,
        "GitHub PR refs",
    )?;
    let mut sources = HashMap::new();
    for path in paths {
        let old = github_get_file_text(repository, &refs.base.sha, path, &token).ok();
        let new = github_get_file_text(repository, &refs.head.sha, path, &token).ok();
        if old.is_some() || new.is_some() {
            sources.insert(path.clone(), PullRequestFileSources { old, new });
        }
    }
    Ok(sources)
}

fn github_get_file_text(
    repository: &str,
    sha: &str,
    path: &str,
    token: &str,
) -> std::result::Result<String, String> {
    let mut url = reqwest::Url::parse("https://api.github.com/").map_err(|e| e.to_string())?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "failed to build GitHub contents URL".to_string())?;
        segments.push("repos");
        for segment in repository.split('/') {
            segments.push(segment);
        }
        segments.push("contents");
        for segment in path.split('/') {
            segments.push(segment);
        }
    }
    url.query_pairs_mut().append_pair("ref", sha);
    let client = reqwest::blocking::Client::builder()
        .user_agent("lazydiff")
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github.raw")
        .send()
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub file contents request failed: {}",
            response.status()
        ));
    }
    response.text().map_err(|error| error.to_string())
}

pub(crate) fn fetch_commit_patch(
    repository: &str,
    sha: &str,
) -> std::result::Result<String, String> {
    let token =
        github_token().ok_or_else(|| "sign in to Quiver to load GitHub commits".to_string())?;
    let value: serde_json::Value = github_get_json(
        &format!("repos/{repository}/commits/{sha}"),
        &token,
        "GitHub commit",
    )?;
    let files = parse_pull_request_files_value(value.get("files").cloned().unwrap_or_default())?;
    Ok(pull_request_files_to_patch(&files))
}

pub(crate) fn fetch_pull_request_commits(
    repository: &str,
    number: u32,
) -> std::result::Result<Vec<GitCommit>, String> {
    let token =
        github_token().ok_or_else(|| "sign in to Quiver to load GitHub commits".to_string())?;
    let value: serde_json::Value = github_get_json(
        &format!("repos/{repository}/pulls/{number}/commits?per_page=100"),
        &token,
        "GitHub PR commits",
    )?;
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
        .timeout(Duration::from_secs(15))
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

// ---------------------------------------------------------------------------
// GitHubForge — Forge trait implementation
// ---------------------------------------------------------------------------

pub struct GitHubForge;

impl Forge for GitHubForge {
    fn name(&self) -> &'static str {
        "GitHub"
    }

    fn auth_status(&self) -> GitHubAuthStatus {
        github_auth_status()
    }

    fn login(&self) -> Result<String, String> {
        let user = login_github()?;
        Ok(user.login)
    }

    fn connect_existing_login(&self) -> Result<String, String> {
        let user = connect_existing_github_login()?;
        Ok(user.login)
    }

    fn logout(&self) -> Result<bool, String> {
        logout_github()
    }

    fn fetch_queue(&self) -> Result<GitHubQueue, String> {
        let token = github_token().ok_or_else(|| "Sign in to load GitHub PRs".to_string())?;
        fetch_github_queue(&token)
    }

    fn fetch_pull_request_comments(
        &self,
        repo: &str,
        number: u32,
    ) -> Result<Vec<GitHubComment>, String> {
        fetch_pull_request_comments(repo, number)
    }

    fn fetch_pull_request_patch(&self, repo: &str, number: u32) -> Result<String, String> {
        fetch_pull_request_patch(repo, number)
    }

    fn fetch_pull_request_file_sources(
        &self,
        repo: &str,
        number: u32,
        paths: &[String],
    ) -> Result<HashMap<String, PullRequestFileSources>, String> {
        fetch_pull_request_file_sources(repo, number, paths)
    }

    fn fetch_pull_request_commits(
        &self,
        repo: &str,
        number: u32,
    ) -> Result<Vec<GitCommit>, String> {
        fetch_pull_request_commits(repo, number)
    }

    fn fetch_commit_patch(&self, repo: &str, sha: &str) -> Result<String, String> {
        fetch_commit_patch(repo, sha)
    }

    fn post_comment(
        &self,
        repo: &str,
        number: u32,
        target: &DiffLineRangeTarget,
        body: &str,
    ) -> Result<GitHubComment, String> {
        post_pull_request_comment(repo, number, target, body)
    }

    fn pull_request_url(&self, repo: &str, number: u32) -> String {
        format!("https://github.com/{repo}/pull/{number}")
    }

    fn branch_url(&self, repo: &str, branch: &str) -> String {
        format!("https://github.com/{repo}/tree/{branch}")
    }
}
