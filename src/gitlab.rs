//! GitLab API and glab CLI integration for fetching repositories.

use serde::Deserialize;
use std::process::Stdio;
use tokio::process::Command;

/// Information about a GitLab repository.
#[derive(Debug, Clone)]
pub struct RepoInfo {
    pub name: String,
    /// Full namespace path as in the project URL, e.g. `group/subgroup/project`.
    pub path_with_namespace: String,
    /// Directory relative to the target directory where the repo is cloned.
    pub local_path: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub web_url: String,
    pub is_private: bool,
}

/// Response from the GitLab API for projects.
#[derive(Debug, Deserialize)]
struct GitLabProject {
    name: String,
    path_with_namespace: String,
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
        .args(["api", "--paginate", &api_path])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let projects = parse_paginated_projects(&stdout)?;

    Some(projects.into_iter().map(RepoInfo::from).collect())
}

/// Parse the output of `glab api --paginate`.
///
/// Every page is printed as a separate JSON array (`[...][...]`), so a plain
/// `serde_json::from_str` fails on anything beyond the first page.
fn parse_paginated_projects(stdout: &str) -> Option<Vec<GitLabProject>> {
    let mut projects = Vec::new();
    for page in serde_json::Deserializer::from_str(stdout).into_iter::<Vec<GitLabProject>>() {
        projects.extend(page.ok()?);
    }
    Some(projects)
}

impl From<GitLabProject> for RepoInfo {
    fn from(p: GitLabProject) -> Self {
        Self {
            local_path: p.name.clone(),
            name: p.name,
            path_with_namespace: p.path_with_namespace,
            clone_url: p.http_url_to_repo,
            ssh_url: p.ssh_url_to_repo,
            web_url: p.web_url,
            is_private: p.visibility == "private",
        }
    }
}

impl RepoInfo {
    /// Clone into `path_with_namespace` instead of the flat `name` directory,
    /// like `glab repo clone --preserve-namespace`.
    #[must_use]
    pub fn with_preserved_namespace(mut self) -> Self {
        self.local_path.clone_from(&self.path_with_namespace);
        self
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

    fn project_json(name: &str, visibility: &str) -> String {
        format!(
            r#"{{"name":"{name}","path_with_namespace":"g/sub/{name}","http_url_to_repo":"https://gitlab.com/g/{name}.git","ssh_url_to_repo":"git@gitlab.com:g/{name}.git","web_url":"https://gitlab.com/g/{name}","visibility":"{visibility}"}}"#
        )
    }

    #[test]
    fn test_parse_single_page() {
        let input = format!(
            "[{},{}]",
            project_json("a", "public"),
            project_json("b", "private")
        );
        let projects = parse_paginated_projects(&input).unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "a");
        assert_eq!(projects[1].visibility, "private");
    }

    #[test]
    fn test_parse_concatenated_pages() {
        let input = format!(
            "[{}][{}][{}]",
            project_json("a", "public"),
            project_json("b", "public"),
            project_json("c", "internal")
        );
        let names: Vec<_> = parse_paginated_projects(&input)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn test_parse_pages_separated_by_whitespace() {
        let input = format!(
            "[{}]\n[{}]\n",
            project_json("a", "public"),
            project_json("b", "public")
        );
        assert_eq!(parse_paginated_projects(&input).unwrap().len(), 2);
    }

    #[test]
    fn test_parse_trailing_empty_page() {
        let input = format!("[{}][]", project_json("a", "public"));
        assert_eq!(parse_paginated_projects(&input).unwrap().len(), 1);
    }

    #[test]
    fn test_parse_empty_output() {
        assert!(parse_paginated_projects("").unwrap().is_empty());
        assert!(parse_paginated_projects("[]").unwrap().is_empty());
    }

    #[test]
    fn test_parse_invalid_page_fails() {
        let input = format!("[{}][{{\"broken\":", project_json("a", "public"));
        assert!(parse_paginated_projects(&input).is_none());
        assert!(parse_paginated_projects("not json").is_none());
    }

    #[test]
    fn test_parse_maps_to_repo_info() {
        let input = format!("[{}]", project_json("a", "private"));
        let repo = RepoInfo::from(parse_paginated_projects(&input).unwrap().remove(0));
        assert_eq!(repo.name, "a");
        assert_eq!(repo.path_with_namespace, "g/sub/a");
        assert_eq!(repo.local_path, "a");
        assert_eq!(repo.clone_url, "https://gitlab.com/g/a.git");
        assert_eq!(repo.ssh_url, "git@gitlab.com:g/a.git");
        assert_eq!(repo.web_url, "https://gitlab.com/g/a");
        assert!(repo.is_private);
    }

    #[test]
    fn test_with_preserved_namespace() {
        let input = format!("[{}]", project_json("a", "public"));
        let repo = RepoInfo::from(parse_paginated_projects(&input).unwrap().remove(0))
            .with_preserved_namespace();
        assert_eq!(repo.local_path, "g/sub/a");
        assert_eq!(repo.name, "a");
    }

    #[test]
    fn test_repo_info_clone() {
        let repo = RepoInfo {
            name: "test".to_string(),
            path_with_namespace: "g/test".to_string(),
            local_path: "test".to_string(),
            clone_url: "https://gitlab.com/test.git".to_string(),
            ssh_url: "git@gitlab.com:test.git".to_string(),
            web_url: "https://gitlab.com/test".to_string(),
            is_private: false,
        };
        let cloned = repo;
        assert_eq!(cloned.name, "test");
    }
}
