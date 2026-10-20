//! Developer aid: with `ENGRAM_GUI_SNAPSHOT=<dir>` the window writes a PPM image
//! of itself to `<dir>/snap-NNN.ppm` every 1.5 s (for docs screenshots and
//! manual checks without a screen grabber). No effect otherwise.

use crate::MainWindow;
use slint::{ComponentHandle, Timer, TimerMode};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

pub fn start(w: &MainWindow) -> Option<Timer> {
    let dir = PathBuf::from(std::env::var_os("ENGRAM_GUI_SNAPSHOT")?);
    std::fs::create_dir_all(&dir).ok()?;
    let ui = w.as_weak();
    let n = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(1500), move || {
        // The Qt backend renders the snapshot synchronously, which must not
        // happen inside a timer callback: defer it to the next event loop turn.
        let (ui, dir, n) = (ui.clone(), dir.clone(), n.clone());
        let _ = slint::spawn_local(async move {
            let Some(w) = ui.upgrade() else { return };
            let Ok(img) = w.window().take_snapshot() else { return };
            n.set(n.get() + 1);
            let mut out = format!("P6\n{} {}\n255\n", img.width(), img.height()).into_bytes();
            out.extend(img.as_slice().iter().flat_map(|p| [p.r, p.g, p.b]));
            if let Ok(mut f) = std::fs::File::create(dir.join(format!("snap-{:03}.ppm", n.get()))) {
                let _ = f.write_all(&out);
            }
        });
    });
    Some(timer)
}
