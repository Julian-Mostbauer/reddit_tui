use serde_json::Value;
use std::fmt;

#[derive(Debug)]
pub enum FetchError {
    Io(std::io::Error),
    Http(reqwest::Error),
    Status(u16),
    Parse(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Io(e) => write!(f, "IO error: {}", e),
            FetchError::Http(e) => write!(f, "HTTP error: {}", e),
            FetchError::Status(code) => write!(f, "HTTP status error: {}", code),
            FetchError::Parse(s) => write!(f, "Parse error: {}", s),
        }
    }
}

impl std::error::Error for FetchError {}

pub fn create_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("reddit-tui/0.1")
        .build()
        .unwrap()
}
pub fn create_client_blocking() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent("reddit-tui/0.1")
        .build()
        .unwrap()
}

/// Fetch content from either a local file (including file://) or HTTP(S).
/// Returns the raw text on success or a `FetchError` on failure.
pub async fn fetch_content(url: &str) -> Result<String, FetchError> {
    if url.starts_with("http://") || url.starts_with("https://") {
        let client = create_client();
        let resp = client.get(url).send().await.map_err(FetchError::Http)?;
        if !resp.status().is_success() {
            return Err(FetchError::Status(resp.status().as_u16()));
        }
        resp.text().await.map_err(FetchError::Http)
    } else {
        let path = if let Some(p) = url.strip_prefix("file://") {
            p
        } else {
            url
        };
        std::fs::read_to_string(path).map_err(FetchError::Io)
    }
}

/// Convenience: parse a JSON string into a serde_json::Value and map parse errors
pub fn parse_json(content: &str) -> Result<Value, FetchError> {
    serde_json::from_str(content).map_err(|e| FetchError::Parse(e.to_string()))
}

pub fn build_search_url(query: &str) -> String {
    format!("https://www.reddit.com/search/.json?q=({})", query)
}
