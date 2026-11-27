use std::process::{Command, ExitCode};
use std::fs;

use clap::{Parser, Subcommand};
use serde::Serialize;

use aoc_gmenu::backend::{
    detect_aoc, get_feature, get_tpv_support, set_feature, set_raw_dump, BackendError, I2cBackend,
};
use aoc_gmenu::config::{crosshair_pidfile, load_config, save_config, AppConfig};
use aoc_gmenu::ddc::INTER_FEATURE_DELAY_MS;
use aoc_gmenu::features::{find_feature, format_value, parse_value, FEATURES};
use aoc_gmenu::logutil;
use aoc_gmenu::profile::{apply_profile, get_profile, list_profiles};
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(name = "aoc-gmenu", about = "CLI DDC/CI para monitores AOC (G-Menu)")]
struct Cli {
    /// Número do barramento i2c (ex.: 11 → /dev/i2c-11)
    #[arg(long, global = true)]
    bus: Option<u32>,

    /// Saída JSON
    #[arg(long, global = true)]
    json: bool,

    /// Imprime frames escritos e bytes lidos em hex (stderr)
    #[arg(long, global = true)]
    raw: bool,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Localiza o barramento DDC do monitor AOC via DRM/EDID (+ probe Get VCP)
    Detect,
    /// Lista features mapeadas e tenta ler valores atuais
    List {
        /// Mostra também features sem resposta válida / não suportadas
        #[arg(long)]
        all: bool,
    },
    /// Lê uma feature
    Get { feature: String },
    /// Escreve uma feature (exige --yes; escreve no monitor)
    Set {
        feature: String,
        value: String,
        /// Confirma escrita no monitor
        #[arg(long)]
        yes: bool,
    },
    /// Consulta bitmask de suporte TPV (Get F8/E1 com ext 0x00)
    Support,
    /// Perfis em config.toml
    Profile {
        #[command(subcommand)]
        action: ProfileCmd,
    },
    /// Controla a mira de software (`aoc-crosshair`)
    Crosshair {
        #[command(subcommand)]
        action: CrosshairCmd,
    },
}

#[derive(Subcommand, Debug)]
enum ProfileCmd {
    List,
    Show { name: String },
    /// Aplica perfil (exige --yes; escreve no monitor)
    Apply {
        name: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand, Debug)]
enum CrosshairCmd {
    /// Alterna mira on/off (atualiza config + start/stop do processo)
    Toggle,
    On,
    Off,
    Status,
}

#[derive(Serialize)]
struct DetectJson {
    connector: String,
    bus: u32,
    manufacturer: String,
    model: Option<String>,
}

#[derive(Serialize)]
struct FeatureRow {
    id: String,
    label: String,
    access: String,
    value: Option<String>,
    raw: Option<u16>,
    max: Option<u16>,
    error: Option<String>,
}

fn access_desc(a: aoc_gmenu::features::Access) -> String {
    match a {
        aoc_gmenu::features::Access::Standard { vcp } => format!("VCP 0x{vcp:02X}"),
        aoc_gmenu::features::Access::Tpv { app, acode, ext } => {
            format!("TPV 0x{app:02X}/0x{acode:02X}/0x{ext:02X}")
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    set_raw_dump(cli.raw);
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.cmd {
        Commands::Detect => cmd_detect(cli.bus, cli.json),
        Commands::List { all } => cmd_list(cli.bus, cli.json, all),
        Commands::Get { feature } => cmd_get(cli.bus, cli.json, &feature),
        Commands::Set {
            feature,
            value,
            yes,
        } => cmd_set(cli.bus, cli.json, &feature, &value, yes),
        Commands::Support => cmd_support(cli.bus, cli.json),
        Commands::Profile { action } => cmd_profile(cli.bus, cli.json, action),
        Commands::Crosshair { action } => cmd_crosshair(action, cli.json),
    }
}

fn resolve_bus(prefer: Option<u32>) -> Result<(u32, aoc_gmenu::backend::DetectedMonitor), String> {
    let mon = detect_aoc(prefer).map_err(|e| e.to_string())?;
    Ok((mon.bus, mon))
}

fn cmd_detect(bus: Option<u32>, json: bool) -> Result<(), String> {
    let mon = detect_aoc(bus).map_err(|e| e.to_string())?;
    if json {
        let j = DetectJson {
            connector: mon.connector,
            bus: mon.bus,
            manufacturer: mon.edid.manufacturer,
            model: mon.edid.model_name,
        };
        println!("{}", serde_json::to_string_pretty(&j).unwrap());
    } else {
        println!(
            "connector={} bus={} (/dev/i2c-{}) manufacturer={} model={}",
            mon.connector,
            mon.bus,
            mon.bus,
            mon.edid.manufacturer,
            mon.edid.model_name.as_deref().unwrap_or("?")
        );
    }
    Ok(())
}

fn cmd_list(bus: Option<u32>, json: bool, show_all: bool) -> Result<(), String> {
    let (bus, mon) = resolve_bus(bus)?;
    if !json {
        eprintln!(
            "# {} bus={} model={}",
            mon.connector,
            bus,
            mon.edid.model_name.as_deref().unwrap_or("?")
        );
    }
    let mut rows = Vec::new();
    for f in FEATURES {
        let mut row = FeatureRow {
            id: f.id.into(),
            label: f.label.into(),
            access: access_desc(f.access),
            value: None,
            raw: None,
            max: None,
            error: None,
        };
        match get_feature(bus, f.access) {
            Ok(r) => {
                row.raw = Some(r.current);
                row.max = Some(r.maximum);
                row.value = Some(format_value(f, r.current));
            }
            Err(BackendError::Unsupported) => {
                row.error = Some("não suportado".into());
            }
            Err(e) => {
                logutil::tech(format!("list {}: {e}", f.id));
                row.error = Some(logutil::short_cli_reason(&e.to_string()));
            }
        }
        if show_all || row.value.is_some() {
            rows.push(row);
        }
        thread::sleep(Duration::from_millis(INTER_FEATURE_DELAY_MS));
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
    } else {
        for r in &rows {
            match (&r.value, &r.error) {
                (Some(v), _) => println!(
                    "{:<22} {:>16}  max={:<5}  {}",
                    r.id,
                    v,
                    r.max.unwrap_or(0),
                    r.access
                ),
                (_, Some(e)) => println!("{:<22}  ERR  {}  ({})", r.id, e, r.access),
                _ => {}
            }
        }
    }
    Ok(())
}

fn cmd_get(bus: Option<u32>, json: bool, feature: &str) -> Result<(), String> {
    let f = find_feature(feature).ok_or_else(|| format!("feature desconhecida: {feature}"))?;
    let (bus, _) = resolve_bus(bus)?;
    match get_feature(bus, f.access) {
        Ok(r) => {
            let formatted = format_value(f, r.current);
            if json {
                let obj = serde_json::json!({
                    "id": f.id,
                    "label": f.label,
                    "access": access_desc(f.access),
                    "raw": r.current,
                    "max": r.maximum,
                    "value": formatted,
                });
                println!("{}", serde_json::to_string_pretty(&obj).unwrap());
            } else {
                println!(
                    "{}={} (raw={}, max={})",
                    f.id, formatted, r.current, r.maximum
                );
            }
            Ok(())
        }
        Err(BackendError::Unsupported) => Err(format!("{}: não suportado", f.id)),
        Err(e) => {
            logutil::tech(format!("get {}: {e}", f.id));
            Err(format!("{}: {}", f.id, logutil::short_cli_reason(&e.to_string())))
        }
    }
}

fn cmd_set(
    bus: Option<u32>,
    json: bool,
    feature: &str,
    value: &str,
    yes: bool,
) -> Result<(), String> {
    let f = find_feature(feature).ok_or_else(|| format!("feature desconhecida: {feature}"))?;
    if !f.writable {
        return Err(format!("{} não é gravável", f.id));
    }
    let code = parse_value(f, value)?;
    if !yes {
        return Err(
            "recusado: set escreve no monitor; passe --yes para confirmar".into(),
        );
    }
    let (bus, _) = resolve_bus(bus)?;
    set_feature(bus, f.access, code).map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": f.id,
                "written": code,
                "ok": true
            }))
            .unwrap()
        );
    } else {
        println!("ok: {} ← {} ({})", f.id, format_value(f, code), code);
    }
    Ok(())
}

fn cmd_support(bus: Option<u32>, json: bool) -> Result<(), String> {
    let (bus, mon) = resolve_bus(bus)?;
    let (bits, flags) = get_tpv_support(bus).map_err(|e| match e {
        BackendError::Unsupported => "não suportado".into(),
        other => other.to_string(),
    })?;
    if json {
        let names: Vec<_> = flags.iter().map(|(n, _)| *n).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "connector": mon.connector,
                "bus": bus,
                "bitmask": format!("0x{bits:04X}"),
                "bitmask_u16": bits,
                "flags": names,
            }))
            .unwrap()
        );
    } else {
        println!(
            "# {} bus={} model={}  TPV support bitmask=0x{bits:04X}",
            mon.connector,
            bus,
            mon.edid.model_name.as_deref().unwrap_or("?"),
        );
        for (name, mask) in flags {
            println!("  {name} (0x{mask:04X})");
        }
    }
    Ok(())
}

fn cmd_profile(bus: Option<u32>, json: bool, action: ProfileCmd) -> Result<(), String> {
    let cfg = load_config().unwrap_or_else(|_| AppConfig::default());
    match action {
        ProfileCmd::List => {
            let list = list_profiles(&cfg);
            if json {
                println!("{}", serde_json::to_string_pretty(&list).unwrap());
            } else {
                for (name, desc) in list {
                    println!("{name}: {desc}");
                }
            }
            Ok(())
        }
        ProfileCmd::Show { name } => {
            let p = get_profile(&cfg, &name).map_err(|e| e.to_string())?;
            if json {
                println!("{}", serde_json::to_string_pretty(p).unwrap());
            } else {
                println!("# perfil {name}: {}", p.description);
                for (k, v) in &p.features {
                    println!("{k} = {v}");
                }
                if let Some(c) = p.crosshair_enabled {
                    println!("crosshair_enabled = {c}");
                }
            }
            Ok(())
        }
        ProfileCmd::Apply { name, yes } => {
            if !yes {
                return Err(
                    "recusado: profile apply escreve no monitor; passe --yes para confirmar"
                        .into(),
                );
            }
            let p = get_profile(&cfg, &name).map_err(|e| e.to_string())?.clone();
            let (bus_n, _) = resolve_bus(bus.or(cfg.preferred_bus))?;
            let mut backend = I2cBackend::new(bus_n);
            let applied = apply_profile(&mut backend, &p).map_err(|e| e.to_string())?;
            // Mira de software (config only; start/stop)
            if let Some(en) = p.crosshair_enabled {
                let mut cfg2 = cfg;
                cfg2.crosshair.enabled = en;
                let _ = save_config(&cfg2);
                if en {
                    let _ = start_crosshair_process();
                } else {
                    let _ = stop_crosshair_process();
                }
            }
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "profile": name,
                        "applied": applied,
                        "ok": true
                    }))
                    .unwrap()
                );
            } else {
                println!("ok: perfil {name} aplicado ({} features)", applied.len());
                for (id, code) in applied {
                    println!("  {id} ← {code}");
                }
            }
            Ok(())
        }
    }
}

fn find_crosshair_bin() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("AOC_CROSSHAIR_BIN") {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join("aoc-crosshair");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    which_bin("aoc-crosshair")
}

fn which_bin(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

fn read_pidfile() -> Option<u32> {
    let path = crosshair_pidfile();
    let text = fs::read_to_string(path).ok()?;
    text.trim().parse().ok()
}

fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

use std::path::Path;

fn start_crosshair_process() -> Result<(), String> {
    if let Some(pid) = read_pidfile() {
        if process_alive(pid) {
            return Ok(());
        }
    }
    let bin = find_crosshair_bin().ok_or_else(|| {
        String::from(
            "aoc-crosshair não encontrado (compile o workspace ou defina AOC_CROSSHAIR_BIN)",
        )
    })?;
    let child = Command::new(bin)
        .spawn()
        .map_err(|e| format!("falha ao iniciar aoc-crosshair: {e}"))?;
    let pid = child.id();
    let path = crosshair_pidfile();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&path, format!("{pid}\n")).map_err(|e| e.to_string())?;
    // detach: drop child without wait — process keeps running
    std::mem::forget(child);
    Ok(())
}

fn stop_crosshair_process() -> Result<(), String> {
    if let Some(pid) = read_pidfile() {
        if process_alive(pid) {
            let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).status();
        }
    }
    let _ = fs::remove_file(crosshair_pidfile());
    Ok(())
}

fn cmd_crosshair(action: CrosshairCmd, json: bool) -> Result<(), String> {
    let mut cfg = load_config().unwrap_or_else(|_| AppConfig::default());
    match action {
        CrosshairCmd::Status => {
            let pid = read_pidfile();
            let alive = pid.map(process_alive).unwrap_or(false);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "enabled": cfg.crosshair.enabled,
                        "pid": pid,
                        "running": alive,
                        "output": cfg.crosshair.output,
                    }))
                    .unwrap()
                );
            } else {
                println!(
                    "enabled={} running={} pid={} output={}",
                    cfg.crosshair.enabled,
                    alive,
                    pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
                    cfg.crosshair.output
                );
            }
            Ok(())
        }
        CrosshairCmd::On => {
            cfg.crosshair.enabled = true;
            save_config(&cfg)?;
            start_crosshair_process()?;
            println!("mira: on");
            Ok(())
        }
        CrosshairCmd::Off => {
            cfg.crosshair.enabled = false;
            save_config(&cfg)?;
            stop_crosshair_process()?;
            println!("mira: off");
            Ok(())
        }
        CrosshairCmd::Toggle => {
            if cfg.crosshair.enabled {
                cfg.crosshair.enabled = false;
                save_config(&cfg)?;
                stop_crosshair_process()?;
                println!("mira: off");
            } else {
                cfg.crosshair.enabled = true;
                save_config(&cfg)?;
                start_crosshair_process()?;
                println!("mira: on");
            }
            Ok(())
        }
    }
}

