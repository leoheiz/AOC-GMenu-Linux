//! Tray StatusNotifierItem (ksni) — processo separado (não precisa de janela egui).
//! Justificativa: no Plasma Wayland o SNI funciona sem surface; manter o tray
//! fora da GUI evita reabrir eframe só para o ícone.

use std::process::Command;
use std::sync::{Arc, Mutex};

use aoc_gmenu::backend::{detect_aoc, I2cBackend};
use aoc_gmenu::config::{crosshair_pidfile, load_config, save_config, AppConfig};
use aoc_gmenu::profile::{apply_profile, get_profile};
use ksni::menu::*;

struct Shared {
    cfg: AppConfig,
}

struct GMenuTray {
    shared: Arc<Mutex<Shared>>,
}

impl GMenuTray {
    fn apply_profile(&self, name: &str) {
        let mut shared = self.shared.lock().unwrap();
        let Ok(p) = get_profile(&shared.cfg, name).cloned() else {
            eprintln!("tray: perfil desconhecido {name}");
            return;
        };
        match detect_aoc(shared.cfg.preferred_bus) {
            Ok(mon) => {
                let mut backend = I2cBackend::new(mon.bus);
                if let Err(e) = apply_profile(&mut backend, &p) {
                    eprintln!("tray: apply {name}: {e}");
                } else {
                    eprintln!("tray: perfil {name} aplicado");
                    if let Some(en) = p.crosshair_enabled {
                        shared.cfg.crosshair.enabled = en;
                        let _ = save_config(&shared.cfg);
                        if en {
                            start_crosshair();
                        } else {
                            stop_crosshair();
                        }
                    }
                }
            }
            Err(e) => eprintln!("tray: detect: {e}"),
        }
    }

    fn toggle_mira(&self) {
        let mut shared = self.shared.lock().unwrap();
        if shared.cfg.crosshair.enabled {
            shared.cfg.crosshair.enabled = false;
            let _ = save_config(&shared.cfg);
            stop_crosshair();
        } else {
            shared.cfg.crosshair.enabled = true;
            let _ = save_config(&shared.cfg);
            start_crosshair();
        }
    }

    fn reload_cfg(&self) {
        let mut shared = self.shared.lock().unwrap();
        if let Ok(cfg) = load_config() {
            shared.cfg = cfg;
            eprintln!("tray: config recarregada");
        }
    }
}

fn find_bin(name: &str) -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join(name);
            if cand.exists() {
                return cand;
            }
        }
    }
    name.into()
}

fn start_crosshair() {
    stop_crosshair();
    match Command::new(find_bin("aoc-crosshair")).spawn() {
        Ok(child) => {
            let pid = child.id();
            let _ = std::fs::write(crosshair_pidfile(), format!("{pid}\n"));
            std::mem::forget(child);
        }
        Err(e) => eprintln!("tray: aoc-crosshair: {e}"),
    }
}

fn stop_crosshair() {
    if let Ok(pid_str) = std::fs::read_to_string(crosshair_pidfile()) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }
    }
    let _ = std::fs::remove_file(crosshair_pidfile());
}

fn open_gmenu() {
    let _ = Command::new(find_bin("aoc-gmenu-gui")).spawn();
}

impl ksni::Tray for GMenuTray {
    fn id(&self) -> String {
        "aoc-gmenu".into()
    }

    fn title(&self) -> String {
        "AOC G-Menu".into()
    }

    fn icon_name(&self) -> String {
        "video-display".into()
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Perfil Jogo".into(),
                activate: Box::new(|this: &mut Self| this.apply_profile("jogo")),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Perfil Leitura".into(),
                activate: Box::new(|this: &mut Self| this.apply_profile("leitura")),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Perfil Original".into(),
                activate: Box::new(|this: &mut Self| this.apply_profile("original")),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Mira on/off".into(),
                activate: Box::new(|this: &mut Self| this.toggle_mira()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Abrir G-Menu".into(),
                activate: Box::new(|_this: &mut Self| open_gmenu()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Recarregar".into(),
                activate: Box::new(|this: &mut Self| this.reload_cfg()),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Sair".into(),
                activate: Box::new(|_this: &mut Self| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn main() {
    let cfg = load_config().unwrap_or_default();
    let tray = GMenuTray {
        shared: Arc::new(Mutex::new(Shared { cfg })),
    };
    eprintln!("aoc-gmenu-tray: rodando (StatusNotifierItem)");
    if let Err(e) = ksni::TrayService::new(tray).run() {
        eprintln!("aoc-gmenu-tray: {e}");
        std::process::exit(1);
    }
}
