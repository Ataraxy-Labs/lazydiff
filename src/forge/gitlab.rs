use std::env;

use lazydiff_diffs::DiffLineRangeTarget;
use serde::Deserialize;

use super::credentials;
use crate::forge::{Forge, ForgeAuthStatus, ForgeComment, ForgeQueue};
use crate::github::models::{GitHubQueue, GitHubQueueStatus};
use crate::github::worktree::GitCommit;

pub struct GitLabForge {
    base_url: String,
}

impl GitLabForge {
    pub fn from_hostname(hostname: &str) -> Self {
        Self {
            base_url: format!("https://{hostname}"),
        }
    }

    pub fn from_env_or_default() -> Self {
        let base = env::var("GITLAB_URL").unwrap_or_else(|_| "https://gitlab.com".to_string());
        Self {
            base_url: base.trim_end_matches('/').to_string(),
        }
    }

    fn api_url(&self, endpoint: &str) -> String {
        format!("{}/api/v4/{}", self.base_url, endpoint.trim_start_matches('/'))
    }

    fn token(&self) -> Option<String> {
        env::var("GITLAB_TOKEN")
            .ok()
            .or_else(|| credentials::load_token("lazydiff-gitlab"))
    }

    fn client(&self) -> Result<reqwest::blocking::Client, String> {
        reqwest::blocking::Client::builder()
            .user_agent("lazydiff")
            .build()
            .map_err(|e| e.to_string())
    }

    fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        token: &str,
        label: &str,
    ) -> Result<T, String> {
        let client = self.client()?;
        let response = client
            .get(self.api_url(endpoint))
            .header("PRIVATE-TOKEN", token)
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("{label} request failed: {}", response.status()));
        }
        response.json().map_err(|e| e.to_string())
    }

    /// URL-encode a project path (e.g. `group/project` → `group%2Fproject`).
    fn encode_project(repo: &str) -> String {
        repo.replace('/', "%2F")
    }
}

impl Forge for GitLabForge {
    fn name(&self) -> &'static str {
        "GitLab"
    }

    fn auth_status(&self) -> ForgeAuthStatus {
        if self.token().is_some() {
            ForgeAuthStatus::Authenticated
        } else {
            ForgeAuthStatus::MissingLogin
        }
    }

    fn login(&self) -> Result<String, String> {
        println!();
        println!("GitLab sign-in");
        println!(
            "Create a Personal Access Token at {}//-/user_settings/personal_access_tokens",
            self.base_url
        );
        println!("Required scopes: read_api, api");
        println!();
        print!("Paste your token: ");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let mut token = String::new();
        std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut token)
            .map_err(|e| e.to_string())?;
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err("no token provided".to_string());
        }

        // Verify the token works
        let user: GitLabUser = self.get_json("user", &token, "GitLab user")?;
        credentials::store_token("lazydiff-gitlab", &token)?;
        Ok(user.username)
    }

    fn logout(&self) -> Result<bool, String> {
        credentials::delete_token("lazydiff-gitlab")
    }

    fn fetch_queue(&self) -> Result<ForgeQueue, String> {
        let token = self
            .token()
            .ok_or_else(|| "set GITLAB_TOKEN to load merge requests".to_string())?;

        let assigned: Vec<GitLabMergeRequest> =
            self.get_json("merge_requests?scope=assigned_to_me&state=opened&per_page=30", &token, "GitLab assigned MRs")?;
        let authored: Vec<GitLabMergeRequest> =
            self.get_json("merge_requests?scope=created_by_me&state=opened&per_page=30", &token, "GitLab authored MRs")?;

        let user: GitLabUser = self.get_json("user", &token, "GitLab user")?;

        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for mr in assigned.into_iter().chain(authored) {
            let key = (mr.project_id, mr.iid);
            if !seen.insert(key) {
                continue;
            }
            items.push(mr.into_pull_request());
        }

        Ok(GitHubQueue {
            viewer: Some(user.username),
            status: GitHubQueueStatus::Ready,
            items,
            cached_at: None,
        })
    }

    fn fetch_pull_request_comments(
        &self,
        repo: &str,
        number: u32,
    ) -> Result<Vec<ForgeComment>, String> {
        let token = self
            .token()
            .ok_or_else(|| "sign in to GitLab to load comments".to_string())?;
        let project = Self::encode_project(repo);
        let notes: Vec<GitLabNote> = self.get_json(
            &format!("projects/{project}/merge_requests/{number}/notes?per_page=100"),
            &token,
            "GitLab MR notes",
        )?;
        Ok(notes.into_iter().filter(|n| !n.system).map(|n| n.into_comment()).collect())
    }

    fn fetch_pull_request_patch(
        &self,
        repo: &str,
        number: u32,
    ) -> Result<String, String> {
        let token = self
            .token()
            .ok_or_else(|| "sign in to GitLab to load diffs".to_string())?;
        let project = Self::encode_project(repo);
        let changes: GitLabMrChanges = self.get_json(
            &format!("projects/{project}/merge_requests/{number}/changes?per_page=100"),
            &token,
            "GitLab MR changes",
        )?;
        Ok(changes.to_unified_patch())
    }

    fn fetch_pull_request_commits(
        &self,
        repo: &str,
        number: u32,
    ) -> Result<Vec<GitCommit>, String> {
        let token = self
            .token()
            .ok_or_else(|| "sign in to GitLab to load commits".to_string())?;
        let project = Self::encode_project(repo);
        let commits: Vec<GitLabCommit> = self.get_json(
            &format!("projects/{project}/merge_requests/{number}/commits?per_page=100"),
            &token,
            "GitLab MR commits",
        )?;
        Ok(commits.into_iter().map(|c| c.into_git_commit()).collect())
    }

    fn fetch_commit_patch(&self, repo: &str, sha: &str) -> Result<String, String> {
        let token = self
            .token()
            .ok_or_else(|| "sign in to GitLab to load commits".to_string())?;
        let project = Self::encode_project(repo);
        let diff: Vec<GitLabDiffFile> = self.get_json(
            &format!("projects/{project}/repository/commits/{sha}/diff?per_page=100"),
            &token,
            "GitLab commit diff",
        )?;
        Ok(diff_files_to_patch(&diff))
    }

    fn post_comment(
        &self,
        repo: &str,
        number: u32,
        _target: &DiffLineRangeTarget,
        body: &str,
    ) -> Result<ForgeComment, String> {
        let token = self
            .token()
            .ok_or_else(|| "sign in to GitLab to post comments".to_string())?;
        let project = Self::encode_project(repo);
        let client = self.client()?;
        let response = client
            .post(self.api_url(&format!(
                "projects/{project}/merge_requests/{number}/notes"
            )))
            .header("PRIVATE-TOKEN", &token)
            .json(&serde_json::json!({ "body": body.trim() }))
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "GitLab comment failed: {}",
                response.status()
            ));
        }
        let note: GitLabNote = response.json().map_err(|e| e.to_string())?;
        Ok(note.into_comment())
    }

    fn pull_request_url(&self, repo: &str, number: u32) -> String {
        format!("{}/{repo}/-/merge_requests/{number}", self.base_url)
    }

    fn branch_url(&self, repo: &str, branch: &str) -> String {
        format!("{}/{repo}/-/tree/{branch}", self.base_url)
    }
}

// ---------------------------------------------------------------------------
// GitLab API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GitLabUser {
    username: String,
}

#[derive(Deserialize)]
struct GitLabMergeRequest {
    iid: u32,
    project_id: u64,
    title: String,
    #[serde(default)]
    description: Option<String>,
    author: GitLabAuthor,
    source_branch: String,
    #[serde(default)]
    web_url: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    draft: bool,
}

impl GitLabMergeRequest {
    fn into_pull_request(self) -> crate::github::models::GitHubPullRequest {
        use crate::app::WorkItemKind;
        use crate::github::models::*;
        // Extract "group/project" from the web_url or fall back to project_id
        let repository = extract_repo_from_web_url(&self.web_url)
            .unwrap_or_else(|| format!("{}", self.project_id));
        let review_status = if self.draft {
            ReviewStatus::Draft
        } else {
            ReviewStatus::None
        };
        GitHubPullRequest {
            kind: WorkItemKind::OwnedPrFeedback,
            repository,
            author: self.author.username,
            head_ref_name: self.source_branch,
            number: self.iid,
            title: self.title,
            body: self.description.unwrap_or_default(),
            additions: 0,
            deletions: 0,
            changed_files: 0,
            review_status,
            check_status: CheckRollupStatus::None,
            check_summary: None,
            checks: Vec::new(),
            comments: Vec::new(),
            created_at: self.created_at,
        }
    }
}

#[derive(Deserialize)]
struct GitLabAuthor {
    username: String,
}

#[derive(Deserialize)]
struct GitLabNote {
    author: GitLabAuthor,
    body: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    system: bool,
}

impl GitLabNote {
    fn into_comment(self) -> ForgeComment {
        ForgeComment {
            author: self.author.username,
            body: self.body,
            created_at: self.created_at,
        }
    }
}

#[derive(Deserialize)]
struct GitLabMrChanges {
    #[serde(default)]
    changes: Vec<GitLabDiffFile>,
}

impl GitLabMrChanges {
    fn to_unified_patch(&self) -> String {
        diff_files_to_patch(&self.changes)
    }
}

#[derive(Deserialize)]
struct GitLabDiffFile {
    #[serde(default)]
    old_path: String,
    #[serde(default)]
    new_path: String,
    #[serde(default)]
    diff: String,
    #[serde(default)]
    new_file: bool,
    #[serde(default)]
    renamed_file: bool,
    #[serde(default)]
    deleted_file: bool,
}

#[derive(Deserialize)]
struct GitLabCommit {
    id: String,
    #[serde(default)]
    short_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    author_name: String,
    #[serde(default)]
    authored_date: String,
}

impl GitLabCommit {
    fn into_git_commit(self) -> GitCommit {
        GitCommit {
            sha: self.id,
            short_sha: self.short_id,
            subject: self.title,
            author: self.author_name,
            authored_at: self.authored_date,
            files: Vec::new(),
        }
    }
}

/// Build a unified-diff-style patch from GitLab diff files.
fn diff_files_to_patch(files: &[GitLabDiffFile]) -> String {
    let mut patch = String::new();
    for file in files {
        let old = if file.new_file {
            "/dev/null".to_string()
        } else {
            format!("a/{}", file.old_path)
        };
        let new = if file.deleted_file {
            "/dev/null".to_string()
        } else {
            format!("b/{}", file.new_path)
        };
        patch.push_str(&format!("diff --git a/{} b/{}\n", file.old_path, file.new_path));
        if file.new_file {
            patch.push_str("new file mode 100644\n");
        }
        if file.deleted_file {
            patch.push_str("deleted file mode 100644\n");
        }
        if file.renamed_file {
            patch.push_str(&format!("rename from {}\nrename to {}\n", file.old_path, file.new_path));
        }
        patch.push_str(&format!("--- {old}\n+++ {new}\n"));
        patch.push_str(&file.diff);
        if !file.diff.ends_with('\n') {
            patch.push('\n');
        }
    }
    patch
}

/// Try to extract "group/project" from a GitLab web_url like
/// `https://gitlab.com/group/project/-/merge_requests/123`.
fn extract_repo_from_web_url(url: &str) -> Option<String> {
    // Strip scheme
    let after_scheme = url.split("://").nth(1)?;
    // Strip host
    let after_host = after_scheme.find('/').map(|i| &after_scheme[i + 1..])?;
    // Everything before `/-/` is the project path
    let project = after_host.split("/-/").next()?;
    if project.is_empty() {
        return None;
    }
    Some(project.to_string())
}

