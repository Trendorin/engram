//! Terminal output helpers for the human-facing commands (doctor, setup,
//! config, uninstall). Colours only when stdout is a terminal and NO_COLOR is unset.

use std::io::{BufRead, IsTerminal, Write};

fn color() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

fn paint(code: &str, s: &str) -> String {
    if color() {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    paint("1", s)
}

pub fn dim(s: &str) -> String {
    paint("2", s)
}

pub fn ok(label: &str, detail: impl AsRef<str>) {
    println!("  {} {label:<20} {}", paint("32", "✓"), detail.as_ref());
}

pub fn bad(label: &str, detail: impl AsRef<str>) {
    println!("  {} {label:<20} {}", paint("31", "✗"), detail.as_ref());
}

pub fn warn(label: &str, detail: impl AsRef<str>) {
    println!("  {} {label:<20} {}", paint("33", "!"), detail.as_ref());
}

/// Ask a yes/no question on the terminal. Without a terminal, `default` wins.
pub fn confirm(question: &str, default: bool) -> bool {
    if !std::io::stdin().is_terminal() {
        return default;
    }
    let hint = if default { "[Y/n]" } else { "[y/N]" };
    print!("  {} {question} {} ", paint("36", "?"), dim(hint));
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return default;
    }
    match line.trim().to_ascii_lowercase().as_str() {
        "" => default,
        a => a.starts_with('y') || a.starts_with('д'),
    }
}

/// `$HOME/x` → `~/x` for display.
pub fn tilde(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s,
    }
}
