use std::process::Command;
use std::sync::Arc;

use super::Forge;
use crate::forge::gitea::GiteaForge;
use crate::forge::gitlab::GitLabForge;
use crate::github::service::GitHubForge;

/// Detect which forge to use based on the `LAZYDIFF_FORGE` env var or
/// the git remote `origin` URL.
pub(crate) fn detect_forge() -> Arc<dyn Forge> {
    if let Ok(override_val) = std::env::var("LAZYDIFF_FORGE") {
        return match override_val.to_ascii_lowercase().as_str() {
            "github" => Arc::new(GitHubForge),
            "gitlab" => Arc::new(GitLabForge::from_env_or_default()),
            "gitea" | "forgejo" | "codeberg" => Arc::new(GiteaForge::from_env_or_default()),
            _ => {
                eprintln!(
                    "[lazydiff] unknown LAZYDIFF_FORGE={override_val:?}, falling back to GitHub"
                );
                Arc::new(GitHubForge)
            }
        };
    }

    let hostname = remote_origin_hostname().unwrap_or_default();
    match classify_hostname(&hostname) {
        ForgeKind::GitHub => Arc::new(GitHubForge),
        ForgeKind::GitLab => Arc::new(GitLabForge::from_hostname(&hostname)),
        ForgeKind::Gitea => Arc::new(GiteaForge::from_hostname(&hostname)),
    }
}

enum ForgeKind {
    GitHub,
    GitLab,
    Gitea,
}

fn classify_hostname(hostname: &str) -> ForgeKind {
    let host = hostname.to_ascii_lowercase();
    if host.is_empty() || host.contains("github.com") {
        return ForgeKind::GitHub;
    }
    if host.contains("gitlab.com") || host.contains("gitlab") {
        return ForgeKind::GitLab;
    }
    if host.contains("codeberg.org") || host.contains("gitea.com") || host.contains("gitea") {
        return ForgeKind::Gitea;
    }
    // Unknown host — default to GitHub (most common).
    ForgeKind::GitHub
}

/// Run `git remote get-url origin` and extract the hostname.
fn remote_origin_hostname() -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_hostname_from_url(&url)
}

/// Extract the hostname from a git remote URL.
///
/// Handles SSH (`git@host:owner/repo.git`), HTTPS (`https://host/owner/repo`),
/// and `ssh://git@host/…` forms.
fn parse_hostname_from_url(url: &str) -> Option<String> {
    // ssh://git@host/…
    if let Some(rest) = url.strip_prefix("ssh://") {
        let after_at = rest.find('@').map(|i| &rest[i + 1..]).unwrap_or(rest);
        let host = after_at.split(['/', ':']).next()?;
        return Some(host.to_string());
    }
    // https://host/… or http://host/…
    if url.starts_with("https://") || url.starts_with("http://") {
        let after_scheme = url.split("://").nth(1)?;
        let host = after_scheme.split('/').next()?;
        // Strip optional user@
        let host = host.rsplit('@').next().unwrap_or(host);
        // Strip optional :port
        let host = host.split(':').next().unwrap_or(host);
        return Some(host.to_string());
    }
    // git@host:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@") {
        let host = rest.split(':').next()?;
        return Some(host.to_string());
    }
    // user@host:owner/repo.git
    if let Some(at_pos) = url.find('@') {
        let after_at = &url[at_pos + 1..];
        let host = after_at.split(':').next()?;
        return Some(host.to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_https_url() {
        assert_eq!(
            parse_hostname_from_url("https://github.com/owner/repo.git"),
            Some("github.com".into())
        );
    }

    #[test]
    fn parse_ssh_url() {
        assert_eq!(
            parse_hostname_from_url("git@github.com:owner/repo.git"),
            Some("github.com".into())
        );
    }

    #[test]
    fn parse_ssh_scheme_url() {
        assert_eq!(
            parse_hostname_from_url("ssh://git@gitlab.example.com/owner/repo.git"),
            Some("gitlab.example.com".into())
        );
    }

    #[test]
    fn parse_gitlab_https() {
        assert_eq!(
            parse_hostname_from_url("https://gitlab.com/group/project.git"),
            Some("gitlab.com".into())
        );
    }

    #[test]
    fn parse_codeberg_https() {
        assert_eq!(
            parse_hostname_from_url("https://codeberg.org/user/repo"),
            Some("codeberg.org".into())
        );
    }

    #[test]
    fn classify_known_hosts() {
        assert!(matches!(classify_hostname("github.com"), ForgeKind::GitHub));
        assert!(matches!(classify_hostname("gitlab.com"), ForgeKind::GitLab));
        assert!(matches!(
            classify_hostname("gitlab.example.com"),
            ForgeKind::GitLab
        ));
        assert!(matches!(
            classify_hostname("codeberg.org"),
            ForgeKind::Gitea
        ));
        assert!(matches!(
            classify_hostname("gitea.example.com"),
            ForgeKind::Gitea
        ));
        assert!(matches!(classify_hostname(""), ForgeKind::GitHub));
        assert!(matches!(
            classify_hostname("unknown.example.com"),
            ForgeKind::GitHub
        ));
    }
}
