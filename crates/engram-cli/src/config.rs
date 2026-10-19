//! `engram config`: show and change settings (`engram_proto::config`).

use crate::term::{bold, dim, tilde};
use engram_proto::client::Client;
use engram_proto::config::{Config, KEYS};
use engram_proto::paths;
use serde_json::json;
use std::process::ExitCode;

pub fn run(args: &[String]) -> ExitCode {
    let arg = |i: usize| args.get(i).map(String::as_str);
    match (arg(0), arg(1)) {
        (None | Some("list" | "show"), _) => list(),
        (Some("path"), _) => {
            println!("{}", paths::config_file().display());
            ExitCode::SUCCESS
        }
        (Some("get"), Some(key)) => match Config::load().get(key) {
            Some(v) => {
                println!("{v}");
                ExitCode::SUCCESS
            }
            None => fail(&format!("unknown setting '{key}'")),
        },
        (Some("set"), Some(key)) => {
            let value = args[2..].join(" ");
            let mut c = Config::load();
            if let Err(e) = c.set(key, &value) {
                return fail(&e);
            }
            save(&c, &format!("{key} = {}", c.get(key).unwrap_or_default()))
        }
        (Some("reset"), key) => {
            let mut c = Config::load();
            let msg = match key {
                Some(k) => match c.reset(k) {
                    Ok(()) => format!("{k} reset to {}", c.get(k).unwrap_or_default()),
                    Err(e) => return fail(&e),
                },
                None => {
                    c = Config::default();
                    "all settings reset to defaults".to_string()
                }
            };
            save(&c, &msg)
        }
        _ => {
            eprintln!("usage: engram config [list | get <key> | set <key> <value> | reset [key] | path]");
            ExitCode::from(2)
        }
    }
}

fn list() -> ExitCode {
    let c = Config::load();
    let d = Config::default();
    println!("{}  {}\n", bold("Engram settings"), dim(&tilde(&paths::config_file())));
    for k in KEYS {
        let v = c.get(k.name).unwrap_or_default();
        let shown = if v.is_empty() { dim("(auto)") } else { v.clone() };
        let changed = if Some(&v) != d.get(k.name).as_ref() { " *" } else { "" };
        println!("  {:<20} {shown}{changed}", k.name);
        println!("  {:<20} {}", "", dim(&format!("{} — {}", k.description, k.accepts)));
    }
    println!("\n{}", dim("Change with: engram config set <key> <value>   (* = changed from default)"));
    ExitCode::SUCCESS
}

fn save(c: &Config, msg: &str) -> ExitCode {
    if let Err(e) = c.save() {
        return fail(&format!("cannot write {}: {e}", paths::config_file().display()));
    }
    // A running engine picks up timeouts and the freeze interval right away.
    if let Ok(mut client) = Client::connect("cli") {
        let _ = client.call("reload_config", json!({}));
    }
    println!("{msg}");
    ExitCode::SUCCESS
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("engram config: {msg}");
    ExitCode::from(2)
}
