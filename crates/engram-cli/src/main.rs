//! `engram` — command line for humans and AI agents, plus the MCP server.
//!
//! `main` only routes: tools go to the daemon (`call_tool`), everything else is
//! a module (help, config, setup/doctor, uninstall, mcp).

mod config;
mod help;
mod mcp;
mod setup;
mod term;
mod uninstall;

use engram_proto::client::Client;
use engram_proto::tools;
use engram_proto::{detect_client, paths, RpcError};
use serde_json::{json, Map, Value};
use std::io::IsTerminal;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut pretty = std::io::stdout().is_terminal();
    // Our own flags end at `--`: after it come the child's arguments (`engram launch -- game --compact`).
    let own = args.iter().position(|a| a == "--").unwrap_or(args.len());
    let mut i = 0;
    args.retain(|a| {
        let ours = i < own;
        i += 1;
        match (ours, a.as_str()) {
            (true, "--pretty") => {
                pretty = true;
                false
            }
            (true, "--compact") => {
                pretty = false;
                false
            }
            _ => true,
        }
    });
    let Some(cmd) = args.first().cloned() else {
        print!("{}", help::USAGE);
        return ExitCode::SUCCESS;
    };
    let rest = &args[1..];
    if matches!(cmd.as_str(), "-h" | "--help" | "help" | "tools" | "config" | "doctor" | "version" | "-V" | "--version") {
        // Text output for people: `engram tools | head` should end quietly, not panic.
        // Commands that talk to the daemon for long keep Rust's EPIPE handling.
        // SAFETY: restoring the default SIGPIPE action before any output.
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    }
    match cmd.as_str() {
        "-h" | "--help" => {
            print!("{}", help::USAGE);
            ExitCode::SUCCESS
        }
        "-V" | "--version" | "version" => {
            println!("engram {}", engram_proto::VERSION);
            ExitCode::SUCCESS
        }
        "help" => help::tool(rest.first().map(String::as_str)),
        "tools" => help::list(rest.iter().any(|a| a == "--markdown")),
        "config" => config::run(rest),
        "uninstall" => uninstall::run(rest),
        "mcp" => mcp::run(),
        "doctor" => setup::doctor(),
        "setup" => setup::setup(),
        "agent-init" | "init" => setup::agent_init(rest.first().map(String::as_str)),
        "launch" => launch(rest),
        "events" => events(),
        "daemon" => daemon(rest.first().map(String::as_str).unwrap_or("status"), pretty),
        tool => call_tool(tool, rest, pretty),
    }
}

fn print_json(v: &Value, pretty: bool) {
    let s = if pretty { serde_json::to_string_pretty(v) } else { serde_json::to_string(v) };
    println!("{}", s.unwrap_or_default());
}

fn fail(e: &RpcError, pretty: bool) -> ExitCode {
    print_json(&json!({ "error": e }), pretty);
    ExitCode::from(1)
}

/// True when JSON would print the number exactly as typed (`37`, not `1.000` or `1e3`).
fn is_canonical(n: &Value, typed: &str) -> bool {
    let printed = n.to_string();
    printed == typed
}

/// `key=value` pairs → JSON object. Numbers and booleans are typed; everything
/// else stays a string (so `address=[[Game.exe+10]+18]` is not parsed as JSON).
pub fn parse_args(rest: &[String]) -> Result<Value, String> {
    if rest.len() == 1 && rest[0].trim_start().starts_with('{') {
        return serde_json::from_str(&rest[0]).map_err(|e| format!("invalid JSON arguments: {e}"));
    }
    let mut m = Map::new();
    for a in rest {
        let (k, v) = a.split_once('=').ok_or_else(|| format!("expected key=value, got '{a}'"))?;
        let parsed = match serde_json::from_str::<Value>(v) {
            // A number keeps its text unless JSON would rewrite it: `1.000` is a scan literal with
            // three decimals (it sets the tolerance), not the float 1.0.
            Ok(x @ Value::Number(_)) if is_canonical(&x, v) => x,
            Ok(x @ Value::Bool(_)) => x,
            Ok(x @ (Value::Array(_) | Value::Object(_))) if k == "argv" || k == "env" => x,
            _ => Value::String(v.to_string()),
        };
        m.insert(k.to_string(), parsed);
    }
    Ok(Value::Object(m))
}

fn call_tool(tool: &str, rest: &[String], pretty: bool) -> ExitCode {
    let Some(def) = tools::find(tool) else {
        eprintln!("unknown command or tool '{tool}'. Run `engram tools` or `engram --help`.");
        return ExitCode::from(2);
    };
    let params = match parse_args(rest) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let mut c = match Client::connect_or_spawn(&detect_client()) {
        Ok(c) => c,
        Err(e) => return fail(&e, pretty),
    };
    match c.call(def.name, params) {
        Ok(v) => {
            print_json(&v, pretty);
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e, pretty),
    }
}

fn launch(rest: &[String]) -> ExitCode {
    let argv: Vec<String> = match rest.iter().position(|a| a == "--") {
        Some(i) => rest[i + 1..].to_vec(),
        None => rest.to_vec(),
    };
    if argv.is_empty() {
        eprintln!("usage: engram launch -- <command> [args...]");
        return ExitCode::from(2);
    }
    let env: Map<String, Value> = std::env::vars().map(|(k, v)| (k, Value::String(v))).collect();
    let cwd = std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default();
    let mut c = match Client::connect_or_spawn("launcher") {
        Ok(c) => c,
        Err(e) => return fail(&e, false),
    };
    let pid = match c.call("launch", json!({ "argv": argv, "cwd": cwd, "env": env })) {
        Ok(v) => v["pid"].as_i64().unwrap_or(0),
        Err(e) => return fail(&e, false),
    };
    eprintln!("engram: launched pid {pid} as a child of engramd; attach to it (or its game process) from Engram");
    // Stay alive as long as the program runs, so Steam and shells see the real lifetime.
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        match c.call("launch_status", json!({ "pid": pid })) {
            Ok(v) if v["running"] == json!(false) => return ExitCode::from(v["exit_code"].as_i64().unwrap_or(0).clamp(0, 255) as u8),
            Ok(_) => {}
            Err(_) => match Client::connect("launcher") {
                Ok(n) => c = n,
                Err(_) => return ExitCode::from(1),
            },
        }
    }
}

fn events() -> ExitCode {
    let c = match Client::connect_or_spawn(&detect_client()) {
        Ok(c) => c,
        Err(e) => return fail(&e, false),
    };
    match c.subscribe(false) {
        Ok(stream) => {
            for e in stream {
                println!("{}", serde_json::to_string(&e).unwrap_or_default());
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e, false),
    }
}

fn daemon(action: &str, pretty: bool) -> ExitCode {
    match action {
        "start" => match Client::connect_or_spawn("cli") {
            Ok(mut c) => {
                print_json(&c.call("hello", json!({})).unwrap_or(Value::Null), pretty);
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e, pretty),
        },
        "stop" => {
            match Client::connect("cli") {
                Ok(mut c) => {
                    let _ = c.call("shutdown", json!({}));
                    println!("stopped");
                }
                Err(_) => println!("not running"),
            }
            ExitCode::SUCCESS
        }
        _ => match Client::connect("cli") {
            Ok(mut c) => {
                print_json(&c.call("hello", json!({})).unwrap_or(Value::Null), pretty);
                ExitCode::SUCCESS
            }
            Err(_) => {
                println!("not running (socket {})", paths::socket().display());
                ExitCode::from(1)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_args() {
        let v = parse_args(&["value=37".into(), "address=[[Game.exe+10]+18]".into(), "aligned=false".into(), "type=f32".into()]).unwrap();
        assert_eq!(v["value"], json!(37));
        assert_eq!(v["address"], json!("[[Game.exe+10]+18]"));
        assert_eq!(v["aligned"], json!(false));
        assert_eq!(v["type"], json!("f32"));
        let j = parse_args(&[r#"{"pid":5}"#.into()]).unwrap();
        assert_eq!(j["pid"], json!(5));
    }
}
