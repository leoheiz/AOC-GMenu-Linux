//! Aplicação de perfis (ordem segura + delays). Só escreve features listadas.

use std::thread;
use std::time::Duration;

use thiserror::Error;

use crate::backend::Backend;
use crate::config::{AppConfig, Profile};
use crate::ddc::DDC_DELAY_MS;
use crate::features::{find_feature, parse_value, Access};

#[derive(Debug, Error)]
pub enum ProfileApplyError {
    #[error("perfil desconhecido: {0}")]
    Unknown(String),
    #[error("feature desconhecida no perfil: {0}")]
    BadFeature(String),
    #[error("valor inválido para {0}: {1}")]
    BadValue(String, String),
    #[error("{0}")]
    Backend(String),
}

/// Ordem segura de escrita: modos “pesados” primeiro, depois imagem fina.
const WRITE_ORDER: &[&str] = &[
    "low_blue_mode",
    "overdrive",
    "low_input_lag",
    "dcr",
    "eco_mode",
    "gamma",
    "color_temp",
    "shadow_control",
    "game_color",
    "frame_counter",
    "dial_point",
    "game_mode",
    "hdr_mode",
    "brightness",
    "contrast",
    "volume",
    "input_source",
];

fn order_key(id: &str) -> usize {
    WRITE_ORDER
        .iter()
        .position(|x| *x == id)
        .unwrap_or(WRITE_ORDER.len())
}

/// Sequência exata de writes `(Access, value)` que `apply_profile` executaria.
pub fn profile_writes(profile: &Profile) -> Result<Vec<(Access, u16)>, ProfileApplyError> {
    let mut items: Vec<(String, String)> = profile
        .features
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    items.sort_by(|a, b| order_key(&a.0).cmp(&order_key(&b.0)).then(a.0.cmp(&b.0)));

    let mut out = Vec::with_capacity(items.len());
    for (id, val) in items {
        let f = find_feature(&id).ok_or_else(|| ProfileApplyError::BadFeature(id.clone()))?;
        if !f.writable {
            return Err(ProfileApplyError::BadFeature(format!("{id} (não gravável)")));
        }
        let code = parse_value(f, &val)
            .map_err(|e| ProfileApplyError::BadValue(id.clone(), e))?;
        out.push((f.access, code));
    }
    Ok(out)
}

/// Aplica perfil no backend (com delays entre writes). Não altera a mira — o
/// chamador deve atualizar `config.crosshair.enabled` se `crosshair_enabled` estiver setado.
pub fn apply_profile<B: Backend>(
    backend: &mut B,
    profile: &Profile,
) -> Result<Vec<(String, u16)>, ProfileApplyError> {
    let writes = profile_writes(profile)?;
    let mut applied = Vec::new();
    let mut ids: Vec<String> = profile.features.keys().cloned().collect();
    ids.sort_by(|a, b| order_key(a).cmp(&order_key(b)).then(a.cmp(b)));

    for (i, (access, code)) in writes.into_iter().enumerate() {
        if i > 0 {
            thread::sleep(Duration::from_millis(DDC_DELAY_MS));
        }
        backend
            .set(access, code)
            .map_err(|e| ProfileApplyError::Backend(e.to_string()))?;
        applied.push((ids[i].clone(), code));
    }
    Ok(applied)
}

pub fn list_profiles(cfg: &AppConfig) -> Vec<(String, String)> {
    cfg.profiles
        .iter()
        .map(|(n, p)| (n.clone(), p.description.clone()))
        .collect()
}

pub fn get_profile<'a>(cfg: &'a AppConfig, name: &str) -> Result<&'a Profile, ProfileApplyError> {
    cfg.profiles
        .get(name)
        .or_else(|| {
            cfg.profiles
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v)
        })
        .ok_or_else(|| ProfileApplyError::Unknown(name.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockBackend;
    use crate::config::default_profiles;
    use crate::features::Access;

    #[test]
    fn jogo_write_sequence() {
        let profiles = default_profiles();
        let p = profiles.get("jogo").unwrap();
        let writes = profile_writes(p).unwrap();
        let expected_ids = [
            "low_blue_mode",
            "overdrive",
            "low_input_lag",
            "shadow_control",
            "game_color",
            "brightness",
            "contrast",
        ];
        let mut ids: Vec<_> = p.features.keys().cloned().collect();
        ids.sort_by(|a, b| order_key(a).cmp(&order_key(b)).then(a.cmp(b)));
        assert_eq!(ids, expected_ids);

        assert_eq!(writes[0], (Access::f8e1(89), 0)); // low_blue off
        assert_eq!(writes[1], (Access::f8e1(81), 1)); // overdrive fast
        assert_eq!(writes[2], (Access::f8e1(85), 1)); // low_input_lag on
        assert_eq!(writes[3], (Access::f8e1(88), 10)); // shadow
        assert_eq!(writes[4], (Access::f8e1(87), 12)); // game_color
        assert_eq!(writes[5], (Access::std(0x10), 90)); // brightness
        assert_eq!(writes[6], (Access::std(0x12), 60)); // contrast
    }

    #[test]
    fn apply_records_exact_writes() {
        let profiles = default_profiles();
        let p = profiles.get("leitura").unwrap();
        let expected = profile_writes(p).unwrap();
        let mut mock = MockBackend::new();
        apply_profile(&mut mock, p).unwrap();
        assert_eq!(mock.writes, expected);
    }

    #[test]
    fn original_includes_baseline() {
        let profiles = default_profiles();
        let p = profiles.get("original").unwrap();
        let writes = profile_writes(p).unwrap();
        assert!(writes.iter().any(|(a, v)| *a == Access::std(0x10) && *v == 80));
        assert!(writes.iter().any(|(a, v)| *a == Access::f8e1(81) && *v == 1));
        assert_eq!(p.crosshair_enabled, Some(false));
    }
}
