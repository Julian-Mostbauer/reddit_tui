use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};

use super::app::{App, View};
use super::helpers::{wrap_words, wrapped_lines};

pub fn draw_frame(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Layout: left list vs right main. Give the list more space when no post is open
    let left_pct = if app.view == View::List { 65 } else { 35 };
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(100 - left_pct),
        ])
        .split(area);

    // Left: Post list view
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title("Posts (press ':' to load subreddit, Enter to open)");
    // Render each post as a compact two-line item: title on first line, small meta on second line
    let items: Vec<ListItem> = app
        .posts
        .iter()
        .map(|p| {
            let meta = format!(
                "↑ {} ↓   • {} comments  • /r/{}",
                p.score, p.num_comments, p.subreddit
            );
            let text = format!("{}\n{}", p.title, meta);
            ListItem::new(text)
        })
        .collect();
    let mut list_state = ratatui::widgets::ListState::default();
    if !app.posts.is_empty() {
        list_state.select(Some(app.list_selected));
    }
    let list = List::new(items).block(list_block).highlight_style(
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    f.render_stateful_widget(list, chunks[0], &mut list_state);

    // Right: Focus view or placeholder
    let right = chunks[1];
    if app.view == View::List {
        // show messages and instruction
        let msgs = app
            .messages
            .iter()
            .rev()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        let p =
            Paragraph::new(msgs).block(Block::default().borders(Borders::ALL).title("Status/Logs"));
        f.render_widget(p, right);
    } else {
        // Focus view: title, body (media + text), comments
        if let Some(pst) = &app.focused {
            // Build body text first so we can compute heights dynamically
            let mut body = String::new();
            if let Some(selftext) = &pst.selftext
                && !selftext.is_empty()
            {
                body.push_str(selftext);
                body.push('\n');
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

            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(title_height),
                    Constraint::Length(body_height),
                    Constraint::Min(1),
                ])
                .split(right);

            // title
            let vote_pct = pst.upvote_ratio * 100.0;
            let title = Paragraph::new(pst.title.clone())
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title(format!(
                    "Post — /r/{} — {} upvotes ({:.0}% upvoted) — {} comments",
                    pst.subreddit, pst.score, vote_pct, pst.num_comments
                )));
            f.render_widget(title, right_chunks[0]);

            // body area (selftext + media link)
            let body_p = Paragraph::new(body)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title("Body"));
            f.render_widget(body_p, right_chunks[1]);

            // comments area
            let carea = right_chunks[2];
            let col_width = carea.width as usize;
            let comments_items: Vec<ListItem> = app
                .focus_items
                .iter()
                .map(|it| {
                    let indent = "  ".repeat(it.depth);
                    let marker = if it.has_children {
                        if app.expanded.contains(&it.path) {
                            "[-]"
                        } else {
                            "[+]"
                        }
                    } else {
                        "[/]"
                    };
                    let prefix_len = indent.chars().count() + marker.chars().count() + 1; // +1 for space
                    let content_width = if col_width > prefix_len {
                        col_width - prefix_len
                    } else {
                        1
                    };

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
                })
                .collect();
            let comments = List::new(comments_items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Comments (press 'l' to load, Enter to expand)"),
                )
                .highlight_style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
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
                let p = Paragraph::new("Loading comments...")
                    .block(Block::default().borders(Borders::ALL).title("Loading"));
                f.render_widget(p, popup);
            }
        } else {
            let p = Paragraph::new("No post focused")
                .block(Block::default().borders(Borders::ALL).title("Post"));
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
        // increase height by 1 to show help line under the input
        let y = area.y + area.height.saturating_sub(5);
        let cmd_area = Rect::new(x, y, width, 4);

        // Clear background under popup
        f.render_widget(Clear, cmd_area);

        // top area: the command input with a border
        let cmd_text = format!(":{}", app.input);
        let style = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let cmd =
            Paragraph::new(cmd_text).block(Block::default().borders(Borders::ALL).title("Command"));
        // render it into the upper 3 rows of the popup (leaving one row for help)
        let input_rect = Rect::new(x, y, width, 3);
        f.render_widget(cmd.style(style), input_rect);

        // help line below the input (dimmed) — center horizontally under the command box
        let help_text = "Commands: goto <subreddit>, search <query>, help, home, o, q";
        let help_w = std::cmp::min(width.saturating_sub(4), help_text.len() as u16 + 2);
        let hx = x + (width.saturating_sub(help_w)) / 2;
        let help_rect = Rect::new(hx, y + 3, help_w, 1);
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center);
        f.render_widget(help, help_rect);

        // If browsing history, show a right-aligned small indicator like "history 3/10"
        if let Some(pos) = app.history_pos {
            let total = app.command_history.len().max(1);
            let hist_text = format!("history {}/{}", pos + 1, total);
            let hist_w = (hist_text.len() as u16).saturating_add(2);
            let hx = x + width.saturating_sub(hist_w).saturating_sub(1);
            let hist_rect = Rect::new(hx, y + 3, hist_w, 1);
            let hist = Paragraph::new(hist_text)
                .style(Style::default().fg(Color::Gray))
                .alignment(Alignment::Right);
            f.render_widget(hist, hist_rect);
        }
    }

    // Loading overlay (shows while posts are being fetched)
    if app.loading_posts {
        let width = std::cmp::min(50u16, area.width.saturating_sub(10));
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        // center vertically
        let y = area.y + (area.height / 2).saturating_sub(1);
        let popup = Rect::new(x, y, width, 3);
        f.render_widget(Clear, popup);
        let p = Paragraph::new("Loading posts...")
            .block(Block::default().borders(Borders::ALL).title("Loading"));
        f.render_widget(p, popup);
    }

    // Flash popup message (short-lived)
    if let Some(msg) = &app.flash_message {
        let width = std::cmp::min(50u16, area.width.saturating_sub(10));
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height / 2).saturating_sub(4);
        let popup = Rect::new(x, y, width, 3);
        f.render_widget(Clear, popup);
        let p = Paragraph::new(msg.clone())
            .block(Block::default().borders(Borders::ALL).title("Notice"));
        f.render_widget(p, popup);
    }

    // Help popup
    if app.show_help {
        let width = std::cmp::min(80u16, area.width.saturating_sub(10));
        let height = std::cmp::min(14u16, area.height.saturating_sub(6));
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let popup = Rect::new(x, y, width, height);
        f.render_widget(Clear, popup);

        let help_lines = vec![
            "Commands:",
            "  goto <subreddit>   — open subreddit (e.g. 'goto rust' or 'goto r/rust')",
            "  search <query>     — search reddit for a query",
            "  view [caca]        — view media locally (Linux only)",
            "  open               — open focused post in browser",
            "  home               — go to reddit home",
            "  help, ?            — show this help",
            "  q                  — quit",
            "",
            "Navigation: Enter to open post, l to load comments, Up/Down to move",
            "Press Esc or 'h' to dismiss",
        ]
        .join("\n");

        let p = Paragraph::new(help_lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(p, popup);
    }
}
