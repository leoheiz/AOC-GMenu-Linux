//! Mapeamento data-driven de features G-Menu → VCP / TPV.

use serde::Serialize;

/// Como o valor é endereçado no barramento DDC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// MCCS Get/Set VCP padrão (`0x01` / `0x03` + código).
    Standard { vcp: u8 },
    /// TPV estendido: AppCommand + ACode + ext.
    Tpv {
        app: u8,
        acode: u8,
        ext: u8,
    },
}

impl Access {
    pub const fn std(vcp: u8) -> Self {
        Self::Standard { vcp }
    }

    pub const fn f8e1(ext: u8) -> Self {
        Self::Tpv {
            app: 0xF8,
            acode: 0xE1,
            ext,
        }
    }

    pub const fn e2a0(ext: u8) -> Self {
        Self::Tpv {
            app: 0xE2,
            acode: 0xA0,
            ext,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    /// Inteiro contínuo [min, max] inclusive (max pode ser sobrescrito pelo monitor).
    Range { min: u16, max: u16 },
    /// Nomes simbólicos → códigos.
    Named,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NamedValue {
    pub name: &'static str,
    pub code: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Feature {
    pub id: &'static str,
    pub label: &'static str,
    pub access: Access,
    pub kind: ValueKind,
    pub values: &'static [NamedValue],
    pub writable: bool,
    /// Confirmado legível no U34G4C (mostrar na GUI / list padrão).
    pub confirmed: bool,
}

const fn nv(name: &'static str, code: u16) -> NamedValue {
    NamedValue { name, code }
}

/// U34G4C (Mstar): OSD Off / Fast / Faster / Fastest (max=3).
/// Aliases weak/medium/strong mantidos por compat com o enum G-Menu clássico.
const OVERDRIVE: &[NamedValue] = &[
    nv("off", 0),
    nv("fast", 1),
    nv("faster", 2),
    nv("fastest", 3),
    nv("weak", 1),
    nv("medium", 2),
    nv("strong", 3),
];

const GAME_MODE: &[NamedValue] = &[
    nv("off", 0),
    nv("fps", 11),
    nv("rts", 12),
    nv("racing", 13),
    nv("gamer1", 14),
    nv("gamer2", 15),
    nv("gamer3", 16),
];

const COLOR_TEMP: &[NamedValue] = &[
    nv("srgb", 1),
    nv("warm", 5),
    nv("normal", 6),
    nv("cool", 8),
    nv("user", 11),
    nv("adobe_rgb", 14),
];

const INPUT_SOURCE: &[NamedValue] = &[
    nv("vga", 1),
    nv("dvi", 3),
    nv("dp1", 15),
    nv("dp2", 16),
    nv("hdmi1", 17),
    nv("hdmi2", 18),
    nv("hdmi3", 19),
    nv("usbc1", 21),
    nv("usbc2", 22),
];

const ON_OFF: &[NamedValue] = &[nv("off", 0), nv("on", 1)];

const LOW_BLUE: &[NamedValue] = &[
    nv("off", 0),
    nv("multimedia", 1),
    nv("internet", 2),
    nv("office", 3),
    nv("reading", 4),
];

const LOW_BLUE_E2A0: &[NamedValue] = &[
    nv("off", 0),
    nv("level1", 1),
    nv("level2", 2),
    nv("level3", 3),
    nv("level4", 4),
];

const ECO: &[NamedValue] = &[
    nv("standard", 0),
    nv("text", 1),
    nv("internet", 2),
    nv("game", 3),
    nv("movie", 4),
    nv("sport", 5),
    nv("reading", 6),
    nv("uniformity", 7),
];

const GAMMA_TPV: &[NamedValue] = &[nv("gamma1", 0), nv("gamma2", 1), nv("gamma3", 2)];

const FRAME_COUNTER: &[NamedValue] = &[
    nv("off", 0),
    nv("right_up", 1),
    nv("right_down", 2),
    nv("left_down", 3),
    nv("left_up", 4),
];

const SHADOW_BOOST: &[NamedValue] = &[
    nv("level1", 0),
    nv("level2", 1),
    nv("level3", 2),
    nv("dynamic", 3),
];

const SMART_CROSSHAIR: &[NamedValue] = &[nv("off", 0), nv("on", 1), nv("smart", 2)];

/// Tabela principal usada pelo CLI (família AOC / U34G4C).
pub static FEATURES: &[Feature] = &[
    Feature {
        id: "brightness",
        label: "Brightness",
        access: Access::std(0x10),
        kind: ValueKind::Range { min: 0, max: 100 },
        values: &[],
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "contrast",
        label: "Contrast",
        access: Access::std(0x12),
        kind: ValueKind::Range { min: 0, max: 100 },
        values: &[],
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "color_temp",
        label: "Color Temperature",
        access: Access::std(0x14),
        kind: ValueKind::Named,
        values: COLOR_TEMP,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "input_source",
        label: "Input Source",
        access: Access::std(0x60),
        kind: ValueKind::Named,
        values: INPUT_SOURCE,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "volume",
        label: "Volume",
        access: Access::std(0x62),
        kind: ValueKind::Range { min: 0, max: 100 },
        values: &[],
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "sharpness",
        label: "Sharpness",
        access: Access::std(0x87),
        kind: ValueKind::Range { min: 0, max: 100 },
        values: &[],
        writable: true,
        confirmed: false,
    },
    Feature {
        id: "game_mode",
        label: "Game Mode",
        access: Access::std(0xDC),
        kind: ValueKind::Named,
        values: GAME_MODE,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "eco_mode",
        label: "ECO Mode",
        access: Access::f8e1(17),
        kind: ValueKind::Named,
        values: ECO,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "gamma",
        label: "Gamma (TPV)",
        access: Access::f8e1(18),
        kind: ValueKind::Named,
        values: GAMMA_TPV,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "dcr",
        label: "DCR",
        access: Access::f8e1(19),
        kind: ValueKind::Named,
        values: ON_OFF,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "overdrive",
        label: "Overdrive",
        access: Access::f8e1(81),
        kind: ValueKind::Named,
        values: OVERDRIVE,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "hdr_mode",
        label: "HDR Mode",
        access: Access::f8e1(82),
        kind: ValueKind::Range { min: 0, max: 5 },
        values: &[],
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "low_input_lag",
        label: "Low Input Lag (F8/E1)",
        access: Access::f8e1(85),
        kind: ValueKind::Named,
        values: ON_OFF,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "game_color",
        label: "Game Color",
        access: Access::f8e1(87),
        kind: ValueKind::Range { min: 0, max: 20 },
        values: &[],
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "shadow_control",
        label: "Shadow Control",
        access: Access::f8e1(88),
        kind: ValueKind::Range { min: 0, max: 20 },
        values: &[],
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "low_blue_mode",
        label: "Low Blue Mode",
        access: Access::f8e1(89),
        kind: ValueKind::Named,
        values: LOW_BLUE,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "dial_point",
        label: "Dial Point",
        access: Access::f8e1(90),
        kind: ValueKind::Named,
        values: ON_OFF,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "frame_counter",
        label: "Frame Counter",
        access: Access::f8e1(91),
        kind: ValueKind::Named,
        values: FRAME_COUNTER,
        writable: true,
        confirmed: true,
    },
    Feature {
        id: "mbr",
        label: "MBR / MPRT",
        access: Access::e2a0(0x02),
        kind: ValueKind::Named,
        values: ON_OFF,
        writable: true,
        confirmed: false,
    },
    Feature {
        id: "mbr_sync",
        label: "MBR Sync",
        access: Access::e2a0(0x03),
        kind: ValueKind::Named,
        values: ON_OFF,
        writable: true,
        confirmed: false,
    },
    Feature {
        id: "smart_crosshair",
        label: "Smart Crosshair",
        access: Access::e2a0(0x04),
        kind: ValueKind::Named,
        values: SMART_CROSSHAIR,
        writable: true,
        confirmed: false,
    },
    Feature {
        id: "shadow_boost",
        label: "Shadow Boost",
        access: Access::e2a0(0x05),
        kind: ValueKind::Named,
        values: SHADOW_BOOST,
        writable: true,
        confirmed: false,
    },
    Feature {
        id: "low_input_lag_e2a0",
        label: "Low Input Lag (E2A0)",
        access: Access::e2a0(0x07),
        kind: ValueKind::Named,
        values: ON_OFF,
        writable: true,
        confirmed: false,
    },
    Feature {
        id: "low_blue_e2a0",
        label: "Low Blue Mode (E2A0)",
        access: Access::e2a0(0x24),
        kind: ValueKind::Named,
        values: LOW_BLUE_E2A0,
        writable: true,
        confirmed: false,
    },
    Feature {
        id: "adaptive_sync",
        label: "Adaptive-Sync / FreeSync",
        access: Access::e2a0(0x40),
        kind: ValueKind::Named,
        values: ON_OFF,
        writable: true,
        confirmed: false,
    },
];

/// Features confirmadas no U34G4C (para GUI).
pub fn confirmed_features() -> impl Iterator<Item = &'static Feature> {
    FEATURES.iter().filter(|f| f.confirmed)
}

pub fn find_feature(id: &str) -> Option<&'static Feature> {
    FEATURES
        .iter()
        .find(|f| f.id.eq_ignore_ascii_case(id) || f.label.eq_ignore_ascii_case(id))
}

pub fn parse_value(feature: &Feature, raw: &str) -> Result<u16, String> {
    let trimmed = raw.trim();
    match feature.kind {
        ValueKind::Range { min, max } => {
            let n: u16 = trimmed
                .parse()
                .map_err(|_| format!("valor numérico inválido: {raw}"))?;
            if n < min || n > max {
                return Err(format!(
                    "valor {n} fora de [{min}, {max}] para {}",
                    feature.id
                ));
            }
            Ok(n)
        }
        ValueKind::Named => {
            if let Ok(n) = trimmed.parse::<u16>() {
                if feature.values.iter().any(|v| v.code == n) {
                    return Ok(n);
                }
                return Err(format!(
                    "código {n} não mapeado para {}; use um dos: {}",
                    feature.id,
                    feature
                        .values
                        .iter()
                        .map(|v| v.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            let lower = trimmed.to_ascii_lowercase();
            feature
                .values
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(&lower))
                .map(|v| v.code)
                .ok_or_else(|| {
                    format!(
                        "valor desconhecido '{raw}' para {}; opções: {}",
                        feature.id,
                        feature
                            .values
                            .iter()
                            .map(|v| v.name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
        }
    }
}

pub fn format_value(feature: &Feature, code: u16) -> String {
    if let Some(v) = feature.values.iter().find(|v| v.code == code) {
        format!("{} ({code})", v.name)
    } else {
        code.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overdrive_round_trip() {
        let f = find_feature("overdrive").unwrap();
        assert_eq!(parse_value(f, "fast").unwrap(), 1);
        assert_eq!(parse_value(f, "fastest").unwrap(), 3);
        assert_eq!(parse_value(f, "3").unwrap(), 3);
        assert!(parse_value(f, "nope").is_err());
        // format prefere o primeiro nome com aquele código
        assert_eq!(format_value(f, 2), "faster (2)");
        match f.access {
            Access::Tpv { app, acode, ext } => {
                assert_eq!((app, acode, ext), (0xF8, 0xE1, 81));
            }
            _ => panic!("expected tpv"),
        }
    }

    #[test]
    fn adaptive_sync_e2a0() {
        let f = find_feature("adaptive_sync").unwrap();
        assert_eq!(parse_value(f, "on").unwrap(), 1);
        match f.access {
            Access::Tpv { app, acode, ext } => {
                assert_eq!((app, acode, ext), (0xE2, 0xA0, 0x40));
            }
            _ => panic!("expected tpv"),
        }
    }

    #[test]
    fn brightness_range() {
        let f = find_feature("brightness").unwrap();
        assert_eq!(parse_value(f, "50").unwrap(), 50);
        assert!(parse_value(f, "101").is_err());
        assert!(matches!(f.access, Access::Standard { vcp: 0x10 }));
    }

    #[test]
    fn all_feature_ids_unique() {
        let mut ids: Vec<_> = FEATURES.iter().map(|f| f.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), FEATURES.len());
    }

    #[test]
    fn game_mode_codes() {
        let f = find_feature("game_mode").unwrap();
        assert_eq!(parse_value(f, "fps").unwrap(), 11);
        assert_eq!(parse_value(f, "gamer1").unwrap(), 14);
    }
}
