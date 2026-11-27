//! Configuração em `$XDG_CONFIG_HOME/aoc-gmenu/config.toml` (defaults se ausente).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CrosshairShape {
    #[default]
    Cross,
    X,
    Dot,
    Circle,
    CrossDot,
}

impl CrosshairShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cross => "cross",
            Self::X => "x",
            Self::Dot => "dot",
            Self::Circle => "circle",
            Self::CrossDot => "cross_dot",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cross" | "cruz" | "+" => Some(Self::Cross),
            "x" => Some(Self::X),
            "dot" | "ponto" | "bolinha" => Some(Self::Dot),
            "circle" | "circulo" | "círculo" => Some(Self::Circle),
            "cross_dot" | "cruz_ponto" => Some(Self::CrossDot),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrosshairConfig {
    pub enabled: bool,
    /// Nome do output Wayland/DRM (ex.: `DP-1`).
    pub output: String,
    pub shape: CrosshairShape,
    pub size: u32,
    pub thickness: u32,
    /// Cor RGBA hex `#RRGGBB` ou `#RRGGBBAA`.
    pub color: String,
    pub opacity: f32,
    pub gap: u32,
    pub outline: bool,
    pub outline_color: String,
    pub outline_thickness: u32,
}

impl Default for CrosshairConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: "DP-1".into(),
            shape: CrosshairShape::Cross,
            size: 24,
            thickness: 2,
            color: "#FF0000FF".into(),
            opacity: 1.0,
            gap: 4,
            outline: true,
            outline_color: "#000000FF".into(),
            outline_thickness: 1,
        }
    }
}

/// Valores de um perfil (feature id → valor textual, como na CLI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Profile {
    pub description: String,
    #[serde(default)]
    pub features: BTreeMap<String, String>,
    /// Liga/desliga a mira de software (não é Dial Point do OSD).
    #[serde(default)]
    pub crosshair_enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    pub crosshair: CrosshairConfig,
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default = "default_bus")]
    pub preferred_bus: Option<u32>,
}

fn default_bus() -> Option<u32> {
    None
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            crosshair: CrosshairConfig::default(),
            profiles: default_profiles(),
            preferred_bus: None,
        }
    }
}

/// Perfis sugeridos (jogo / leitura / original capturado no U34G4C).
pub fn default_profiles() -> BTreeMap<String, Profile> {
    let mut m = BTreeMap::new();
    m.insert(
        "jogo".into(),
        Profile {
            description: "FPS / competitivos".into(),
            features: BTreeMap::from([
                ("brightness".into(), "90".into()),
                ("contrast".into(), "60".into()),
                ("overdrive".into(), "fast".into()),
                ("low_input_lag".into(), "on".into()),
                ("shadow_control".into(), "10".into()),
                ("game_color".into(), "12".into()),
                ("low_blue_mode".into(), "off".into()),
            ]),
            crosshair_enabled: Some(true),
        },
    );
    m.insert(
        "leitura".into(),
        Profile {
            description: "Leitura / conforto".into(),
            features: BTreeMap::from([
                ("brightness".into(), "40".into()),
                ("contrast".into(), "50".into()),
                // office = nível 3 no F8/E1; “level 2” do brief ≈ internet(2) —
                // usamos office por ser o modo Office/Reading do OSD.
                ("low_blue_mode".into(), "office".into()),
                ("overdrive".into(), "off".into()),
                ("shadow_control".into(), "0".into()),
                ("game_color".into(), "10".into()),
            ]),
            crosshair_enabled: Some(false),
        },
    );
    m.insert(
        "original".into(),
        Profile {
            description: "Valores capturados no U34G4C (baseline)".into(),
            features: BTreeMap::from([
                ("brightness".into(), "80".into()),
                ("contrast".into(), "60".into()),
                ("color_temp".into(), "warm".into()),
                ("gamma".into(), "gamma1".into()),
                ("dcr".into(), "off".into()),
                ("eco_mode".into(), "standard".into()),
                ("overdrive".into(), "fast".into()),
                ("low_input_lag".into(), "on".into()),
                ("game_color".into(), "10".into()),
                ("shadow_control".into(), "0".into()),
                ("frame_counter".into(), "off".into()),
                ("dial_point".into(), "off".into()),
                ("low_blue_mode".into(), "off".into()),
                ("input_source".into(), "dp1".into()),
                ("volume".into(), "50".into()),
            ]),
            crosshair_enabled: Some(false),
        },
    );
    m
}

pub fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("aoc-gmenu");
        }
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(".").join(".config"))
        .join("aoc-gmenu")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn load_config() -> Result<AppConfig, String> {
    load_config_from(&config_path())
}

pub fn load_config_from(path: &Path) -> Result<AppConfig, String> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let cfg: AppConfig = toml::from_str(&text).map_err(|e| e.to_string())?;
    Ok(cfg)
}

pub fn save_config(cfg: &AppConfig) -> Result<(), String> {
    save_config_to(&config_path(), cfg)
}

pub fn save_config_to(path: &Path, cfg: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

/// Pidfile da mira em `$XDG_RUNTIME_DIR/aoc-crosshair.pid`.
pub fn crosshair_pidfile() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    runtime.join("aoc-crosshair.pid")
}

pub fn parse_rgba_hex(s: &str) -> Option<(u8, u8, u8, u8)> {
    let t = s.trim().trim_start_matches('#');
    let (r, g, b, a) = match t.len() {
        6 => {
            let r = u8::from_str_radix(&t[0..2], 16).ok()?;
            let g = u8::from_str_radix(&t[2..4], 16).ok()?;
            let b = u8::from_str_radix(&t[4..6], 16).ok()?;
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&t[0..2], 16).ok()?;
            let g = u8::from_str_radix(&t[2..4], 16).ok()?;
            let b = u8::from_str_radix(&t[4..6], 16).ok()?;
            let a = u8::from_str_radix(&t[6..8], 16).ok()?;
            (r, g, b, a)
        }
        _ => return None,
    };
    Some((r, g, b, a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn defaults_roundtrip_toml() {
        let cfg = AppConfig::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: AppConfig = toml::from_str(&text).unwrap();
        assert_eq!(back.crosshair.shape, CrosshairShape::Cross);
        assert!(back.profiles.contains_key("jogo"));
        assert!(back.profiles.contains_key("leitura"));
        assert!(back.profiles.contains_key("original"));
        assert_eq!(
            back.profiles["jogo"].features.get("brightness").map(String::as_str),
            Some("90")
        );
        assert_eq!(back.profiles["jogo"].crosshair_enabled, Some(true));
        assert_eq!(back.profiles["leitura"].crosshair_enabled, Some(false));
    }

    #[test]
    fn load_missing_returns_defaults() {
        let path = std::env::temp_dir().join(format!(
            "aoc-gmenu-missing-{}.toml",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cfg = load_config_from(&path).unwrap();
        assert_eq!(cfg.crosshair.output, "DP-1");
    }

    #[test]
    fn save_load_roundtrip_file() {
        let dir = std::env::temp_dir().join(format!(
            "aoc-gmenu-cfg-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let mut cfg = AppConfig::default();
        cfg.crosshair.size = 32;
        cfg.crosshair.enabled = true;
        save_config_to(&path, &cfg).unwrap();
        let back = load_config_from(&path).unwrap();
        assert_eq!(back.crosshair.size, 32);
        assert!(back.crosshair.enabled);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_hex_colors() {
        assert_eq!(parse_rgba_hex("#FF0000"), Some((255, 0, 0, 255)));
        assert_eq!(parse_rgba_hex("00FF0080"), Some((0, 255, 0, 128)));
        assert!(parse_rgba_hex("xyz").is_none());
    }

    #[test]
    fn shape_parse() {
        assert_eq!(CrosshairShape::parse("cruz"), Some(CrosshairShape::Cross));
        assert_eq!(CrosshairShape::parse("X"), Some(CrosshairShape::X));
    }
}
