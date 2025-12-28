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
use ratatui::widgets::{Block, Borders, Paragraph, Clear};
use ratatui::Terminal;

struct App {
    command_mode: bool,
    input: String,
    messages: Vec<String>,
}

impl App {
    fn new() -> Self {
        App {
            command_mode: false,
            input: String::new(),
            messages: Vec::new(),
        }
    }

    fn submit_command(&mut self) -> bool {
        let cmd = self.input.trim();
        // Special-case: 'q' command quits the application
        if cmd == "q" {
            return true;
        }
        if !cmd.is_empty() {
            // Log to file
            if let Err(e) = log_command(cmd) {
                self.messages.push(format!("Failed to log command: {}", e));
            } else {
                self.messages.push(format!(":{}", cmd));
            }
        }
        self.input.clear();
        self.command_mode = false;
        false
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
    let mut app = App::new();

    loop {
        terminal.draw(|f| {
            let area = f.area();

            // Main area: show message history
            let msgs = app
                .messages
                .iter()
                .rev()
                .take(10)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let messages = Paragraph::new(msgs).block(Block::default().borders(Borders::ALL).title("Messages"));
            f.render_widget(messages, area);

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
                match (key.code, app.command_mode) {
                    (KeyCode::Char('c'), false) if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        // Exit on Ctrl-C
                        break;
                    }
                    (KeyCode::Char(':'), false) => {
                        app.command_mode = true;
                        app.input.clear();
                    }
                    (KeyCode::Char(ch), true) => {
                        app.input.push(ch);
                    }
                    (KeyCode::Backspace, true) => {
                        app.input.pop();
                    }
                    (KeyCode::Esc, true) => {
                        app.command_mode = false;
                        app.input.clear();
                    }
                    (KeyCode::Enter, true) => {
                        if app.submit_command() {
                            break;
                        }
                    }
                    // allow quitting with 'q' when not in command mode
                    (KeyCode::Char('q'), false) => break,
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
