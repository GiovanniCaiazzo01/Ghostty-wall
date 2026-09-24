//! GitHub API boundary for commit-pinned Source resolution.

use std::time::Duration;

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;

const MAX_JSON_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

/// Credential-free failure classification returned by GitHub adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GithubApiError {
    /// Supplied credentials are absent, invalid, or insufficient.
    Authentication,
    /// GitHub refused the request because its rate limit was exhausted.
    RateLimited,
    /// Transport, status, or response data prevented a complete result.
    Unavailable,
}

/// Kind of object referenced by one Git tree entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GithubEntryKind {
    /// Git blob.
    Blob,
    /// Git tree.
    Tree,
    /// Git commit, including submodule entries.
    Commit,
    /// Unknown object kind.
    Other,
}

/// One entry returned by GitHub's Git Trees API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubTreeEntry {
    path: String,
    mode: String,
    kind: GithubEntryKind,
}

impl GithubTreeEntry {
    /// Constructs an adapter observation.
    pub fn new(path: impl Into<String>, mode: impl Into<String>, kind: GithubEntryKind) -> Self {
        Self {
            path: path.into(),
            mode: mode.into(),
            kind,
        }
    }

    /// Repository-relative UTF-8 path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Git tree mode text.
    pub fn mode(&self) -> &str {
        &self.mode
    }

    /// Referenced Git object kind.
    pub const fn kind(&self) -> GithubEntryKind {
        self.kind
    }
}

/// Complete or explicitly truncated Git tree observation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubTree {
    entries: Vec<GithubTreeEntry>,
    truncated: bool,
}

impl GithubTree {
    /// Constructs a Git tree observation.
    pub fn new(entries: Vec<GithubTreeEntry>, truncated: bool) -> Self {
        Self { entries, truncated }
    }

    /// Entries returned by GitHub.
    pub fn entries(&self) -> &[GithubTreeEntry] {
        &self.entries
    }

    /// Whether GitHub omitted entries from a recursive response.
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Narrow network seam used by deterministic Source-resolution tests.
pub trait GithubApi {
    /// Returns repository default branch name.
    fn default_branch(&self, repository: &str) -> Result<String, GithubApiError>;

    /// Resolves one requested ref to a full commit SHA.
    fn resolve_commit(&self, repository: &str, reference: &str) -> Result<String, GithubApiError>;

    /// Returns Git tree entries at an immutable commit.
    fn tree(
        &self,
        repository: &str,
        commit: &str,
        recursive: bool,
    ) -> Result<GithubTree, GithubApiError>;

    /// Downloads one repository-relative path at an immutable commit.
    fn blob(&self, repository: &str, commit: &str, path: &str) -> Result<Vec<u8>, GithubApiError>;
}

/// HTTPS GitHub REST API adapter.
pub struct GithubHttpClient {
    agent: ureq::Agent,
    token: Option<String>,
}

impl GithubHttpClient {
    /// Builds a bounded HTTPS client. Token remains only in memory and error values never include it.
    pub fn new(token: Option<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self {
            agent: config.into(),
            token,
        }
    }

    /// Builds a client from `GITHUB_TOKEN`, if set.
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("GITHUB_TOKEN")
                .ok()
                .filter(|token| !token.is_empty()),
        )
    }

    fn get(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
        accept: &str,
        limit: u64,
    ) -> Result<Vec<u8>, GithubApiError> {
        let path = segments
            .iter()
            .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
            .collect::<Vec<_>>()
            .join("/");
        let query = query
            .iter()
            .map(|(key, value)| {
                format!(
                    "{}={}",
                    utf8_percent_encode(key, NON_ALPHANUMERIC),
                    utf8_percent_encode(value, NON_ALPHANUMERIC)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let url = if query.is_empty() {
            format!("https://api.github.com/{path}")
        } else {
            format!("https://api.github.com/{path}?{query}")
        };

        let mut request = self
            .agent
            .get(&url)
            .header("Accept", accept)
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "ghostty-wall");
        let authorization = self.token.as_ref().map(|token| format!("Bearer {token}"));
        if let Some(value) = authorization.as_deref() {
            request = request.header("Authorization", value);
        }
        let mut response = request.call().map_err(|_| GithubApiError::Unavailable)?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(match status {
                401 => GithubApiError::Authentication,
                403 if response
                    .headers()
                    .get("x-ratelimit-remaining")
                    .is_some_and(|value| value == "0") =>
                {
                    GithubApiError::RateLimited
                }
                403 => GithubApiError::Authentication,
                429 => GithubApiError::RateLimited,
                _ => GithubApiError::Unavailable,
            });
        }
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|_| GithubApiError::Unavailable)
    }

    fn repository_segments<'a>(repository: &'a str, tail: &[&'a str]) -> Vec<&'a str> {
        let (owner, name) = repository
            .split_once('/')
            .expect("validated repository contains one separator");
        let mut segments = vec!["repos", owner, name];
        segments.extend_from_slice(tail);
        segments
    }
}

#[derive(Deserialize)]
struct RepositoryResponse {
    default_branch: String,
}

#[derive(Deserialize)]
struct CommitResponse {
    sha: String,
}

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntryResponse>,
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeEntryResponse {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
}

impl GithubApi for GithubHttpClient {
    fn default_branch(&self, repository: &str) -> Result<String, GithubApiError> {
        let body = self.get(
            &Self::repository_segments(repository, &[]),
            &[],
            "application/vnd.github+json",
            MAX_JSON_BYTES,
        )?;
        let response: RepositoryResponse =
            serde_json::from_slice(&body).map_err(|_| GithubApiError::Unavailable)?;
        if response.default_branch.is_empty() {
            return Err(GithubApiError::Unavailable);
        }
        Ok(response.default_branch)
    }

    fn resolve_commit(&self, repository: &str, reference: &str) -> Result<String, GithubApiError> {
        let body = self.get(
            &Self::repository_segments(repository, &["commits", reference]),
            &[],
            "application/vnd.github+json",
            MAX_JSON_BYTES,
        )?;
        let response: CommitResponse =
            serde_json::from_slice(&body).map_err(|_| GithubApiError::Unavailable)?;
        if !valid_commit(&response.sha) {
            return Err(GithubApiError::Unavailable);
        }
        Ok(response.sha)
    }

    fn tree(
        &self,
        repository: &str,
        commit: &str,
        recursive: bool,
    ) -> Result<GithubTree, GithubApiError> {
        let recursive_value = if recursive { "1" } else { "0" };
        let body = self.get(
            &Self::repository_segments(repository, &["git", "trees", commit]),
            &[("recursive", recursive_value)],
            "application/vnd.github+json",
            MAX_JSON_BYTES,
        )?;
        let response: TreeResponse =
            serde_json::from_slice(&body).map_err(|_| GithubApiError::Unavailable)?;
        let entries = response
            .tree
            .into_iter()
            .map(|entry| {
                let kind = match entry.kind.as_str() {
                    "blob" => GithubEntryKind::Blob,
                    "tree" => GithubEntryKind::Tree,
                    "commit" => GithubEntryKind::Commit,
                    _ => GithubEntryKind::Other,
                };
                GithubTreeEntry::new(entry.path, entry.mode, kind)
            })
            .collect();
        Ok(GithubTree::new(entries, response.truncated))
    }

    fn blob(&self, repository: &str, commit: &str, path: &str) -> Result<Vec<u8>, GithubApiError> {
        let mut tail = vec!["contents"];
        tail.extend(path.split('/'));
        self.get(
            &Self::repository_segments(repository, &tail),
            &[("ref", commit)],
            "application/vnd.github.raw+json",
            MAX_ASSET_BYTES + 1,
        )
    }
}

fn valid_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
