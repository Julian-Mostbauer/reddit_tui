use std::io::Stdout;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::app::App;
use super::render::draw_frame;
use crate::reddit_api::model::{Comment, FetchError, Post};

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

pub fn run_app(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let rt = std::sync::Arc::new(tokio::runtime::Runtime::new()?);
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
                Ok(posts) => {
                    app.posts = posts;
                    app.list_selected = 0;
                    app.messages
                        .push(format!("Loaded {} posts", app.posts.len()));
                }
                Err(e) => {
                    app.messages.push(format!("Failed to load posts: {}", e));
                }
            }
        }

        // check for background comment load completion
        if let Ok(res) = crx.try_recv() {
            app.loading_comments = false;
            match res {
                Ok(comments) => {
                    app.comments = comments;
                    app.expanded.clear();
                    app.build_focus_items();
                    app.messages
                        .push(format!("Loaded {} comments", app.comments.len()));
                }
                Err(e) => {
                    app.messages.push(format!("Failed to load comments: {}", e));
                }
            }
        }

        // decrement/clear flash popup TTL
        if app.flash_ttl > 0 {
            app.flash_ttl -= 1;
            if app.flash_ttl == 0 {
                app.flash_message = None;
            }
        }

        terminal.draw(|f| draw_frame(f, &mut app))?;

        // Poll for events
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            if app.command_mode {
                match key.code {
                    KeyCode::Char(ch) => app.input.push(ch),
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Esc => {
                        app.command_mode = false;
                        app.input.clear();
                    }
                    KeyCode::Enter => {
                        let (quit, target) = app.submit_command();
                        if quit {
                            break;
                        }
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
                    KeyCode::Up => { app.history_prev(); }
                    KeyCode::Down => { app.history_next(); }
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Char('c')
                        if key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL) =>
                    {
                        break;
                    }
                    KeyCode::Char(':') => {
                        app.command_mode = true;
                        app.input.clear();
                    }
                    KeyCode::Up => {
                        if app.view == super::app::View::List && app.list_selected > 0 {
                            app.list_selected -= 1;
                        } else if app.focus_selected > 0 {
                            app.focus_selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        if app.view == super::app::View::List
                            && app.list_selected + 1 < app.posts.len()
                        {
                            app.list_selected += 1;
                        } else if app.focus_selected + 1 < app.focus_items.len() {
                            app.focus_selected += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if app.view == super::app::View::List {
                            app.open_focused();
                        } else {
                            // Toggle expand/collapse or select the current focused comment
                            app.toggle_selected();
                        }
                    }
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        if app.view == super::app::View::Focus
                            && let Some(p) = &app.focused
                        {
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
                    KeyCode::Esc => {
                        if app.view == super::app::View::Focus {
                            app.view = super::app::View::List;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
