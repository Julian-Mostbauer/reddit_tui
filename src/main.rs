mod reddit_api;

use std::fs::OpenOptions;
use std::io::Write;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Clear, List, ListItem};

use ratatui::Terminal;

use crate::reddit_api::model::{Comment, Post};
use std::sync::Arc;

#[derive(PartialEq, Eq)]
enum View {
    List,
    Focus,
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
    focus_items: Vec<String>, // flattened view of focused post + comments
    focus_selected: usize,

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
            rt,
        }
    }

    fn submit_command(&mut self) -> bool {
        let cmd = self.input.trim();
        // Special-case: 'q' command quits the application
        if cmd == "q" {
            return true;
        }

        if !cmd.is_empty() {
            // parse commands: 'home' or 'r/<subreddit>'
            let target = if cmd == "home" || cmd == "/" {
                "https://www.reddit.com/.json".to_string()
            } else if cmd.starts_with("r/") {
                format!("https://www.reddit.com/{}.json", cmd)
            } else {
                // treat as subreddit name
                format!("https://www.reddit.com/r/{}/.json", cmd)
            };

            // Log command
            if let Err(e) = log_command(cmd) {
                self.messages.push(format!("Failed to log command: {}", e));
            } else {
                self.messages.push(format!(":{} (loading)", cmd));
            }

            // fetch posts synchronously via runtime
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
        // first item is the post title and metadata
        self.focus_items.push(format!("{} - /r/{} ({} comments)", p.title, p.subreddit, p.num_comments));
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
        if let Some(p) = &self.focused {
            self.focus_items.push(format!("POST: {} - /r/{}", p.title, p.subreddit));
        }
        fn walk(comments: &Vec<Comment>, out: &mut Vec<String>, depth: usize) {
            for c in comments {
                let indent = "  ".repeat(depth);
                let body_summary = c.body.lines().next().unwrap_or("");
                out.push(format!("{}- {} (by {})", indent, body_summary, c.author));
                if !c.replies.is_empty() {
                    walk(&c.replies, out, depth+1);
                }
            }
        }
        walk(&self.comments, &mut self.focus_items, 0);
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
            let items: Vec<ListItem> = app
                .posts
                .iter()
                .map(|p| ListItem::new(format!("{} - /r/{}", p.title, p.subreddit)))
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
                // Focus view
                let right_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(1)]).split(right);
                if let Some(pst) = &app.focused {
                    let title = Paragraph::new(format!("{}\n/r/{} — {} comments", pst.title, pst.subreddit, pst.num_comments))
                        .block(Block::default().borders(Borders::ALL).title("Post"));
                    f.render_widget(title, right_chunks[0]);

                    // comments area
                    let comments_items: Vec<ListItem> = app.focus_items.iter().map(|s| ListItem::new(s.clone())).collect();
                    let comments = List::new(comments_items).block(Block::default().borders(Borders::ALL).title("Comments (press 'l' to load)"));
                    // Render list with selection handling
                    let mut state = ratatui::widgets::ListState::default();
                    if !app.focus_items.is_empty() {
                        state.select(Some(app.focus_selected));
                    }
                    f.render_stateful_widget(comments, right_chunks[1], &mut state);
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
                        KeyCode::Char('q') => break,
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
                                // if not post (i.e., a comment), show comment body in messages
                                if app.focus_selected > 0 {
                                    let idx = app.focus_selected - 1; // because 0 is the post
                                    if idx < app.focus_items.len() {
                                        let line = app.focus_items[app.focus_selected].clone();
                                        app.messages.push(format!("Selected: {}", line));
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
