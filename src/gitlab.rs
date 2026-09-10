//! GitLab API and glab CLI integration for fetching repositories.

use serde::Deserialize;
use std::process::Stdio;
use tokio::process::Command;

/// Information about a GitLab repository.
#[derive(Debug, Clone)]
pub struct RepoInfo {
    pub name: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub web_url: String,
    pub is_private: bool,
}

/// Response from the GitLab API for projects.
#[derive(Debug, Deserialize)]
struct GitLabProject {
    name: String,
    http_url_to_repo: String,
    ssh_url_to_repo: String,
    web_url: String,
    visibility: String,
}

/// Check if glab CLI is installed and accessible.
pub async fn is_glab_installed() -> bool {
    Command::new("glab")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|s| s.success())
}

/// Get GitLab token from glab CLI auth.
pub async fn get_glab_token() -> Option<String> {
    let output = Command::new("glab")
        .args(["auth", "status", "-t"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;

    // glab outputs the token to stderr
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Look for "Token: <token>" pattern
    for line in stderr.lines() {
        let line = line.trim();
        if let Some(token) = line.strip_prefix("Token: ") {
            let token = token.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    None
}

/// Fetch repos using glab CLI (includes private repos if authenticated).
pub async fn get_repos_from_glab_cli(
    group: Option<&str>,
    user: Option<&str>,
) -> Option<Vec<RepoInfo>> {
    if !is_glab_installed().await {
        return None;
    }

    // glab doesn't have a direct "list repos for user" command like gh.
    // We use `glab api` to query the GitLab API directly.
    let api_path = if let Some(group) = group {
        format!(
            "groups/{}/projects?per_page=100&include_subgroups=true",
            urlencoding(group)
        )
    } else if let Some(user) = user {
        format!("users/{}/projects?per_page=100", urlencoding(user))
    } else {
        return None;
    };

    let output = Command::new("glab")
        .args(["api", &api_path])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let projects: Vec<GitLabProject> = serde_json::from_str(&stdout).ok()?;

    Some(projects.into_iter().map(RepoInfo::from).collect())
}

impl From<GitLabProject> for RepoInfo {
    fn from(p: GitLabProject) -> Self {
        Self {
            name: p.name,
            clone_url: p.http_url_to_repo,
            ssh_url: p.ssh_url_to_repo,
            web_url: p.web_url,
            is_private: p.visibility == "private",
        }
    }
}

/// Fetch repos using the GitLab REST API directly.
pub async fn get_repos_from_api(
    gitlab_url: &str,
    group: Option<&str>,
    user: Option<&str>,
    token: Option<&str>,
) -> Result<Vec<RepoInfo>, String> {
    let client = reqwest::Client::new();

    let api_url = if let Some(group) = group {
        format!(
            "{}/api/v4/groups/{}/projects?per_page=100&include_subgroups=true",
            gitlab_url,
            urlencoding(group)
        )
    } else if let Some(user) = user {
        format!(
            "{}/api/v4/users/{}/projects?per_page=100",
            gitlab_url,
            urlencoding(user)
        )
    } else {
        return Err("Must specify either group or user".to_string());
    };

    let mut all_projects = Vec::new();
    let mut page = 1u32;

    loop {
        let url = format!("{api_url}&page={page}");
        let mut request = client.get(&url);
        if let Some(token) = token {
            request = request.header("PRIVATE-TOKEN", token);
        }

        let response = request.send().await.map_err(|e| {
            if e.is_connect() {
                format!("Unable to connect to {gitlab_url}. Please check your internet connection")
            } else {
                format!("Failed to fetch repositories: {e}")
            }
        })?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            let target = group.or(user).unwrap_or("");
            return Err(format!(
                "'{target}' not found or not accessible at {gitlab_url}"
            ));
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err("Authentication failed. Please provide a valid GitLab token".to_string());
        }
        if !status.is_success() {
            return Err(format!("API request failed with status {status}: {url}"));
        }

        let projects: Vec<GitLabProject> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse API response: {e}"))?;

        if projects.is_empty() {
            break;
        }

        all_projects.extend(projects.into_iter().map(RepoInfo::from));

        page += 1;
        // Safety limit to prevent infinite loops
        if page > 100 {
            break;
        }
    }

    Ok(all_projects)
}

/// Simple URL encoding for path segments (encode slashes in group names).
fn urlencoding(s: &str) -> String {
    s.replace('/', "%2F")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencoding_simple() {
        assert_eq!(urlencoding("mygroup"), "mygroup");
    }

    #[test]
    fn test_urlencoding_with_slashes() {
        assert_eq!(urlencoding("parent/child"), "parent%2Fchild");
    }

    #[test]
    fn test_repo_info_clone() {
        let repo = RepoInfo {
            name: "test".to_string(),
            clone_url: "https://gitlab.com/test.git".to_string(),
            ssh_url: "git@gitlab.com:test.git".to_string(),
            web_url: "https://gitlab.com/test".to_string(),
            is_private: false,
        };
        let cloned = repo;
        assert_eq!(cloned.name, "test");
    }
}
