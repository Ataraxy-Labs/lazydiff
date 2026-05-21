use std::env;

use lazydiff_diffs::DiffLineRangeTarget;
use serde::Deserialize;

use super::credentials;
use crate::forge::{Forge, ForgeAuthStatus, ForgeComment, ForgeQueue};
use crate::github::models::{GitHubQueue, GitHubQueueStatus};
use crate::github::worktree::GitCommit;

pub struct GiteaForge {
    base_url: String,
}

impl GiteaForge {
    pub fn from_hostname(hostname: &str) -> Self {
        Self {
            base_url: format!("https://{hostname}"),
        }
    }

    pub fn from_env_or_default() -> Self {
        let base = env::var("GITEA_URL").unwrap_or_else(|_| "https://codeberg.org".to_string());
        Self {
            base_url: base.trim_end_matches('/').to_string(),
        }
    }

    fn api_url(&self, endpoint: &str) -> String {
        format!(
            "{}/api/v1/{}",
            self.base_url,
            endpoint.trim_start_matches('/')
        )
    }

    fn token(&self) -> Option<String> {
        env::var("GITEA_TOKEN")
            .ok()
            .or_else(|| env::var("CODEBERG_TOKEN").ok())
            .or_else(|| credentials::load_token("lazydiff-gitea"))
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
            .header("Authorization", format!("token {token}"))
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("{label} request failed: {}", response.status()));
        }
        response.json().map_err(|e| e.to_string())
    }
}

impl Forge for GiteaForge {
    fn name(&self) -> &'static str {
        "Gitea"
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
        println!("Gitea / Forgejo / Codeberg sign-in");
        println!(
            "Create a Personal Access Token at {}/user/settings/applications",
            self.base_url
        );
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

        let user: GiteaUser = self.get_json("user", &token, "Gitea user")?;
        credentials::store_token("lazydiff-gitea", &token)?;
        Ok(user.login)
    }

    fn logout(&self) -> Result<bool, String> {
        credentials::delete_token("lazydiff-gitea")
    }

    fn fetch_queue(&self) -> Result<ForgeQueue, String> {
        let token = self
            .token()
            .ok_or_else(|| "set GITEA_TOKEN to load pull requests".to_string())?;

        let user: GiteaUser = self.get_json("user", &token, "Gitea user")?;

        // Fetch repos the user has access to, then PRs from those repos
        let repos: Vec<GiteaRepo> = self.get_json(
            "repos/search?limit=30&sort=updated&order=desc",
            &token,
            "Gitea repos",
        )?;

        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for repo in &repos {
            let prs: Vec<GiteaPullRequest> = match self.get_json(
                &format!("repos/{}/pulls?state=open&limit=30", repo.full_name),
                &token,
                "Gitea PRs",
            ) {
                Ok(prs) => prs,
                Err(_) => continue,
            };
            for pr in prs {
                let key = (repo.full_name.clone(), pr.number);
                if !seen.insert(key) {
                    continue;
                }
                items.push(pr.into_pull_request(&repo.full_name));
            }
        }

        Ok(GitHubQueue {
            viewer: Some(user.login),
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
            .ok_or_else(|| "sign in to Gitea to load comments".to_string())?;
        let mut comments: Vec<ForgeComment> = Vec::new();

        // Issue comments
        let issue_comments: Vec<GiteaComment> = self.get_json(
            &format!("repos/{repo}/issues/{number}/comments?limit=100"),
            &token,
            "Gitea issue comments",
        )?;
        comments.extend(issue_comments.into_iter().map(|c| c.into_comment()));

        // Review comments
        let review_comments: Vec<GiteaComment> = self
            .get_json(
                &format!("repos/{repo}/pulls/{number}/comments?limit=100"),
                &token,
                "Gitea review comments",
            )
            .unwrap_or_default();
        comments.extend(review_comments.into_iter().map(|c| c.into_comment()));

        comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(comments)
    }

    fn fetch_pull_request_patch(
        &self,
        repo: &str,
        number: u32,
    ) -> Result<String, String> {
        let token = self
            .token()
            .ok_or_else(|| "sign in to Gitea to load diffs".to_string())?;
        let client = self.client()?;
        // Gitea serves the raw patch at .diff endpoint
        let response = client
            .get(format!(
                "{}/api/v1/repos/{repo}/pulls/{number}.diff",
                self.base_url
            ))
            .header("Authorization", format!("token {token}"))
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Gitea PR diff failed: {}", response.status()));
        }
        response.text().map_err(|e| e.to_string())
    }

    fn fetch_pull_request_commits(
        &self,
        repo: &str,
        number: u32,
    ) -> Result<Vec<GitCommit>, String> {
        let token = self
            .token()
            .ok_or_else(|| "sign in to Gitea to load commits".to_string())?;
        let commits: Vec<GiteaCommitEntry> = self.get_json(
            &format!("repos/{repo}/pulls/{number}/commits?limit=100"),
            &token,
            "Gitea PR commits",
        )?;
        Ok(commits.into_iter().map(|c| c.into_git_commit()).collect())
    }

    fn fetch_commit_patch(&self, repo: &str, sha: &str) -> Result<String, String> {
        let token = self
            .token()
            .ok_or_else(|| "sign in to Gitea to load commits".to_string())?;
        let client = self.client()?;
        let response = client
            .get(format!(
                "{}/api/v1/repos/{repo}/git/commits/{sha}.diff",
                self.base_url
            ))
            .header("Authorization", format!("token {token}"))
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Gitea commit diff failed: {}", response.status()));
        }
        response.text().map_err(|e| e.to_string())
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
            .ok_or_else(|| "sign in to Gitea to post comments".to_string())?;
        let client = self.client()?;
        let response = client
            .post(self.api_url(&format!("repos/{repo}/issues/{number}/comments")))
            .header("Authorization", format!("token {token}"))
            .json(&serde_json::json!({ "body": body.trim() }))
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("Gitea comment failed: {}", response.status()));
        }
        let comment: GiteaComment = response.json().map_err(|e| e.to_string())?;
        Ok(comment.into_comment())
    }

    fn pull_request_url(&self, repo: &str, number: u32) -> String {
        format!("{}/{repo}/pulls/{number}", self.base_url)
    }

    fn branch_url(&self, repo: &str, branch: &str) -> String {
        format!("{}/{repo}/src/branch/{branch}", self.base_url)
    }
}

// ---------------------------------------------------------------------------
// Gitea API response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GiteaUser {
    login: String,
}

#[derive(Deserialize)]
struct GiteaRepo {
    full_name: String,
}

#[derive(Deserialize)]
struct GiteaPullRequest {
    number: u32,
    title: String,
    #[serde(default)]
    body: Option<String>,
    user: GiteaUser,
    #[serde(default)]
    head: Option<GiteaBranch>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    draft: bool,
}

impl GiteaPullRequest {
    fn into_pull_request(self, repo: &str) -> crate::github::models::GitHubPullRequest {
        use crate::app::WorkItemKind;
        use crate::github::models::*;
        let review_status = if self.draft {
            ReviewStatus::Draft
        } else {
            ReviewStatus::None
        };
        GitHubPullRequest {
            kind: WorkItemKind::OwnedPrFeedback,
            repository: repo.to_string(),
            author: self.user.login,
            head_ref_name: self
                .head
                .map(|h| h.label)
                .unwrap_or_else(|| "unknown".to_string()),
            number: self.number,
            title: self.title,
            body: self.body.unwrap_or_default(),
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
struct GiteaBranch {
    label: String,
}

#[derive(Deserialize, Default)]
struct GiteaComment {
    user: Option<GiteaUser>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: String,
}

impl GiteaComment {
    fn into_comment(self) -> ForgeComment {
        ForgeComment {
            author: self
                .user
                .map(|u| u.login)
                .unwrap_or_else(|| "unknown".to_string()),
            body: self.body,
            created_at: self.created_at,
        }
    }
}

#[derive(Deserialize)]
struct GiteaCommitEntry {
    sha: String,
    commit: GiteaCommitInner,
}

#[derive(Deserialize)]
struct GiteaCommitInner {
    message: String,
    author: Option<GiteaCommitAuthor>,
}

#[derive(Deserialize)]
struct GiteaCommitAuthor {
    name: Option<String>,
    date: Option<String>,
}

impl GiteaCommitEntry {
    fn into_git_commit(self) -> GitCommit {
        let short_sha = self.sha.chars().take(7).collect::<String>();
        let subject = self
            .commit
            .message
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        let author = self
            .commit
            .author
            .as_ref()
            .and_then(|a| a.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let authored_at = self
            .commit
            .author
            .and_then(|a| a.date)
            .unwrap_or_default();
        GitCommit {
            sha: self.sha,
            short_sha,
            subject,
            author,
            authored_at,
            files: Vec::new(),
        }
    }
}

