use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;
use std::sync::mpsc::channel;
use std::thread;

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Clear, Wrap};

use ratatui::Terminal;

use crate::reddit_api::model::{Comment, Post, FetchError};

#[derive(PartialEq, Eq)]
enum View {
    List,
    Focus,
}

struct FocusItem {
    pub body: String,
    pub author: String,
    pub score: u32,
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

    // state for background loading
    loading_posts: bool,
    loading_comments: bool,
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
            loading_posts: false,
            loading_comments: false,
        }
    }

    /// Submit the active command. Returns (quit_flag, optional_target_url).
    /// This no longer blocks; callers should spawn a background fetch if a target is returned.
    fn submit_command(&mut self) -> (bool, Option<String>) {
        let cmd = self.input.trim();
        if cmd == "q" {
            return (true, None);
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

            self.input.clear();
            self.command_mode = false;
            (false, Some(target))
        } else {
            self.input.clear();
            self.command_mode = false;
            (false, None)
        }
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
                let body_full = c.body.clone();
                // cap extremely long comment bodies to keep the UI responsive
                let body_snip = if body_full.chars().count() > 400 {
                    body_full.chars().take(400).collect::<String>() + "..."
                } else {
                    body_full.clone()
                };
                out.push(FocusItem{ body: body_snip, author: c.author.clone(), score: c.score, path: path.clone(), depth, has_children });
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

// Rough calculation of wrapped lines for a text given a width in cells.
// This is an approximation using character counts (doesn't handle wide/unicode widths perfectly)
fn wrapped_lines(text: &str, width: u16) -> u16 {
    if width == 0 { return 1; }
    let mut lines: u16 = 0;
    for ln in text.lines() {
        let len = ln.chars().count() as u16;
        if len == 0 {
            lines = lines.saturating_add(1);
        } else {
            lines = lines.saturating_add((len + width - 1) / width);
        }
    }
    if lines == 0 { 1 } else { lines }
}

// Simple word-wrapping into lines of at most `width` characters (approximate, splits long words)
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    if width == 0 { return vec![text.to_string()] }
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            if line.is_empty() {
                if word.chars().count() > width {
                    // hard-split long word
                    let mut start = 0;
                    let chars: Vec<char> = word.chars().collect();
                    while start < chars.len() {
                        let end = std::cmp::min(start + width, chars.len());
                        out.push(chars[start..end].iter().collect());
                        start = end;
                    }
                } else {
                    line.push_str(word);
                }
            } else {
                let potential = line.chars().count() + 1 + word.chars().count();
                if potential <= width {
                    line.push(' ');
                    line.push_str(word);
                } else {
                    out.push(line.clone());
                    line.clear();
                    if word.chars().count() > width {
                        let mut start = 0;
                        let chars: Vec<char> = word.chars().collect();
                        while start < chars.len() {
                            let end = std::cmp::min(start + width, chars.len());
                            out.push(chars[start..end].iter().collect());
                            start = end;
                        }
                    } else {
                        line.push_str(word);
                    }
                }
            }
        }
        if !line.is_empty() {
            out.push(line.clone());
        }
        // preserve paragraph break as an empty line
        if para != "" { /* do nothing */ } else { out.push(String::new()); }
    }
    if out.is_empty() { out.push(String::new()); }
    out
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

    // Initial load: homepage (spawn background fetch so UI can show a loading indicator)
    let (tx, rx) = channel::<Result<Vec<Post>, FetchError>>();
    // channel for comment loads
    let (ctx, crx) = channel::<Result<Vec<Comment>, FetchError>>();
    app.loading_posts = true;
    let tx0 = tx.clone();
    let rt0 = app.rt.clone();
    let url0 = "https://www.reddit.com/.json".to_string();
    thread::spawn(move || {
        let res = rt0.block_on(Post::get_posts(&url0));
        let _ = tx0.send(res);
    });

    // we'll poll `rx` and `crx` in the main loop to pick up results

    loop {
        // check for background post load completion
        if let Ok(res) = rx.try_recv() {
            app.loading_posts = false;
            match res {
                Ok(posts) => { app.posts = posts; app.list_selected = 0; app.messages.push(format!("Loaded {} posts", app.posts.len())); }
                Err(e) => { app.messages.push(format!("Failed to load posts: {}", e)); }
            }
        }

        // check for background comment load completion
        if let Ok(res) = crx.try_recv() {
            app.loading_comments = false;
            match res {
                Ok(comments) => { app.comments = comments; app.expanded.clear(); app.build_focus_items(); app.messages.push(format!("Loaded {} comments", app.comments.len())); }
                Err(e) => { app.messages.push(format!("Failed to load comments: {}", e)); }
            }
        }
        terminal.draw(|f| {
            let area = f.area();

            // Layout: left list vs right main. Give the list more space when no post is open
            let left_pct = if app.view == View::List { 65 } else { 35 };
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(left_pct), Constraint::Percentage(100 - left_pct)])
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
                if let Some(pst) = &app.focused {
                    // Build body text first so we can compute heights dynamically
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

                    // Compute available inner width (account for borders)
                    let inner_w = right.width.saturating_sub(2);
                    // compute how many wrapped lines the title/body need
                    // allow the title to take up to half the right pane height so long titles can wrap
                    let max_title_inner = (right.height.saturating_sub(4) / 2).max(1);
                    let title_lines = wrapped_lines(&pst.title, inner_w).clamp(1, max_title_inner);
                    let title_height = title_lines.saturating_add(2); // add border space
                    // body should be at least 3 lines but no more than available space after reserving title
                    let max_body_inner = right.height.saturating_sub(title_height + 3).max(1);
                    let body_inner_lines = wrapped_lines(&body, inner_w).clamp(3, max_body_inner);
                    let body_height = body_inner_lines.saturating_add(2); // add border space

                    let right_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(title_height), Constraint::Length(body_height), Constraint::Min(1)]).split(right);

                    // title
                    let vote_pct = pst.upvote_ratio * 100.0;
                    let title = Paragraph::new(pst.title.clone())
                        .wrap(Wrap { trim: true })
                        .block(Block::default().borders(Borders::ALL).title(format!("Post — /r/{} — {} upvotes ({:.0}% upvoted) — {} comments", pst.subreddit, pst.score, vote_pct, pst.num_comments)));
                    f.render_widget(title, right_chunks[0]);

                    // body area (selftext + media link)
                    let body_p = Paragraph::new(body).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).title("Body"));
                    f.render_widget(body_p, right_chunks[1]);

                    // comments area
                    let carea = right_chunks[2];
                    let col_width = carea.width as usize;
                    let comments_items: Vec<ListItem> = app.focus_items.iter().map(|it| {
                        let indent = "  ".repeat(it.depth);
                        let marker = if it.has_children { if app.expanded.contains(&it.path) { "[-]" } else { "[+]" } } else { "[/]" };
                        let prefix_len = indent.chars().count() + marker.chars().count() + 1; // +1 for space
                        let content_width = if col_width > prefix_len { col_width - prefix_len } else { 1 };

                        // wrap body into lines
                        let lines = wrap_words(&it.body, content_width);
                        let mut string_lines: Vec<String> = Vec::new();
                        for (i, ln) in lines.iter().enumerate() {
                            let line = if i == 0 {
                                format!("{}{} {}", indent, marker, ln)
                            } else {
                                let cont_prefix = " ".repeat(marker.chars().count() + 1);
                                format!("{}{}{}", indent, cont_prefix, ln)
                            };
                            string_lines.push(line);
                        }

                        let meta = format!("{}  by {}  • ↑ {} ↓", indent, it.author, it.score);
                        string_lines.push(meta);

                        let content = string_lines.join("\n");
                        ListItem::new(content)
                    }).collect();
                    let comments = List::new(comments_items).block(Block::default().borders(Borders::ALL).title("Comments (press 'l' to load, Enter to expand)"))
                        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD));
                    // Render list with selection handling
                    let mut state = ratatui::widgets::ListState::default();
                    if !app.focus_items.is_empty() {
                        state.select(Some(app.focus_selected));
                    }
                    f.render_stateful_widget(comments, right_chunks[2], &mut state);

                    // Loading overlay for comments
                    if app.loading_comments {
                        let carea = right_chunks[2];
                        let width = std::cmp::min(40u16, carea.width.saturating_sub(10));
                        let x = carea.x + (carea.width.saturating_sub(width)) / 2;
                        let y = carea.y + (carea.height / 2).saturating_sub(1);
                        let popup = Rect::new(x, y, width, 3);
                        f.render_widget(Clear, popup);
                        let p = Paragraph::new("Loading comments...").block(Block::default().borders(Borders::ALL).title("Loading"));
                        f.render_widget(p, popup);
                    }
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

            // Loading overlay (shows while posts are being fetched)
            if app.loading_posts {
                let width = std::cmp::min(50u16, area.width.saturating_sub(10));
                let x = area.x + (area.width.saturating_sub(width)) / 2;
                // center vertically
                let y = area.y + (area.height / 2).saturating_sub(1);
                let popup = Rect::new(x, y, width, 3);
                f.render_widget(Clear, popup);
                let p = Paragraph::new("Loading posts...").block(Block::default().borders(Borders::ALL).title("Loading"));
                f.render_widget(p, popup);
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
                        KeyCode::Enter => {
                            let (quit, target) = app.submit_command();
                            if quit { break; }
                            if let Some(target) = target {
                                app.loading_posts = true;
                                app.posts.clear();
                                let tx2 = tx.clone();
                                let rt2 = app.rt.clone();
                                thread::spawn(move || {
                                    let res = rt2.block_on(Post::get_posts(&target));
                                    let _ = tx2.send(res);
                                });
                            }
                        }
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
                                            let text = app.focus_items[idx].body.clone();
                                            app.messages.push(format!("Comment selected: {}", text));
                                        }
                                    }
                                }
                            }
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            if app.view == View::Focus {
                                if let Some(p) = &app.focused {
                                    let url = format!("https://www.reddit.com{}.json", p.perma_link);
                                    app.loading_comments = true;
                                    app.comments.clear();
                                    app.focus_items.clear();
                                    let ctx2 = ctx.clone();
                                    let rt2 = app.rt.clone();
                                    thread::spawn(move || {
                                        let res = rt2.block_on(Comment::get_comments(&url));
                                        let _ = ctx2.send(res);
                                    });
                                }
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
