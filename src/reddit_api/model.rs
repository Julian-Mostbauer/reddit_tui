/*
structure of the json for a post inside a post listing, for example the reddit front page or a specific subreddit
{
kind: str,
data: {
  -- some unimportant fields
  children: [
      {
        kind: str, -- allways t3 for posts
        data: {
            -- here is the actual data of the post. There are many fields, most are not used by my model or under a different name
        }
      }
    ]
  }
}
*/

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

#[derive(Debug, Clone)]
pub struct Post {
    pub author: String,
    pub num_comments: u32,
    pub media_url: Option<String>,
    pub selftext: Option<String>,
    pub perma_link: String,
    pub subreddit: String,
    pub title: String,
    pub score: u32,
    pub upvote_ratio: f32,
    pub creation_time: i64,
}

impl Post {
    pub async fn get_posts(post_collection_url: &str) -> Result<Vec<Post>, FetchError> {
        // support local files (e.g. "src/reddit_api/json_examples/post_listing_with_two_posts.json"), file:// URIs,
        // or http(s) URLs like "https://www.reddit.com/.json"
        let content = if post_collection_url.starts_with("http://")
            || post_collection_url.starts_with("https://")
        {
            // fetch over HTTP with a User-Agent (reddit rejects empty/default agents)
            let client = reqwest::Client::builder()
                .user_agent("reddit_tui/0.1")
                .build()
                .map_err(FetchError::Http)?;
            let resp = client.get(post_collection_url).send().await.map_err(FetchError::Http)?;
            if !resp.status().is_success() {
                return Err(FetchError::Status(resp.status().as_u16()));
            }
            resp.text().await.map_err(FetchError::Http)?
        } else {
            let path = if let Some(p) = post_collection_url.strip_prefix("file://") {
                p
            } else {
                post_collection_url
            };
            std::fs::read_to_string(path).map_err(FetchError::Io)?
        };

        let json: Value = serde_json::from_str(&content).map_err(|e| FetchError::Parse(e.to_string()))?;

        let mut out = Vec::new();

        if let Some(children) = json
            .get("data")
            .and_then(|d| d.get("children"))
            .and_then(|c| c.as_array())
        {
            for child in children {
                let data = &child["data"];
                let author = data
                    .get("author")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let num_comments = data
                    .get("num_comments")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let media_url = data
                    .get("url_overridden_by_dest")
                    .or_else(|| data.get("url"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| {
                        if s.is_empty() || s == "self" {
                            None
                        } else {
                            Some(s.to_string())
                        }
                    });
                let selftext = data.get("selftext").and_then(|v| v.as_str()).and_then(|s| {
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                });
                let perma_link = data
                    .get("permalink")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let subreddit = data
                    .get("subreddit")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let title = data
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let score = data.get("score").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let upvote_ratio = data
                    .get("upvote_ratio")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                let creation_time = data
                    .get("created_utc")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as i64;

                out.push(Post {
                    author,
                    num_comments,
                    media_url,
                    selftext,
                    perma_link,
                    subreddit,
                    title,
                    score,
                    upvote_ratio,
                    creation_time,
                });
            }
        }

        Ok(out)
    }
}

/*
structure of the json for a post to find its comments
[
-- the first object is unimportant
  {
    kind: str,
    data: obj
  },
-- the second object contains the usefull data
  {
    kind: str,
    data: {
      -- a bunch of unimportant misc. fields
      -- the children array contains the comments.
      children: [
        {
        kind: str, -- for comments allways t1
        data: {
          -- here is the actual data of the comments. There are many fields, most are not used by my model or under a different name
          }
        }
      ]
    }
  },

]
 */

#[derive(Debug, Clone)]
pub struct Comment {
    pub body: String,
    pub replies: Vec<Comment>,
    pub score: u32,
    pub author: String,
    pub permalink: String,
}

impl Comment {
    pub async fn get_comments(post_url: &str) -> Result<Vec<Comment>, FetchError> {
        // support local files (e.g. "src/reddit_api/json_examples/post_with_one_comment.json")
        // or http(s) urls (async HTTP using reqwest with a User-Agent)
        let content = if post_url.starts_with("http://") || post_url.starts_with("https://") {
            let client = reqwest::Client::builder()
                .user_agent("reddit_tui/0.1")
                .build()
                .map_err(FetchError::Http)?;
            let resp = client.get(post_url).send().await.map_err(FetchError::Http)?;
            if !resp.status().is_success() {
                return Err(FetchError::Status(resp.status().as_u16()));
            }
            resp.text().await.map_err(FetchError::Http)?
        } else {
            let path = if let Some(p) = post_url.strip_prefix("file://") {
                p
            } else {
                post_url
            };
            std::fs::read_to_string(path).map_err(FetchError::Io)?
        };

        let json: Value = serde_json::from_str(&content).map_err(|e| FetchError::Parse(e.to_string()))?;

        // The expected structure is an array where the second element contains comments
        let mut out = Vec::new();
        if let Some(second) = json.as_array().and_then(|a| a.get(1)) {
            if let Some(children) = second
                .get("data")
                .and_then(|d| d.get("children"))
                .and_then(|c| c.as_array())
            {
                for child in children {
                    if let Some(comment) = parse_comment(child) {
                        out.push(comment);
                    }
                }
            }
        }

        Ok(out)
    }
}

fn parse_comment(v: &Value) -> Option<Comment> {
    let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("");
    if kind != "t1" {
        return None;
    }
    let data = v.get("data")?;
    let body = data
        .get("body")
        .and_then(|b| b.as_str())
        .unwrap_or("")
        .to_string();
    let score = data
        .get("score")
        .and_then(|s| s.as_i64())
        .or_else(|| data.get("ups").and_then(|s| s.as_i64()))
        .unwrap_or(0) as u32;
    let author = data
        .get("author")
        .and_then(|a| a.as_str())
        .unwrap_or("")
        .to_string();
    let permalink = data
        .get("permalink")
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();

    let mut replies = Vec::new();
    if let Some(rep) = data.get("replies") {
        if rep.is_object() {
            if let Some(children) = rep
                .get("data")
                .and_then(|d| d.get("children"))
                .and_then(|c| c.as_array())
            {
                for ch in children {
                    if let Some(rc) = parse_comment(ch) {
                        replies.push(rc);
                    }
                }
            }
        }
    }

    Some(Comment {
        body,
        replies,
        score,
        author,
        permalink,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[tokio::test]
    async fn test_get_posts_from_file() {
        let posts = Post::get_posts("src/reddit_api/json_examples/post_listing_with_two_posts.json").await.expect("get_posts failed");

        // Expect two posts in the example
        assert_eq!(posts.len(), 2);

        let p0 = &posts[0];
        // check that some key fields parsed correctly
        assert!(p0.title.len() > 0);
        assert_eq!(p0.subreddit, "AskReddit");
        assert!(p0.num_comments > 0);

        let p1 = &posts[1];
        assert_eq!(p1.subreddit, "YNNews");
        assert!(p1.media_url.is_some());
    }

    #[tokio::test]
    async fn test_get_posts_over_http() {
        // Start a tiny local HTTP server (single-request) and serve the example JSON over HTTP
        let file = std::fs::read_to_string(
            "src/reddit_api/json_examples/post_listing_with_two_posts.json",
        )
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    file.len(),
                    file
                );
                let _ = stream.write(response.as_bytes());
                let _ = stream.flush();
            }
        });

        let url = format!("http://{}/posts.json", addr);
        let posts = Post::get_posts(&url).await.expect("get_posts http failed");

        // Expect two posts in the example
        assert_eq!(posts.len(), 2);

        let p0 = &posts[0];
        // check that some key fields parsed correctly
        assert!(p0.title.len() > 0);
        assert_eq!(p0.subreddit, "AskReddit");
        assert!(p0.num_comments > 0);

        let p1 = &posts[1];
        assert_eq!(p1.subreddit, "YNNews");
        assert!(p1.media_url.is_some());
    }

    #[tokio::test]
    async fn test_get_comments_from_file() {
        let comments = Comment::get_comments("src/reddit_api/json_examples/post_with_one_comment.json").await.expect("get_comments failed");
        assert!(!comments.is_empty());

        let c0 = &comments[0];
        assert!(c0.body.len() > 0);
        assert!(c0.author.len() > 0);

        // ensure nested replies are parsed (if present in the example)
        if !c0.replies.is_empty() {
            assert!(c0.replies[0].body.len() > 0);
        }
    }

    #[tokio::test]
    async fn test_get_comments_over_http() {
        // start a tiny local HTTP server and serve the example json
        let file =
            std::fs::read_to_string("src/reddit_api/json_examples/post_with_one_comment.json")
                .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    file.len(),
                    file
                );
                let _ = stream.write(response.as_bytes());
                let _ = stream.flush();
            }
        });

        let url = format!("http://{}/comments.json", addr);
        let comments = Comment::get_comments(&url).await.expect("get_comments http failed");

        assert!(!comments.is_empty());

        let c0 = &comments[0];
        assert!(c0.body.len() > 0);
        assert!(c0.author.len() > 0);

        // ensure nested replies are parsed (if present in the example)
        if !c0.replies.is_empty() {
            assert!(c0.replies[0].body.len() > 0);
        }
    }

    #[tokio::test]
    async fn test_reddit_home_page() {
        // Optional live integration test. Enable by setting `REDDIT_TEST=1` in the environment.
        if std::env::var("REDDIT_TEST").is_err() {
            eprintln!("Skipping live reddit test (set REDDIT_TEST=1 to enable)");
            return;
        }

        let p = Post::get_posts("https://www.reddit.com/.json").await.expect("get_posts failed");
        assert!(!p.is_empty(), "expected to parse at least one post from Reddit");
        dbg!(p);
    }

    // Failure-mode tests
    #[tokio::test]
    async fn test_get_posts_missing_file_returns_io_error() {
        let res = Post::get_posts("nonexistent_file_hopefully_missing.json").await;
        match res {
            Err(FetchError::Io(_)) => {}
            other => panic!("expected Io error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_posts_http_404_returns_status_error() {
        // serve an HTTP 404 response
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.read(&mut [0u8; 1024]);
                let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write(response.as_bytes());
            }
        });

        let url = format!("http://{}/no.json", addr);
        let res = Post::get_posts(&url).await;
        match res {
            Err(FetchError::Status(code)) if code == 404 => {}
            other => panic!("expected Status(404), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_posts_http_invalid_data_returns_parse_error() {
        // serve a 200 with invalid JSON
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.read(&mut [0u8; 1024]);
                let body = "this is not json";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(), body
                );
                let _ = stream.write(response.as_bytes());
            }
        });

        let url = format!("http://{}/invalid.json", addr);
        let res = Post::get_posts(&url).await;
        match res {
            Err(FetchError::Parse(_)) => {}
            other => panic!("expected Parse error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_comments_missing_file_returns_io_error() {
        let res = Comment::get_comments("nonexistent_comments_file.json").await;
        match res {
            Err(FetchError::Io(_)) => {}
            other => panic!("expected Io error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_comments_http_404_returns_status_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.read(&mut [0u8; 1024]);
                let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                let _ = stream.write(response.as_bytes());
            }
        });

        let url = format!("http://{}/no_comments.json", addr);
        let res = Comment::get_comments(&url).await;
        match res {
            Err(FetchError::Status(code)) if code == 404 => {}
            other => panic!("expected Status(404), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_comments_http_invalid_data_returns_parse_error() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = stream.read(&mut [0u8; 1024]);
                let body = "not json";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(), body
                );
                let _ = stream.write(response.as_bytes());
            }
        });

        let url = format!("http://{}/invalid_comments.json", addr);
        let res = Comment::get_comments(&url).await;
        match res {
            Err(FetchError::Parse(_)) => {}
            other => panic!("expected Parse error, got {:?}", other),
        }
    }
}
