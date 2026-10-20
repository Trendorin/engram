//! Plain-text rendering of tool results for the Inspect tab and the status bar.

use engram_proto::RpcError;
use serde_json::Value;
use std::fmt::Write;

pub fn s(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}

/// Strings as-is, everything else as JSON (`null` → "").
pub fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn error(e: &RpcError) -> String {
    match &e.hint {
        Some(h) => format!("{} — {h}", e.message),
        None => e.message.clone(),
    }
}

/// Local wall-clock time of a millisecond Unix timestamp, `HH:MM:SS`.
pub fn clock(ts_ms: u64) -> String {
    let t = (ts_ms / 1000) as libc::time_t;
    // SAFETY: localtime_r only writes the `tm` we pass.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return String::new();
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// `$HOME/x` → `~/x` for display.
pub fn tilde(p: &std::path::Path) -> String {
    let s = p.display().to_string();
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s,
    }
}

pub fn thousands(n: u64) -> String {
    let d = n.to_string();
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn analyze(v: &Value) -> String {
    let mut o = String::new();
    let _ = writeln!(o, "address   {}", s(&v["address"]));
    let _ = writeln!(o, "location  {}", s(&v["location"]));
    if let Some(r) = v["region"].as_object() {
        let path = r.get("path").map(text).unwrap_or_default();
        let _ = writeln!(o, "region    {}-{} {} {} {}", text(&r["start"]), text(&r["end"]), text(&r["perms"]), text(&r["kind"]), path);
    }
    let _ = writeln!(o, "bytes     {}", s(&v["bytes"]));
    if let Some(p) = v["pointer_to"].as_str() {
        let _ = writeln!(o, "pointer → {p}");
    }
    if let Some(t) = v["text"].as_str() {
        let _ = writeln!(o, "text      \"{t}\"");
    }
    if let Some(types) = v["as_types"].as_object() {
        let _ = writeln!(o, "\nas types");
        for (k, val) in types {
            let _ = writeln!(o, "  {k:<6} {}", text(val));
        }
    }
    o
}

pub fn dissect(v: &Value) -> String {
    let mut o = format!("base {}  ({})\n\n", s(&v["base"]), s(&v["location"]));
    let _ = writeln!(o, "{:<8} {:<8} {:<28} note", "offset", "guess", "value");
    for f in v["fields"].as_array().into_iter().flatten() {
        let _ = writeln!(o, "{:<8} {:<8} {:<28} {}", s(&f["offset"]), s(&f["guess"]), s(&f["value"]), text(&f["note"]));
    }
    o
}

pub fn disassemble(v: &Value) -> String {
    let mut o = String::new();
    for i in v["instructions"].as_array().into_iter().flatten() {
        let _ = writeln!(o, "{:<26} {:<30} {}", s(&i["location"]), s(&i["bytes"]), s(&i["text"]));
    }
    o
}

pub fn hexdump(v: &Value) -> String {
    v["dump"].as_array().into_iter().flatten().map(|l| format!("{}\n", s(l))).collect()
}

pub fn sample(v: &Value) -> String {
    let values: Vec<String> =
        v["values"].as_array().into_iter().flatten().map(|x| if x.is_null() { "??".into() } else { text(x) }).collect();
    let mut o = format!(
        "{} samples every {} ms: {} changes, {} distinct values\n",
        values.len(),
        text(&v["interval_ms"]),
        text(&v["changes"]),
        text(&v["distinct"])
    );
    if !v["min"].is_null() {
        let _ = writeln!(o, "min {}  max {}", text(&v["min"]), text(&v["max"]));
    }
    o.push('\n');
    for (i, x) in values.iter().enumerate() {
        let _ = writeln!(o, "{i:>3}  {x}");
    }
    o
}

pub fn pointer_scan(v: &Value) -> String {
    let paths = v["paths"].as_array().cloned().unwrap_or_default();
    let mut o = format!("{} path(s) to {}\n\n", paths.len(), s(&v["target"]));
    for p in &paths {
        let mark = if p["verified"] == Value::Bool(true) { "✓" } else { " " };
        let _ = writeln!(o, "{mark} {}", s(&p["expr"]));
    }
    let _ = write!(o, "\n{}\nCopy a path into a table entry's Address field to keep it across restarts.", s(&v["hint"]));
    o
}

pub fn watch(v: &Value) -> String {
    let list = v["instructions"].as_array().cloned().unwrap_or_default();
    let mut o = format!("{} instruction(s), {} thread(s), {} ms\n\n", list.len(), text(&v["threads"]), text(&v["elapsed_ms"]));
    for i in &list {
        let _ = writeln!(o, "{:>6}×  {:<28} {}", text(&i["count"]), s(&i["location"]), i["instruction"].as_str().unwrap_or("?"));
    }
    if let Some(first) = list.first() {
        if let Some(regs) = first["registers"].as_object() {
            let _ = writeln!(o, "\nregisters at the first hit of the top instruction:");
            let named = REG_ORDER.iter().filter_map(|k| regs.get(*k).map(|v| (k.to_string(), v)));
            let rest = regs.iter().filter(|(k, _)| !REG_ORDER.contains(&k.as_str())).map(|(k, v)| (k.clone(), v));
            let all: Vec<(String, &Value)> = named.chain(rest).collect();
            for chunk in all.chunks(3) {
                let line: Vec<String> = chunk.iter().map(|(k, v)| format!("{k:>3} {:<18}", text(v))).collect();
                let _ = writeln!(o, "  {}", line.join(" ").trim_end());
            }
        }
    }
    let _ = write!(o, "\n{}", s(&v["hint"]));
    o
}

const REG_ORDER: &[&str] =
    &["rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "rip"];

/// Split a command line into argv, honouring simple '…' and "…" quoting.
pub fn split_command(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            (None, c) => {
                cur.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

/// Table type for a scan hit: patterns get their length from the shown value.
pub fn entry_type(scan_type: &str, value: &str) -> String {
    match scan_type {
        "bytes" => format!("bytes:{}", value.split_whitespace().count().max(1)),
        "string" => format!("str:{}", value.len().max(1)),
        "string16" => format!("str16:{}", value.encode_utf16().count().max(1)),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_commands() {
        assert_eq!(split_command(r#"/opt/game "My Game.exe" --x='a b'"#), vec!["/opt/game", "My Game.exe", "--x=a b"]);
        assert_eq!(split_command("  a   b "), vec!["a", "b"]);
        assert_eq!(split_command(r#"a """#), vec!["a", ""]);
    }

    #[test]
    fn pattern_hits_get_lengths() {
        assert_eq!(entry_type("bytes", "48 8b 05"), "bytes:3");
        assert_eq!(entry_type("string", "Hero01"), "str:6");
        assert_eq!(entry_type("string16", "Hé"), "str16:2");
        assert_eq!(entry_type("f32", "1.5"), "f32");
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(thousands(999), "999");
    }
}
