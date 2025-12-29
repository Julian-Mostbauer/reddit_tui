use std::collections::HashSet;
use std::sync::Arc;

use super::helpers::log_command;
use crate::reddit_api::model::{Comment, Post};

#[derive(PartialEq, Eq)]
pub enum View {
    List,
    Focus,
}

pub struct FocusItem {
    pub body: String,
    pub author: String,
    pub score: u32,
    pub path: String, // e.g. "0", "0.1" for nested comments
    pub depth: usize,
    pub has_children: bool,
}

pub struct App {
    pub command_mode: bool,
    pub input: String,
    pub messages: Vec<String>,

    // UI state
    pub view: View,
    pub posts: Vec<Post>,
    pub list_selected: usize,

    // focus view state
    pub focused: Option<Post>,
    pub comments: Vec<Comment>,
    pub focus_items: Vec<FocusItem>, // flattened view of focused post + comments
    pub focus_selected: usize,
    pub expanded: HashSet<String>, // set of paths that are expanded

    // runtime for async ops
    pub rt: Arc<tokio::runtime::Runtime>,

    // state for background loading
    pub loading_posts: bool,
    pub loading_comments: bool,

    // short-lived popup message (cleared automatically after a few frames)
    pub flash_message: Option<String>,
    pub flash_ttl: u8,
}

impl App {
    pub fn new(rt: Arc<tokio::runtime::Runtime>) -> Self {
        App {
            command_mode: false,
            input: String::new(),
            messages: Vec::new(),
            view: View::List,
            posts: Vec::new(),
            list_selected: 0,
            focused: None,
            comments: Vec::new(),
            focus_items: Vec::new(),
            focus_selected: 0,
            expanded: HashSet::new(),
            rt,
            loading_posts: false,
            loading_comments: false,
            flash_message: None,
            flash_ttl: 0,
        }
    }

    /// Submit the active command. Returns (quit_flag, optional_target_url).
    /// This no longer blocks; callers should spawn a background fetch if a target is returned.
    pub fn submit_command(&mut self) -> (bool, Option<String>) {
        let cmd = self.input.trim();
        if cmd == "q" {
            return (true, None);
        }

        if cmd.is_empty() {
            self.input.clear();
            self.command_mode = false;
            return (false, None);
        }

        // Parse commands: "home" | "goto <subreddit>" | "search <query>" | legacy forms
        let mut parts = cmd.splitn(2, char::is_whitespace);
        let action = parts.next().unwrap_or("");
        let rest = parts.next().map(|s| s.trim()).unwrap_or("");

        let target = match action {
            "home" | "/" => Some("https://www.reddit.com/.json".to_string()),
            "goto" => {
                if rest.is_empty() {
                    self.flash_message = Some("Usage: goto <subreddit>".to_string());
                    self.flash_ttl = 30;
                    None
                } else if rest.starts_with("r/") {
                    // accept either "goto r/name" or "goto name"
                    Some(format!("https://www.reddit.com/{}.json", rest))
                } else {
                    Some(format!("https://www.reddit.com/r/{}/.json", rest))
                }
            }
            "search" => {
                if rest.is_empty() {
                    self.flash_message = Some("Usage: search <query>".to_string());
                    self.flash_ttl = 30;
                    None
                } else {
                    Some(crate::reddit_api::fetch::build_search_url(rest))
                }
            }
            _ => {
                // Unknown command — we no longer accept bare subreddit or bare r/ forms
                self.flash_message = Some("Unknown command. Use 'goto <subreddit>' or 'search <query>'".to_string());
                self.flash_ttl = 30;
                None
            }
        };

        // Only log valid commands that produce a target URL
        if target.is_some() {
            if let Err(e) = log_command(cmd) {
                self.messages.push(format!("Failed to log command: {}", e));
            } else {
                self.messages.push(format!(":{} (loading)", cmd));
            }
        }

        self.input.clear();
        self.command_mode = false;
        (false, target)
    }

    pub fn open_focused(&mut self) {
        if self.posts.is_empty() {
            return;
        }
        let idx = self.list_selected.min(self.posts.len().saturating_sub(1));
        let p = self.posts[idx].clone();
        self.focus_selected = 0;
        self.comments.clear();
        self.focus_items.clear();
        self.expanded.clear();
        self.focused = Some(p);
        self.view = View::Focus;
    }

    /// Toggle expand/collapse or select the currently-focused item in the focus view.
    pub fn toggle_selected(&mut self) {
        if self.focus_items.is_empty() {
            return;
        }
        let idx = self.focus_selected;
        if idx >= self.focus_items.len() {
            return;
        }
        let path = self.focus_items[idx].path.clone();
        let has_children = self.focus_items[idx].has_children;
        if has_children {
            if self.expanded.contains(&path) {
                self.expanded.remove(&path);
            } else {
                self.expanded.insert(path.clone());
            }
            // remember selected path and rebuild items
            let selected_path = path.clone();
            self.build_focus_items();
            // if expansion resulted in no visible child nodes, undo and show a flash
            let child_prefix = format!("{}.", selected_path);
            let has_visible_child = self
                .focus_items
                .iter()
                .any(|it| it.path.starts_with(&child_prefix));
            if !has_visible_child {
                // undo expansion
                self.expanded.remove(&selected_path);
                self.build_focus_items();
            }
            // re-select the same item if present
            if let Some(pos) = self
                .focus_items
                .iter()
                .position(|it| it.path == selected_path)
            {
                self.focus_selected = pos;
            } else {
                self.focus_selected = 0;
            }
        } else {
            // leaf comment: show full comment body in messages
            let text = self.focus_items[idx].body.clone();
            self.messages.push(format!("Comment selected: {}", text));
        }
    }

    pub fn build_focus_items(&mut self) {
        self.focus_items.clear();

        fn walk(
            comments: &[Comment],
            out: &mut Vec<FocusItem>,
            path_prefix: String,
            depth: usize,
            expanded: &HashSet<String>,
        ) {
            for (i, c) in comments.iter().enumerate() {
                let path = if path_prefix.is_empty() {
                    format!("{}", i)
                } else {
                    format!("{}.{}", path_prefix, i)
                };
                let has_children = !c.replies.is_empty();
                let body_full = c.body.clone();
                // cap extremely long comment bodies to keep the UI responsive
                let body_snip = if body_full.chars().count() > 400 {
                    body_full.chars().take(400).collect::<String>() + "..."
                } else {
                    body_full.clone()
                };
                out.push(FocusItem {
                    body: body_snip,
                    author: c.author.clone(),
                    score: c.score,
                    path: path.clone(),
                    depth,
                    has_children,
                });
                if has_children && expanded.contains(&path) {
                    walk(&c.replies, out, path.clone(), depth + 1, expanded);
                }
            }
        }

        walk(
            &self.comments,
            &mut self.focus_items,
            String::new(),
            0,
            &self.expanded,
        );

        if self.focus_selected >= self.focus_items.len() {
            self.focus_selected = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reddit_api::model::Comment;
    use std::sync::Arc;

    #[test]
    fn test_toggle_selected_top_comment() {
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let mut app = App::new(rt);

        let child = Comment {
            body: "child".to_string(),
            replies: vec![],
            score: 1,
            author: "a".to_string(),
            permalink: "".to_string(),
        };
        let parent = Comment {
            body: "parent".to_string(),
            replies: vec![child],
            score: 2,
            author: "b".to_string(),
            permalink: "".to_string(),
        };
        app.comments = vec![parent];

        app.build_focus_items();
        assert!(!app.focus_items.is_empty());
        assert!(app.focus_items[0].has_children);

        app.focus_selected = 0;
        app.toggle_selected();

        assert!(app.expanded.contains("0"));
        assert!(app.focus_items.iter().any(|it| it.path.starts_with("0.")));
    }

    #[test]
    fn test_submit_command_variants() {
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let mut app = App::new(rt.clone());

        // home
        app.input = "home".to_string();
        let (quit, target) = app.submit_command();
        assert!(!quit);
        assert_eq!(target, Some("https://www.reddit.com/.json".to_string()));

        // goto bare
        app.input = "goto rust".to_string();
        let (_, target) = app.submit_command();
        assert_eq!(target, Some("https://www.reddit.com/r/rust/.json".to_string()));

        // goto with r/ (still works)
        app.input = "goto r/rust".to_string();
        let (_, target) = app.submit_command();
        assert_eq!(target, Some("https://www.reddit.com/r/rust.json".to_string()));

        // search
        app.input = "search cats".to_string();
        let (_, target) = app.submit_command();
        assert_eq!(target, Some(crate::reddit_api::fetch::build_search_url("cats")));

        // legacy r/ syntax is no longer accepted directly — expect no target and a flash message
        app.input = "r/AskReddit".to_string();
        let (_, target) = app.submit_command();
        assert_eq!(target, None);
        assert!(app.flash_message.is_some());
        assert!(app.flash_ttl > 0);

        // bare subreddit is no longer accepted directly
        app.input = "programming".to_string();
        let (_, target) = app.submit_command();
        assert_eq!(target, None);
        assert!(app.flash_message.is_some());
        assert!(app.flash_ttl > 0);

        // quit
        app.input = "q".to_string();
        let (quit, target) = app.submit_command();
        assert!(quit);
        assert_eq!(target, None);
    }
}
