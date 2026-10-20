//! engram-gui — the Engram window.
//!
//! A client of engramd like the CLI and agents, but trusted as the user: its
//! writes are not gated, and it answers agents' approval requests. All engine
//! calls run on worker threads; replies and events are applied on the UI thread
//! (see `view`).

mod ctx;
mod engine;
mod events;
mod format;
mod i18n;
mod inspect;
mod launcher;
mod poll;
mod settings;
mod snapshot;
mod view;
mod wire;

slint::include_modules!();

use ctx::Ctx;
use engine::Worker;
use engram_proto::config::Config;
use slint::ComponentHandle;
use view::Level;

/// The system monospace family; Qt does not resolve the generic "monospace".
fn monospace_family() -> String {
    std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}", "monospace"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .filter(|f| !f.trim().is_empty())
        .unwrap_or_else(|| "monospace".into())
}

fn main() -> Result<(), slint::PlatformError> {
    let w = MainWindow::new()?;
    let config = Config::load();
    i18n::apply(&config.language);
    view::init(&w, config.result_rows);
    let agents: Vec<slint::SharedString> = launcher::available_agents().into_iter().map(Into::into).collect();
    w.global::<App>().set_agents(slint::ModelRc::new(slint::VecModel::from(agents)));
    settings::init(&w, &config);
    w.global::<Tone>().set_mono(monospace_family().into());
    view::message(&w, Level::Info, i18n::t("Starting the engine…"));

    let ctx = Ctx { ui: w.as_weak(), quick: Worker::spawn("quick"), ops: Worker::spawn("ops") };
    wire::wire(&w, &ctx);

    let (c1, c2) = (ctx.clone(), ctx.clone());
    engine::listen(
        move |r| {
            let c = c1.clone();
            let _ = c1.ui.upgrade_in_event_loop(move |w| c.on_connection(&w, r));
        },
        move |e| {
            let c = c2.clone();
            let _ = c2.ui.upgrade_in_event_loop(move |w| c.on_event(&w, e));
        },
    );
    poll::start(&ctx, config.refresh_ms);
    let _snap = snapshot::start(&w);

    w.run()
}
