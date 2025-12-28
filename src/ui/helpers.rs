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

pub fn log_command(cmd: &str) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open("commands.log")?;
    f.write_all(cmd.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}
