//! Log técnico (stderr + opcional `$XDG_CACHE_HOME/aoc-gmenu/ddc.log`).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static CACHE_LOG: Mutex<Option<PathBuf>> = Mutex::new(None);

fn cache_log_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("aoc-gmenu").join("ddc.log");
        }
    }
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("aoc-gmenu")
        .join("ddc.log")
}

/// Mensagem amigável para a GUI (sem hex / opcodes / ddcutil).
pub const FRIENDLY_READ_FAIL: &str = "Não consegui ler, tentar de novo";

/// Registra detalhe técnico; a UI deve mostrar só [`FRIENDLY_READ_FAIL`].
pub fn tech(msg: impl AsRef<str>) {
    let msg = msg.as_ref();
    eprintln!("[aoc-gmenu] {msg}");
    let path = {
        let mut slot = CACHE_LOG.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_none() {
            *slot = Some(cache_log_path());
        }
        slot.clone().unwrap()
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{msg}");
    }
}

/// Sanitiza erro para CLI: remove dumps hex longos e a frase ddcutil/TPV.
pub fn short_cli_reason(err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    if lower.contains("não suportado") || lower.contains("unsupported") {
        return "não suportado".into();
    }
    // Remover arrays estilo [D2, 78, …] ou hex compacto.
    let mut out = String::new();
    let mut chars = err.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            // pular até ]
            for c2 in chars.by_ref() {
                if c2 == ']' {
                    break;
                }
            }
            out.push_str("[…]");
            continue;
        }
        out.push(c);
    }
    let out = out
        .replace(
            "; ddcutil não cobre framing TPV estendido",
            "",
        )
        .replace("ddcutil não cobre framing TPV estendido", "");
    let out = out.trim().trim_end_matches(';').trim().to_string();
    if out.len() > 120 {
        format!("{}…", &out[..117])
    } else if out.is_empty() {
        FRIENDLY_READ_FAIL.into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_hex_dump_and_ddcutil_tpv() {
        let raw = "i2c falhou para TPV (opcode 0x02 ausente em [D2, 78, 6D, 3B, DE, FD, 7E, 4D, AA, B6, 0C, 59, 4F, 74, 3C, F8]); ddcutil não cobre framing TPV estendido";
        let s = short_cli_reason(raw);
        assert!(!s.contains("D2"));
        assert!(!s.to_ascii_lowercase().contains("ddcutil não cobre"));
        assert!(s.contains("opcode") || s.contains("i2c") || s.contains("[…]"));
    }

    #[test]
    fn friendly_constant() {
        assert!(FRIENDLY_READ_FAIL.contains("Não consegui"));
    }
}
