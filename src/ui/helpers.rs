// Rough calculation of wrapped lines for a text given a width in cells.
// This is an approximation using character counts (doesn't handle wide/unicode widths perfectly)
pub fn wrapped_lines(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }
    let mut lines: u16 = 0;
    for ln in text.lines() {
        let len = ln.chars().count() as u16;
        if len == 0 {
            lines = lines.saturating_add(1);
        } else {
            lines = lines.saturating_add(len.div_ceil(width));
        }
    }
    if lines == 0 { 1 } else { lines }
}

// Simple word-wrapping into lines of at most `width` characters (approximate, splits long words)
pub fn wrap_words(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
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
        if para.is_empty() {
            out.push(String::new());
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

pub fn log_command_to_path(cmd: &str, path: &std::path::Path) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(cmd.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

pub fn log_command(cmd: &str) -> std::io::Result<()> {
    let path = history_file_path();
    log_command_to_path(cmd, &path)
}
/// Load command history from `commands.log` if present. Trims whitespace and ignores empty lines.
use std::path::PathBuf;

fn history_file_path() -> PathBuf {
    // Prefer XDG_CONFIG_HOME/reddit_tui/history, fall back to $HOME/.config/reddit_tui/history,
    // otherwise use ./commands.log as a last resort.
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let mut p = PathBuf::from(xdg);
        p.push("reddit_tui");
        if let Err(_) = std::fs::create_dir_all(&p) {}
        p.push("history");
        return p;
    }

    if let Ok(home) = std::env::var("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".config/reddit_tui");
        if let Err(_) = std::fs::create_dir_all(&p) {}
        p.push("history");
        return p;
    }

    PathBuf::from("commands.log")
}

pub fn load_command_history_from_path(path: &std::path::Path) -> Vec<String> {
    match std::fs::read_to_string(path) {
        Ok(s) => s
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub fn load_command_history() -> Vec<String> {
    let path = history_file_path();
    load_command_history_from_path(&path)
}

/// Open the given URL in the system default browser. On test builds this is a no-op to avoid
/// launching a browser during unit tests.
pub fn open_in_browser(url: &str) -> std::io::Result<()> {
    if cfg!(test) {
        // In tests, avoid spawning external programs
        return Ok(());
    }

    use std::process::{Command, Stdio};
    if cfg!(target_os = "linux") {
        Command::new("xdg-open")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    } else if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "unsupported OS"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_load_command_history_reads_file() {
        // Prepare an isolated temp path for this test
        let td = std::env::temp_dir().join(format!("reddit_tui_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&td);
        std::fs::create_dir_all(&td).unwrap();
        let path = td.join("history");

        let mut f = File::create(&path).expect("create log");
        writeln!(f, "home").unwrap();
        writeln!(f, "search cats").unwrap();
        writeln!(f, "goto rust").unwrap();

        let h = load_command_history_from_path(&path);
        assert_eq!(h.len(), 3);
        assert_eq!(h[0], "home");
        assert_eq!(h[1], "search cats");
        assert_eq!(h[2], "goto rust");

        let _ = std::fs::remove_dir_all(&td);
    }

    #[test]
    fn test_log_command_appends() {
        // Use an isolated temp path for append test
        let td = std::env::temp_dir().join(format!("reddit_tui_test_log_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&td);
        std::fs::create_dir_all(&td).unwrap();
        let path = td.join("history");

        log_command_to_path("home", &path).unwrap();
        log_command_to_path("search cats", &path).unwrap();
        let s = std::fs::read_to_string(&path).unwrap();
        eprintln!("history path: {:?}", &path);
        eprintln!("history content:\n{}", &s);
        assert!(s.contains("home"));
        assert!(s.contains("search cats"));
        let _ = std::fs::remove_dir_all(&td);
    }
}