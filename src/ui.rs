use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Clear};
use ratatui::Terminal;

use crate::reddit_api::model::{Comment, Post};

#[derive(PartialEq, Eq)]
enum View {
    List,
    Focus,
}

struct FocusItem {
    pub text: String,
    pub path: String, // e.g. "0", "0.1" for nested comments
    pub depth: usize,
    pub has_children: bool,
}

struct App {
    command_mode: bool,
    input: String,
    messages: Vec<String>,

    // UI state
    view: View,
    posts: Vec<Post>,
    list_selected: usize,

    // focus view state
    focused: Option<Post>,
    comments: Vec<Comment>,
    focus_items: Vec<FocusItem>, // flattened view of focused post + comments
    focus_selected: usize,
    expanded: HashSet<String>, // set of paths that are expanded

    // runtime for async ops
    rt: Arc<tokio::runtime::Runtime>,
}

impl App {
    fn new(rt: Arc<tokio::runtime::Runtime>) -> Self {
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
        }
    }

    fn submit_command(&mut self) -> bool {
        let cmd = self.input.trim();
        if cmd == "q" {
            return true;
        }

        if !cmd.is_empty() {
            let target = if cmd == "home" || cmd == "/" {
                "https://www.reddit.com/.json".to_string()
            } else if cmd.starts_with("r/") {
                format!("https://www.reddit.com/{}.json", cmd)
            } else {
                format!("https://www.reddit.com/r/{}/.json", cmd)
            };

            if let Err(e) = log_command(cmd) {
                self.messages.push(format!("Failed to log command: {}", e));
            } else {
                self.messages.push(format!(":{} (loading)", cmd));
            }

            let res = self.rt.block_on(Post::get_posts(&target));
            match res {
                Ok(posts) => {
                    self.posts = posts;
                    self.list_selected = 0;
                    self.view = View::List;
                    self.messages.push(format!("Loaded {} posts", self.posts.len()));
                }
                Err(e) => {
                    self.messages.push(format!("Failed to load posts: {}", e));
                }
            }
        }

        self.input.clear();
        self.command_mode = false;
        false
    }

    fn open_focused(&mut self) {
        if self.posts.is_empty() { return; }
        let idx = self.list_selected.min(self.posts.len().saturating_sub(1));
        let p = self.posts[idx].clone();
        self.focus_selected = 0;
        self.comments.clear();
        self.focus_items.clear();
        self.expanded.clear();
        self.focused = Some(p);
        self.view = View::Focus;
    }

    fn load_comments(&mut self) {
        if let Some(p) = &self.focused {
            let url = format!("https://www.reddit.com{}.json", p.perma_link);
            let res = self.rt.block_on(Comment::get_comments(&url));
            match res {
                Ok(c) => {
                    self.comments = c;
                    self.expanded.clear(); // start collapsed
                    self.build_focus_items();
                    self.messages.push(format!("Loaded {} comments", self.comments.len()));
                }
                Err(e) => {
                    self.messages.push(format!("Failed to load comments: {}", e));
                }
            }
        }
    }

    fn build_focus_items(&mut self) {
        self.focus_items.clear();

        fn walk(comments: &Vec<Comment>, out: &mut Vec<FocusItem>, path_prefix: String, depth: usize, expanded: &HashSet<String>) {
            for (i, c) in comments.iter().enumerate() {
                let path = if path_prefix.is_empty() { format!("{}", i) } else { format!("{}.{}", path_prefix, i) };
                let has_children = !c.replies.is_empty();
                let indent = "  ".repeat(depth);
                let body_summary = c.body.lines().next().unwrap_or("");
                let mut text = format!("{}- {} (by {})", indent, body_summary, c.author);
                if has_children {
                    if expanded.contains(&path) {
                        text = format!("[-] {}", text);
                    } else {
                        text = format!("[+] {}", text);
                    }
                }
                out.push(FocusItem{ text, path: path.clone(), depth, has_children });
                if has_children && expanded.contains(&path) {
                    walk(&c.replies, out, path.clone(), depth+1, expanded);
                }
            }
        }

        walk(&self.comments, &mut self.focus_items, String::new(), 0, &self.expanded);

        if self.focus_selected >= self.focus_items.len() {
            self.focus_selected = 0;
        }
    }
}

fn log_command(cmd: &str) -> std::io::Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open("commands.log")?;
    writeln!(f, "{}", cmd)
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, LeaveAlternateScreen)?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
        return Err(err);
    }

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<(), Box<dyn std::error::Error>> {
    let rt = Arc::new(tokio::runtime::Runtime::new()?);
    let mut app = App::new(rt);

    // Initial load: homepage
    match app.rt.block_on(Post::get_posts("https://www.reddit.com/.json")) {
        Ok(posts) => { app.posts = posts; app.messages.push(format!("Loaded {} posts (home)", app.posts.len())); },
        Err(e) => app.messages.push(format!("Failed to load homepage: {}", e)),
    }

    loop {
        terminal.draw(|f| {
            let area = f.area();

            // Layout: left list (30%), right main (70%)
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(area);

            // Left: Post list view
            let list_block = Block::default().borders(Borders::ALL).title("Posts (press ':' to load subreddit, Enter to open)");
            // Render each post as a compact two-line item: title on first line, small meta on second line
            let items: Vec<ListItem> = app
                .posts
                .iter()
                .map(|p| {
                    let meta = format!("↑ {} ↓   • {} comments  • /r/{}", p.score, p.num_comments, p.subreddit);
                    let text = format!("{}\n{}", p.title, meta);
                    ListItem::new(text)
                })
                .collect();
            let mut list_state = ratatui::widgets::ListState::default();
            if !app.posts.is_empty() {
                list_state.select(Some(app.list_selected));
            }
            let list = List::new(items)
                .block(list_block)
                .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD));
            f.render_stateful_widget(list, chunks[0], &mut list_state);

            // Right: Focus view or placeholder
            let right = chunks[1];
            if app.view == View::List {
                // show messages and instruction
                let msgs = app.messages.iter().rev().take(10).cloned().collect::<Vec<_>>().join("\n");
                let p = Paragraph::new(msgs).block(Block::default().borders(Borders::ALL).title("Status/Logs"));
                f.render_widget(p, right);
            } else {
                // Focus view: title, body (media + text), comments
                let right_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Length(6), Constraint::Min(1)]).split(right);
                if let Some(pst) = &app.focused {
                    // title
                    let vote_pct = pst.upvote_ratio * 100.0;
                    let title = Paragraph::new(format!("{}", pst.title))
                        .block(Block::default().borders(Borders::ALL).title(format!("Post — /r/{} — {} upvotes ({:.0}% upvoted) — {} comments", pst.subreddit, pst.score, vote_pct, pst.num_comments)));
                    f.render_widget(title, right_chunks[0]);

                    // body area (selftext + media link)
                    let mut body = String::new();
                    if let Some(selftext) = &pst.selftext {
                        if !selftext.is_empty() {
                            body.push_str(selftext);
                            body.push('\n');
                        }
                    }
                    if let Some(media) = &pst.media_url {
                        body.push_str(&format!("Media: {}", media));
                    }
                    if body.is_empty() {
                        body = "(no body)".to_string();
                    }
                    let body_p = Paragraph::new(body).block(Block::default().borders(Borders::ALL).title("Body"));
                    f.render_widget(body_p, right_chunks[1]);

                    // comments area
                    let comments_items: Vec<ListItem> = app.focus_items.iter().map(|it| ListItem::new(it.text.clone())).collect();
                    let comments = List::new(comments_items).block(Block::default().borders(Borders::ALL).title("Comments (press 'l' to load, Enter to expand)"))
                        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD));
                    // Render list with selection handling
                    let mut state = ratatui::widgets::ListState::default();
                    if !app.focus_items.is_empty() {
                        state.select(Some(app.focus_selected));
                    }
                    f.render_stateful_widget(comments, right_chunks[2], &mut state);
                } else {
                    let p = Paragraph::new("No post focused").block(Block::default().borders(Borders::ALL).title("Post"));
                    f.render_widget(p, right);
                }
            }

            // Floating command bar when active
            if app.command_mode {
                // width: min(80, area.width - 10)
                let mut width = std::cmp::min(80u16, area.width.saturating_sub(10));
                if width < 10 {
                    width = area.width;
                }
                let x = area.x + (area.width.saturating_sub(width)) / 2;
                let y = area.y + area.height.saturating_sub(4);
                let cmd_area = Rect::new(x, y, width, 3);

                // Clear background under popup
                f.render_widget(Clear, cmd_area);

                let cmd_text = format!(":{}", app.input);
                let style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
                let cmd = Paragraph::new(cmd_text).block(Block::default().borders(Borders::ALL).title("Command"));
                f.render_widget(cmd.style(style), cmd_area);
            }
        })?;

        // Poll for events
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if app.command_mode {
                    match key.code {
                        KeyCode::Char(ch) => app.input.push(ch),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => { app.command_mode = false; app.input.clear(); }
                        KeyCode::Enter => { if app.submit_command() { break; } }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => break,
                        KeyCode::Char(':') => { app.command_mode = true; app.input.clear(); }
                        KeyCode::Up => {
                            if app.view == View::List {
                                if app.list_selected > 0 { app.list_selected -= 1; }
                            } else {
                                if app.focus_selected > 0 { app.focus_selected -= 1; }
                            }
                        }
                        KeyCode::Down => {
                            if app.view == View::List {
                                if app.list_selected + 1 < app.posts.len() { app.list_selected += 1; }
                            } else {
                                if app.focus_selected + 1 < app.focus_items.len() { app.focus_selected += 1; }
                            }
                        }
                        KeyCode::Enter => {
                            if app.view == View::List {
                                app.open_focused();
                            } else {
                                // if not post (i.e., a comment), toggle expand/collapse when applicable
                                if app.focus_selected == 0 {
                                    // nothing for the post
                                } else {
                                    let idx = app.focus_selected;
                                    if idx < app.focus_items.len() {
                                        let path = app.focus_items[idx].path.clone();
                                        let has_children = app.focus_items[idx].has_children;
                                        if has_children {
                                            if app.expanded.contains(&path) {
                                                app.expanded.remove(&path);
                                            } else {
                                                app.expanded.insert(path.clone());
                                            }
                                            // remember selected path and rebuild items
                                            let selected_path = path.clone();
                                            app.build_focus_items();
                                            // re-select the same item if present
                                            if let Some(pos) = app.focus_items.iter().position(|it| it.path == selected_path) {
                                                app.focus_selected = pos;
                                            } else {
                                                app.focus_selected = 0;
                                            }
                                        } else {
                                            // leaf comment: show full comment body in messages
                                            let text = app.focus_items[idx].text.clone();
                                            app.messages.push(format!("Comment selected: {}", text));
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            if app.view == View::Focus {
                                app.load_comments();
                            }
                        }
                        KeyCode::Esc => {
                            if app.view == View::Focus {
                                app.view = View::List;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}
