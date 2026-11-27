//! Parsing mínimo de EDID (modelo / fabricante).

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EdidInfo {
    pub manufacturer: String,
    pub model_name: Option<String>,
    pub product_code: u16,
}

fn mfg_id(edid: &[u8]) -> Option<String> {
    if edid.len() < 10 {
        return None;
    }
    let w = u16::from(edid[8]) << 8 | u16::from(edid[9]);
    let letter = |n: u16| -> Option<char> {
        let v = ((n - 1) % 26) as u8;
        Some((b'A' + v) as char)
    };
    Some(format!(
        "{}{}{}",
        letter((w >> 10) & 0x1F)?,
        letter((w >> 5) & 0x1F)?,
        letter(w & 0x1F)?
    ))
}

fn descriptor_text(desc: &[u8]) -> Option<String> {
    if desc.len() < 18 {
        return None;
    }
    // Display Product Name: tag 0xFC
    if desc[0] == 0 && desc[1] == 0 && desc[2] == 0 && desc[3] == 0xFC {
        let raw = &desc[5..18];
        let end = raw.iter().position(|&b| b == 0x0A || b == 0x00).unwrap_or(raw.len());
        let s = String::from_utf8_lossy(&raw[..end])
            .trim()
            .trim_end_matches('\0')
            .to_string();
        if s.is_empty() {
            None
        } else {
            Some(s)
        }
    } else {
        None
    }
}

pub fn parse_edid(edid: &[u8]) -> Result<EdidInfo, String> {
    if edid.len() < 128 {
        return Err(format!("EDID curto: {} bytes", edid.len()));
    }
    if edid[0..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
        return Err("header EDID inválido".into());
    }
    let manufacturer = mfg_id(edid).ok_or_else(|| "fabricante inválido".to_string())?;
    let product_code = u16::from(edid[10]) | (u16::from(edid[11]) << 8);
    let mut model_name = None;
    for i in 0..4 {
        let off = 54 + i * 18;
        if let Some(name) = descriptor_text(&edid[off..off + 18]) {
            model_name = Some(name);
            break;
        }
    }
    Ok(EdidInfo {
        manufacturer,
        model_name,
        product_code,
    })
}

/// EDID sintético mínimo: AOC + nome "U34G4C" no primeiro descriptor 0xFC.
#[cfg(test)]
pub fn synthetic_aoc_u34g4c() -> Vec<u8> {
    let mut e = vec![0u8; 128];
    e[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
    // AOC: A=1,O=15,C=3 → (1<<10)|(15<<5)|3 = 0x05E3
    e[8] = 0x05;
    e[9] = 0xE3;
    e[10] = 0x34;
    e[11] = 0x12;
    // descriptor 0 at offset 54: type FC + "U34G4C"
    let off = 54;
    e[off] = 0;
    e[off + 1] = 0;
    e[off + 2] = 0;
    e[off + 3] = 0xFC;
    e[off + 4] = 0;
    let name = b"U34G4C\n";
    e[off + 5..off + 5 + name.len()].copy_from_slice(name);
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_synthetic_u34g4c() {
        let edid = synthetic_aoc_u34g4c();
        let info = parse_edid(&edid).unwrap();
        assert_eq!(info.manufacturer, "AOC");
        assert_eq!(info.model_name.as_deref(), Some("U34G4C"));
    }

    #[test]
    fn reject_short() {
        assert!(parse_edid(&[0u8; 10]).is_err());
    }
}
