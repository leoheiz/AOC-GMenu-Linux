//! Backend DDC: trait + i2c-dev real + mock para testes. Fallback ddcutil (somente get).

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use i2cdev::core::I2CDevice;
use i2cdev::linux::LinuxI2CDevice;
use thiserror::Error;

use crate::bus_lock;
use crate::ddc::{self, VcpReply, GET_RETRIES, RESULT_UNSUPPORTED};
use crate::edid::{self, EdidInfo};
use crate::features::Access;
use crate::logutil;

static RAW_DUMP: AtomicBool = AtomicBool::new(false);

pub fn set_raw_dump(enabled: bool) {
    RAW_DUMP.store(enabled, Ordering::Relaxed);
}

fn raw_enabled() -> bool {
    RAW_DUMP.load(Ordering::Relaxed)
}

#[derive(Debug, Error)]
pub enum BackendError {
    #[error("i2c: {0}")]
    I2c(String),
    #[error("ddcutil: {0}")]
    Ddcutil(String),
    #[error("EDID: {0}")]
    Edid(String),
    #[error("não suportado")]
    Unsupported,
    #[error("{0}")]
    Other(String),
}

/// Acesso serializado a Get/Set VCP (real ou mock).
pub trait Backend: Send {
    fn get(&mut self, access: Access) -> Result<VcpReply, BackendError>;
    fn set(&mut self, access: Access, value: u16) -> Result<(), BackendError>;
}

#[derive(Debug, Clone)]
pub struct DetectedMonitor {
    pub connector: String,
    pub bus: u32,
    pub edid: EdidInfo,
}

#[derive(Debug, Clone)]
struct ConnectorBuses {
    connector: String,
    /// Buses preferidos: filhos `i2c-*` (DP AUX) primeiro, depois `ddc`.
    buses: Vec<u32>,
    edid_path: PathBuf,
}

/// Backend i2c real (e fallback ddcutil em Get de VCP padrão).
#[derive(Debug)]
pub struct I2cBackend {
    pub bus: u32,
}

impl I2cBackend {
    pub fn new(bus: u32) -> Self {
        Self { bus }
    }
}

impl Backend for I2cBackend {
    fn get(&mut self, access: Access) -> Result<VcpReply, BackendError> {
        get_feature(self.bus, access)
    }

    fn set(&mut self, access: Access, value: u16) -> Result<(), BackendError> {
        set_feature(self.bus, access, value)
    }
}

/// Mock in-memory: não toca `/dev/i2c`. Grava sequência de writes para asserts.
#[derive(Debug, Default)]
pub struct MockBackend {
    pub values: HashMap<AccessKey, (u16, u16)>,
    pub writes: Vec<(Access, u16)>,
    pub unsupported: Vec<AccessKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AccessKey {
    pub kind: u8,
    pub a: u8,
    pub b: u8,
    pub c: u8,
}

impl AccessKey {
    pub fn from_access(a: Access) -> Self {
        match a {
            Access::Standard { vcp } => Self {
                kind: 0,
                a: vcp,
                b: 0,
                c: 0,
            },
            Access::Tpv { app, acode, ext } => Self {
                kind: 1,
                a: app,
                b: acode,
                c: ext,
            },
        }
    }
}

impl MockBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&mut self, access: Access, current: u16, maximum: u16) {
        self.values
            .insert(AccessKey::from_access(access), (current, maximum));
    }

    pub fn mark_unsupported(&mut self, access: Access) {
        self.unsupported.push(AccessKey::from_access(access));
    }
}

impl Backend for MockBackend {
    fn get(&mut self, access: Access) -> Result<VcpReply, BackendError> {
        let key = AccessKey::from_access(access);
        if self.unsupported.contains(&key) {
            return Err(BackendError::Unsupported);
        }
        let (current, maximum) = self.values.get(&key).copied().unwrap_or((0, 100));
        let (vcp_or_marker, vcp_type) = match access {
            Access::Standard { vcp } => (vcp, 0),
            Access::Tpv { app, ext, .. } => (app, ext),
        };
        Ok(VcpReply {
            result_code: 0,
            vcp_or_marker,
            vcp_type,
            maximum,
            current,
        })
    }

    fn set(&mut self, access: Access, value: u16) -> Result<(), BackendError> {
        let key = AccessKey::from_access(access);
        if self.unsupported.contains(&key) {
            return Err(BackendError::Unsupported);
        }
        self.writes.push((access, value));
        let max = self.values.get(&key).map(|(_, m)| *m).unwrap_or(100);
        self.values.insert(key, (value, max));
        Ok(())
    }
}

/// Enumera conectores DRM com possíveis buses DDC.
fn list_drm_connectors() -> Vec<ConnectorBuses> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return out;
    };
    for ent in entries.flatten() {
        let name = ent.file_name().to_string_lossy().into_owned();
        if name == "version" || !name.contains('-') {
            continue;
        }
        let path = ent.path();
        let edid_path = path.join("edid");
        if !edid_path.exists() {
            continue;
        }

        let mut buses = Vec::new();

        if let Ok(children) = fs::read_dir(&path) {
            let mut child_buses: Vec<u32> = children
                .flatten()
                .filter_map(|c| {
                    let n = c.file_name().to_string_lossy().into_owned();
                    n.strip_prefix("i2c-")
                        .and_then(|s| s.parse::<u32>().ok())
                        .filter(|_| c.path().is_dir())
                })
                .collect();
            child_buses.sort_unstable();
            for b in child_buses {
                if !buses.contains(&b) {
                    buses.push(b);
                }
            }
        }

        let ddc = path.join("ddc");
        if ddc.exists() {
            if let Ok(target) = fs::read_link(&ddc) {
                let bus_name = target
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                if let Some(num) = bus_name.strip_prefix("i2c-").and_then(|s| s.parse().ok()) {
                    if !buses.contains(&num) {
                        buses.push(num);
                    }
                }
            }
        }

        if buses.is_empty() {
            continue;
        }
        out.push(ConnectorBuses {
            connector: name,
            buses,
            edid_path,
        });
    }
    out.sort_by(|a, b| a.connector.cmp(&b.connector));
    out
}

fn read_edid_file(path: &Path) -> Result<Vec<u8>, BackendError> {
    let mut f = fs::File::open(path).map_err(|e| BackendError::Edid(e.to_string()))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)
        .map_err(|e| BackendError::Edid(e.to_string()))?;
    if buf.len() < 128 {
        return Err(BackendError::Edid(format!(
            "EDID curto em {}: {} bytes",
            path.display(),
            buf.len()
        )));
    }
    Ok(buf)
}

/// Lê EDID em 0x50 via i2c (read-only).
pub fn read_edid_i2c(bus: u32) -> Result<Vec<u8>, BackendError> {
    let path = format!("/dev/i2c-{bus}");
    let mut dev =
        LinuxI2CDevice::new(&path, 0x50).map_err(|e| BackendError::I2c(e.to_string()))?;
    let mut buf = vec![0u8; 128];
    let _ = dev.smbus_write_byte(0);
    thread::sleep(Duration::from_millis(10));
    dev.read(&mut buf)
        .map_err(|e| BackendError::I2c(format!("EDID read: {e}")))?;
    Ok(buf)
}

/// Probe inofensivo: Get VCP 0x10 (brightness). Retorna true se resposta válida.
pub fn probe_ddc_bus(bus: u32) -> bool {
    match get_feature_i2c(bus, Access::std(0x10)) {
        Ok(r) => !r.is_unsupported() && r.vcp_or_marker == 0x10,
        Err(_) => false,
    }
}

fn find_connector_for_bus(bus: u32) -> Option<(String, PathBuf)> {
    for c in list_drm_connectors() {
        if c.buses.contains(&bus) {
            return Some((c.connector, c.edid_path));
        }
    }
    None
}

pub fn detect_aoc(prefer_bus: Option<u32>) -> Result<DetectedMonitor, BackendError> {
    if let Some(bus) = prefer_bus {
        let (connector, edid_path) = find_connector_for_bus(bus)
            .unwrap_or_else(|| (format!("i2c-{bus}"), PathBuf::from("")));
        let edid_bytes = if edid_path.as_os_str().is_empty() {
            read_edid_i2c(bus)?
        } else {
            read_edid_file(&edid_path).or_else(|_| read_edid_i2c(bus))?
        };
        let edid = edid::parse_edid(&edid_bytes).map_err(BackendError::Edid)?;
        return Ok(DetectedMonitor {
            connector,
            bus,
            edid,
        });
    }

    let mut candidates = Vec::new();
    for c in list_drm_connectors() {
        let edid_bytes = match read_edid_file(&c.edid_path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let Ok(info) = edid::parse_edid(&edid_bytes) else {
            continue;
        };
        let is_aoc = info.manufacturer.eq_ignore_ascii_case("AOC");
        let name = info.model_name.clone().unwrap_or_default();
        let score = if is_aoc && name.to_ascii_uppercase().contains("U34G4") {
            3
        } else if is_aoc {
            2
        } else if !name.is_empty() {
            1
        } else {
            0
        };

        let mut chosen = None;
        for &bus in &c.buses {
            if probe_ddc_bus(bus) {
                chosen = Some(bus);
                break;
            }
        }
        let bus = chosen.unwrap_or(c.buses[0]);
        let score = if chosen.is_some() { score + 10 } else { score };

        candidates.push((
            score,
            DetectedMonitor {
                connector: c.connector,
                bus,
                edid: info,
            },
        ));
    }
    candidates.sort_by_key(|a| std::cmp::Reverse(a.0));
    candidates
        .into_iter()
        .map(|(_, m)| m)
        .next()
        .ok_or_else(|| {
            BackendError::Other(
                "nenhum monitor com EDID encontrado via /sys/class/drm".into(),
            )
        })
}

fn open_ddc(bus: u32) -> Result<LinuxI2CDevice, BackendError> {
    let path = format!("/dev/i2c-{bus}");
    LinuxI2CDevice::new(&path, ddc::I2C_ADDR).map_err(|e| BackendError::I2c(e.to_string()))
}

fn i2c_transact_dev(
    dev: &mut LinuxI2CDevice,
    write: &[u8],
    read_len: usize,
    delay_ms: u64,
) -> Result<Vec<u8>, BackendError> {
    if raw_enabled() {
        eprintln!("# RAW write: {}", ddc::hex_bytes(write));
    }
    dev.write(write)
        .map_err(|e| BackendError::I2c(format!("write: {e}")))?;
    thread::sleep(Duration::from_millis(delay_ms));
    if read_len == 0 {
        return Ok(Vec::new());
    }
    let mut buf = vec![0u8; read_len];
    dev.read(&mut buf)
        .map_err(|e| BackendError::I2c(format!("read: {e}")))?;
    if raw_enabled() {
        eprintln!("# RAW read:  {}", ddc::hex_bytes(&buf));
    }
    Ok(buf)
}

/// Descarta bytes residuais no barramento (best-effort) antes de retry.
fn flush_dev(dev: &mut LinuxI2CDevice) {
    for _ in 0..3 {
        let mut sink = [0u8; 32];
        if dev.read(&mut sink).is_err() {
            break;
        }
        // Se veio tudo 0xFF / zeros, ainda assim pausamos um pouco.
        thread::sleep(Duration::from_millis(5));
    }
}

/// Executa `op` até `max_attempts` vezes; `Unsupported` não é retentado.
/// `op` recebe o índice da tentativa (0-based). Testável sem I2C.
pub fn retry_operation<T, F>(max_attempts: u32, mut op: F) -> Result<T, BackendError>
where
    F: FnMut(u32) -> Result<T, BackendError>,
{
    let mut last_err = None;
    for attempt in 0..max_attempts {
        match op(attempt) {
            Ok(v) => return Ok(v),
            Err(BackendError::Unsupported) => return Err(BackendError::Unsupported),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| BackendError::Other("operação falhou".into())))
}

pub fn get_feature_i2c(bus: u32, access: Access) -> Result<VcpReply, BackendError> {
    let _lock = bus_lock::acquire().map_err(BackendError::Other)?;
    let frame = ddc::build_get(access);
    let delay = ddc::delay_after_write(access);
    let mut dev = open_ddc(bus)?;

    retry_operation(GET_RETRIES, |attempt| {
        if attempt > 0 {
            flush_dev(&mut dev);
            thread::sleep(Duration::from_millis(ddc::retry_backoff_ms(attempt, access)));
        }
        let buf = i2c_transact_dev(&mut dev, &frame, 16, delay)?;
        match ddc::parse_get_reply(&buf) {
            Ok(r) if r.is_unsupported() => Err(BackendError::Unsupported),
            Ok(r) => Ok(r),
            Err(e) => {
                logutil::tech(format!(
                    "get bus={bus} access={access:?} attempt={attempt}: {e} raw={}",
                    ddc::hex_bytes(&buf)
                ));
                Err(BackendError::Other(e))
            }
        }
    })
}

pub fn set_feature_i2c(bus: u32, access: Access, value: u16) -> Result<(), BackendError> {
    let _lock = bus_lock::acquire().map_err(BackendError::Other)?;
    let frame = ddc::build_set(access, value);
    let delay = ddc::delay_after_write(access);
    let mut dev = open_ddc(bus)?;
    let _ = i2c_transact_dev(&mut dev, &frame, 0, delay)?;
    thread::sleep(Duration::from_millis(ddc::DDC_DELAY_MS));
    Ok(())
}

fn ddcutil_get(bus: u32, vcp: u8) -> Result<VcpReply, BackendError> {
    let out = Command::new("ddcutil")
        .args([
            "--bus",
            &bus.to_string(),
            "--terse",
            "getvcp",
            &format!("{vcp:02x}"),
        ])
        .output()
        .map_err(|e| BackendError::Ddcutil(e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if err.to_ascii_lowercase().contains("unsupported")
            || err.to_ascii_lowercase().contains("not found")
            || err.contains("ERR")
        {
            return Err(BackendError::Unsupported);
        }
        return Err(BackendError::Ddcutil(err));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<_> = text.split_whitespace().collect();
    if parts.len() >= 5 && parts[0] == "VCP" {
        let current = parse_ddcutil_num(parts[3])?;
        let maximum = parse_ddcutil_num(parts[4]).unwrap_or(0);
        return Ok(VcpReply {
            result_code: 0,
            vcp_or_marker: vcp,
            vcp_type: 0,
            maximum,
            current,
        });
    }
    Err(BackendError::Ddcutil(format!(
        "saída inesperada: {}",
        text.trim()
    )))
}

fn parse_ddcutil_num(s: &str) -> Result<u16, BackendError> {
    let s = s.strip_prefix('x').unwrap_or(s);
    if s.chars().any(|c| matches!(c, 'a'..='f' | 'A'..='F')) {
        return u16::from_str_radix(s, 16).map_err(|e| BackendError::Ddcutil(e.to_string()));
    }
    s.parse::<u16>()
        .or_else(|_| u16::from_str_radix(s, 16))
        .map_err(|e| BackendError::Ddcutil(e.to_string()))
}

pub fn get_feature(bus: u32, access: Access) -> Result<VcpReply, BackendError> {
    match get_feature_i2c(bus, access) {
        Ok(r) => Ok(r),
        Err(BackendError::Unsupported) => Err(BackendError::Unsupported),
        Err(i2c_err) => match access {
            Access::Standard { vcp } => {
                // ddcutil só como fallback de VCP padrão (não TPV).
                let _lock = bus_lock::acquire().map_err(BackendError::Other)?;
                ddcutil_get(bus, vcp).map_err(|e| match e {
                    BackendError::Unsupported => BackendError::Unsupported,
                    other => {
                        logutil::tech(format!(
                            "fallback ddcutil VCP 0x{vcp:02X}: i2c={i2c_err}; ddcutil={other}"
                        ));
                        BackendError::Other(format!(
                            "i2c falhou ({}); fallback também falhou",
                            logutil::short_cli_reason(&i2c_err.to_string())
                        ))
                    }
                })
            }
            Access::Tpv { .. } => {
                logutil::tech(format!("TPV get falhou após retries: {i2c_err}"));
                Err(BackendError::Other(format!(
                    "leitura TPV inválida ({})",
                    logutil::short_cli_reason(&i2c_err.to_string())
                )))
            }
        },
    }
}

impl BackendError {
    /// Texto curto para a GUI (sem dumps técnicos).
    pub fn user_message(&self) -> String {
        match self {
            Self::Unsupported => "não suportado".into(),
            _ => logutil::FRIENDLY_READ_FAIL.into(),
        }
    }
}

pub fn set_feature(bus: u32, access: Access, value: u16) -> Result<(), BackendError> {
    set_feature_i2c(bus, access, value)
}

pub type SupportFlag = (&'static str, u16);

/// Decodes the TPV F8/E1 support bitmask from Get with ext `0x00`.
pub fn decode_tpv_support(bits: u16) -> Vec<SupportFlag> {
    const FLAGS: &[SupportFlag] = &[
        ("ECOMode", 0x0001),
        ("Gamma", 0x0002),
        ("DCR", 0x0004),
        ("DCBMode", 0x0008),
        ("OverDrive", 0x0010),
        ("HDRMode", 0x0020),
        ("LEDColor", 0x0040),
        ("LEDIntensity", 0x0080),
        ("LowInputLag", 0x0100),
        ("USBCharging", 0x0200),
        ("GameColor", 0x0400),
        ("ShadowControl", 0x0800),
        ("LowBlueMode", 0x1000),
        ("LightFX", 0x2000),
        ("DialPoint", 0x4000),
        ("FrameCounter", 0x8000),
    ];
    FLAGS
        .iter()
        .filter(|(_, m)| bits & m != 0)
        .copied()
        .collect()
}

/// Query de suporte TPV (Get puro F8/E1/00).
pub fn get_tpv_support(bus: u32) -> Result<(u16, Vec<SupportFlag>), BackendError> {
    let r = get_feature(bus, Access::f8e1(0))?;
    if r.result_code == RESULT_UNSUPPORTED {
        return Err(BackendError::Unsupported);
    }
    Ok((r.current, decode_tpv_support(r.current)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn mock_get_set_roundtrip() {
        let mut m = MockBackend::new();
        m.seed(Access::std(0x10), 80, 100);
        let r = m.get(Access::std(0x10)).unwrap();
        assert_eq!(r.current, 80);
        m.set(Access::std(0x10), 40).unwrap();
        assert_eq!(m.writes, vec![(Access::std(0x10), 40)]);
        assert_eq!(m.get(Access::std(0x10)).unwrap().current, 40);
    }

    #[test]
    fn mock_unsupported() {
        let mut m = MockBackend::new();
        m.mark_unsupported(Access::e2a0(0x40));
        assert!(matches!(
            m.get(Access::e2a0(0x40)),
            Err(BackendError::Unsupported)
        ));
    }

    #[test]
    fn retry_succeeds_after_n_failures() {
        let attempts = AtomicU32::new(0);
        let r = retry_operation(5, |_| {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            if n < 3 {
                Err(BackendError::Other(
                    "opcode 0x02 ausente em [D2, 78, 6D, 3B, DE, FD, 7E, 4D]".into(),
                ))
            } else {
                Ok(VcpReply {
                    result_code: 0,
                    vcp_or_marker: 0xf8,
                    vcp_type: 0x51,
                    maximum: 3,
                    current: 1,
                })
            }
        })
        .unwrap();
        assert_eq!(r.current, 1);
        assert_eq!(attempts.load(Ordering::SeqCst), 4); // 3 fails + 1 ok
    }

    #[test]
    fn retry_gives_up_and_unsupported_short_circuits() {
        let r: Result<VcpReply, _> =
            retry_operation(3, |_| Err(BackendError::Other("lixo".into())));
        assert!(r.is_err());

        let attempts = AtomicU32::new(0);
        let r: Result<VcpReply, _> = retry_operation(5, |_| {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(BackendError::Unsupported)
        });
        assert!(matches!(r, Err(BackendError::Unsupported)));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn user_message_hides_tech() {
        let e = BackendError::Other(
            "opcode 0x02 ausente em [D2, 78, 6D]; ddcutil não cobre framing TPV estendido".into(),
        );
        assert_eq!(e.user_message(), logutil::FRIENDLY_READ_FAIL);
        let short = logutil::short_cli_reason(&e.to_string());
        assert!(!short.contains("D2"));
        assert!(!short.to_ascii_lowercase().contains("ddcutil não cobre"));
    }

    #[test]
    fn get_uses_lock_path_helper() {
        // Garante que o módulo de lock está linkado e o path padrão é sensato.
        let p = bus_lock::default_lock_path();
        assert!(p.to_string_lossy().contains("aoc-gmenu"));
    }
}
