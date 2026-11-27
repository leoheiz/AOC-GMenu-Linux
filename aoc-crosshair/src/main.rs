//! Mira de software — overlay Wayland (wlr-layer-shell).

mod wayland_overlay;

use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use aoc_gmenu::config::{config_path, crosshair_pidfile, load_config};

fn write_pidfile() -> Result<(), String> {
    let path = crosshair_pidfile();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, format!("{}\n", std::process::id())).map_err(|e| e.to_string())
}

fn clear_pidfile() {
    let _ = fs::remove_file(crosshair_pidfile());
}

fn main() {
    let exit = Arc::new(AtomicBool::new(false));
    let reload = Arc::new(AtomicBool::new(false));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&exit));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&exit));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGUSR1, Arc::clone(&reload));

    if let Err(e) = write_pidfile() {
        eprintln!("aoc-crosshair: pidfile: {e}");
    }

    let path = config_path();
    let cfg = load_config().unwrap_or_default();
    if !cfg.crosshair.enabled {
        eprintln!("aoc-crosshair: crosshair.enabled=false no config; rodando mesmo assim");
    }

    let result = wayland_overlay::OverlayApp::run(cfg.crosshair, path, exit, reload);
    clear_pidfile();
    if let Err(e) = result {
        eprintln!("aoc-crosshair: {e}");
        std::process::exit(1);
    }
}
