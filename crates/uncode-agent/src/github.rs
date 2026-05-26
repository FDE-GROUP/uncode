use serde::{Deserialize, Serialize};
use uncode_core::error::UncodeError;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubIssue {
    number: u64,
    title: String,
    body: Option<String>,
    state: String,
    html_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GitHubPullRequest {
    html_url: String,
    number: u64,
}

pub struct GitHubClient {
    token: String,
    client: reqwest::Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueInfo {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub url: Option<String>,
}

impl GitHubClient {
    pub fn new(token: String) -> Self {
        Self {
            token,
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_issue(
        &self,
        owner: &str,
        repo: &str,
        issue_number: u64,
    ) -> Result<IssueInfo, UncodeError> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/issues/{issue_number}");

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "uncode")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(UncodeError::Other(format!(
                "GitHub API error: {}",
                response.status()
            )));
        }

        let issue: GitHubIssue = response
            .json()
            .await
            .map_err(|e| UncodeError::Other(e.to_string()))?;

        Ok(IssueInfo {
            number: issue.number,
            title: issue.title,
            body: issue.body.unwrap_or_default(),
            url: issue.html_url,
        })
    }

    pub async fn create_pr(
        &self,
        owner: &str,
        repo: &str,
        title: &str,
        head: &str,
        base: &str,
        body: &str,
    ) -> Result<String, UncodeError> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls");

        let payload = serde_json::json!({
            "title": title,
            "head": head,
            "base": base,
            "body": body,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "uncode")
            .header("Accept", "application/vnd.github+json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| UncodeError::Network(e.to_string()))?;

        if !response.status().is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(UncodeError::Other(format!("create PR failed: {text}")));
        }

        let pr: GitHubPullRequest = response
            .json()
            .await
            .map_err(|e| UncodeError::Other(e.to_string()))?;

        Ok(pr.html_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_issue_info_serde_roundtrip() {
        let issue = IssueInfo {
            number: 42,
            title: "Fix login bug".into(),
            body: "The login button does not work on Safari.".into(),
            url: Some("https://github.com/user/repo/issues/42".into()),
        };
        let json = serde_json::to_string(&issue).unwrap();
        let back: IssueInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(issue.number, back.number);
        assert_eq!(issue.title, back.title);
        assert_eq!(issue.body, back.body);
        assert_eq!(issue.url, back.url);
    }
}
