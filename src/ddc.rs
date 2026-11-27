//! Framing DDC/CI (MCCS + TPV estendido) e checksum.

use crate::features::Access;

pub const I2C_ADDR: u16 = 0x37;
pub const HOST_ADDR: u8 = 0x51;
pub const DEST_ADDR_WRITE: u8 = 0x6E;
/// Endereço virtual do host na verificação de checksum de respostas (leitura).
pub const REPLY_HOST_VIRTUAL: u8 = 0x50;

pub const OPCODE_GET: u8 = 0x01;
pub const OPCODE_SET: u8 = 0x03;
pub const OPCODE_GET_REPLY: u8 = 0x02;

/// Resultado MCCS: Unsupported VCP Code.
pub const RESULT_UNSUPPORTED: u8 = 0x01;

/// Delay após write Get VCP padrão antes do read (ms).
pub const DDC_DELAY_MS: u64 = 50;

/// Delay após write Get TPV (F8/E1, E2A0) — monitores AOC precisam de mais folga.
pub const TPV_DELAY_MS: u64 = 100;

/// Pausa entre leituras sequenciais (`list` / reload GUI).
pub const INTER_FEATURE_DELAY_MS: u64 = 45;

/// Tentativas em caso de busy / null / checksum / lixo no barramento.
pub const GET_RETRIES: u32 = 5;

/// Delay pós-write conforme tipo de acesso.
pub fn delay_after_write(access: crate::features::Access) -> u64 {
    match access {
        crate::features::Access::Standard { .. } => DDC_DELAY_MS,
        crate::features::Access::Tpv { .. } => TPV_DELAY_MS,
    }
}

/// Backoff crescente entre retries (attempt 0 = delay base; 1+ soma 30 ms × n).
pub fn retry_backoff_ms(attempt: u32, access: crate::features::Access) -> u64 {
    let base = delay_after_write(access);
    base + u64::from(attempt) * 30
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcpReply {
    pub result_code: u8,
    pub vcp_or_marker: u8,
    /// Byte de tipo MCCS (0x00 continuous / 0x01 momentary) **ou**, em respostas TPV F8/E1, o ext code ecoado.
    pub vcp_type: u8,
    pub maximum: u16,
    pub current: u16,
}

impl VcpReply {
    pub fn is_unsupported(&self) -> bool {
        self.result_code == RESULT_UNSUPPORTED
    }
}

/// Checksum de frame de **escrita**: XOR de `0x6E` com todos os bytes do pacote (incl. 0x51 e length).
pub fn checksum(bytes: &[u8]) -> u8 {
    let mut c = DEST_ADDR_WRITE;
    for b in bytes {
        c ^= b;
    }
    c
}

/// Checksum de **resposta**: XOR de `0x50` com source (`0x6E`) + length + payload (sem o próprio CHK).
pub fn reply_checksum(from_source_through_payload: &[u8]) -> u8 {
    let mut c = REPLY_HOST_VIRTUAL;
    for b in from_source_through_payload {
        c ^= b;
    }
    c
}

fn length_byte(payload_len: usize) -> u8 {
    0x80 | (payload_len as u8)
}

/// Monta frame de Get VCP padrão: `51 82 01 vv CHK`.
pub fn build_get_vcp(vcp: u8) -> Vec<u8> {
    let payload = [OPCODE_GET, vcp];
    let mut frame = Vec::with_capacity(5);
    frame.push(HOST_ADDR);
    frame.push(length_byte(payload.len()));
    frame.extend_from_slice(&payload);
    let chk = checksum(&frame);
    frame.push(chk);
    frame
}

/// Monta frame de Set VCP padrão: `51 84 03 vv vh vl CHK`.
pub fn build_set_vcp(vcp: u8, value: u16) -> Vec<u8> {
    let payload = [
        OPCODE_SET,
        vcp,
        (value >> 8) as u8,
        (value & 0xFF) as u8,
    ];
    let mut frame = Vec::with_capacity(7);
    frame.push(HOST_ADDR);
    frame.push(length_byte(payload.len()));
    frame.extend_from_slice(&payload);
    let chk = checksum(&frame);
    frame.push(chk);
    frame
}

/// Get TPV: `51 84 01 app acode ext CHK`.
pub fn build_get_tpv(app: u8, acode: u8, ext: u8) -> Vec<u8> {
    let payload = [OPCODE_GET, app, acode, ext];
    let mut frame = Vec::with_capacity(7);
    frame.push(HOST_ADDR);
    frame.push(length_byte(payload.len()));
    frame.extend_from_slice(&payload);
    let chk = checksum(&frame);
    frame.push(chk);
    frame
}

/// Set TPV: `51 86 03 app acode ext vh vl CHK`.
pub fn build_set_tpv(app: u8, acode: u8, ext: u8, value: u16) -> Vec<u8> {
    let payload = [
        OPCODE_SET,
        app,
        acode,
        ext,
        (value >> 8) as u8,
        (value & 0xFF) as u8,
    ];
    let mut frame = Vec::with_capacity(9);
    frame.push(HOST_ADDR);
    frame.push(length_byte(payload.len()));
    frame.extend_from_slice(&payload);
    let chk = checksum(&frame);
    frame.push(chk);
    frame
}

pub fn build_get(access: Access) -> Vec<u8> {
    match access {
        Access::Standard { vcp } => build_get_vcp(vcp),
        Access::Tpv { app, acode, ext } => build_get_tpv(app, acode, ext),
    }
}

pub fn build_set(access: Access, value: u16) -> Vec<u8> {
    match access {
        Access::Standard { vcp } => build_set_vcp(vcp, value),
        Access::Tpv { app, acode, ext } => build_set_tpv(app, acode, ext, value),
    }
}

fn is_null_or_busy(buf: &[u8]) -> bool {
    if buf.is_empty() {
        return true;
    }
    if buf.iter().take(8).all(|&b| b == 0xFF) {
        return true;
    }
    // Mensagem nula DDC/CI: `6E 80 BE` (length 0)
    let data = if buf[0] == DEST_ADDR_WRITE {
        buf
    } else {
        // sem líder: `80 xx`
        return buf[0] == 0x80;
    };
    data.len() >= 2 && (data[1] & 0x7F) == 0
}

/// Interpreta resposta Get VCP Feature Reply.
///
/// Layout MCCS (após source opcional `0x6E`):
/// `len(=0x88)  02  rc  vv  type  mh ml  sh sl  CHK`
///
/// Em respostas TPV F8/E1 no U34G4C, `vv` ecoa o AppCommand (`0xF8`) e `type` ecoa o ext.
/// Se o buffer estiver desalinhado, procura `6E 88 02` (ou `88 02`) no interior.
pub fn parse_get_reply(buf: &[u8]) -> Result<VcpReply, String> {
    match parse_get_reply_at(buf) {
        Ok(r) => Ok(r),
        Err(e) if buf.len() < 11 => Err(e),
        Err(first_err) => {
            // Resync: varre por frame válido embutido (lixo + resposta).
            for i in 1..buf.len().saturating_sub(10) {
                if buf[i] == DEST_ADDR_WRITE {
                    if let Ok(r) = parse_get_reply_at(&buf[i..]) {
                        return Ok(r);
                    }
                }
                if buf[i] == 0x88 && buf.get(i + 1) == Some(&OPCODE_GET_REPLY) {
                    if let Ok(r) = parse_get_reply_body(&buf[i..], false) {
                        return Ok(r);
                    }
                }
            }
            Err(first_err)
        }
    }
}

fn parse_get_reply_at(buf: &[u8]) -> Result<VcpReply, String> {
    if is_null_or_busy(buf) {
        return Err("resposta nula/busy (unsupported/busy)".into());
    }

    if !buf.is_empty() && buf[0] == DEST_ADDR_WRITE {
        parse_get_reply_body(buf, true)
    } else {
        // Alguns adapters omitem o source.
        parse_get_reply_body(buf, false)
    }
}

fn parse_get_reply_body(body: &[u8], has_source: bool) -> Result<VcpReply, String> {
    if body.len() < 3 {
        return Err(format!(
            "resposta DDC curta ({} bytes): {:02X?}",
            body.len(),
            body
        ));
    }

    let (len_idx, payload_start) = if has_source {
        (1usize, 2usize)
    } else {
        (0usize, 1usize)
    };

    let len_byte = body[len_idx];
    let payload_len = (len_byte & 0x7F) as usize;
    if payload_len < 8 {
        return Err(format!(
            "length DDC insuficiente ({payload_len}): {:02X?}",
            body
        ));
    }

    let need = payload_start + payload_len + 1; // + checksum
    if body.len() < need && body.len() < payload_start + 8 {
        return Err(format!(
            "buffer curto para payload {payload_len}: {:02X?}",
            body
        ));
    }

    // Localizar opcode 0x02 imediatamente após o length (ou no início do payload).
    let opcode_idx = if body.get(payload_start) == Some(&OPCODE_GET_REPLY) {
        payload_start
    } else {
        body
            .iter()
            .enumerate()
            .skip(payload_start)
            .take(payload_len.min(4))
            .find(|(_, b)| **b == OPCODE_GET_REPLY)
            .map(|(i, _)| i)
            .ok_or_else(|| format!("opcode 0x02 ausente em {:02X?}", body))?
    };

    if opcode_idx + 7 >= body.len() {
        return Err(format!("payload 0x02 incompleto em {:02X?}", body));
    }

    // Validar checksum quando temos source + length + payload + chk.
    if has_source {
        let chk_idx = payload_start + payload_len;
        if body.len() > chk_idx {
            let expected = reply_checksum(&body[..chk_idx]);
            let got = body[chk_idx];
            if expected != got {
                return Err(format!(
                    "checksum inválido (exp={expected:02X} got={got:02X}) em {:02X?}",
                    &body[..chk_idx + 1]
                ));
            }
        }
    }

    let result_code = body[opcode_idx + 1];
    let vcp_or_marker = body[opcode_idx + 2];
    let vcp_type = body[opcode_idx + 3];
    let maximum = u16::from(body[opcode_idx + 4]) << 8 | u16::from(body[opcode_idx + 5]);
    let current = u16::from(body[opcode_idx + 6]) << 8 | u16::from(body[opcode_idx + 7]);

    Ok(VcpReply {
        result_code,
        vcp_or_marker,
        vcp_type,
        maximum,
        current,
    })
}

/// Formata bytes em hex espaçado (`6e 88 02 …`).
pub fn hex_bytes(buf: &[u8]) -> String {
    buf.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_brightness_golden() {
        let frame = build_get_vcp(0x10);
        assert_eq!(frame, vec![0x51, 0x82, 0x01, 0x10, 0xAC]);
        assert_eq!(checksum(&[0x51, 0x82, 0x01, 0x10]), 0xAC);
    }

    #[test]
    fn set_contrast_golden() {
        let frame = build_set_vcp(0x12, 50);
        assert_eq!(frame[..6], [0x51, 0x84, 0x03, 0x12, 0x00, 50]);
        assert_eq!(frame.len(), 7);
        assert_eq!(*frame.last().unwrap(), checksum(&frame[..6]));
    }

    #[test]
    fn get_tpv_overdrive_golden() {
        let frame = build_get_tpv(0xF8, 0xE1, 81);
        assert_eq!(frame[..6], [0x51, 0x84, 0x01, 0xF8, 0xE1, 81]);
        assert_eq!(frame.len(), 7);
        assert_eq!(*frame.last().unwrap(), checksum(&frame[..6]));
    }

    #[test]
    fn set_tpv_adaptive_sync_on() {
        let frame = build_set_tpv(0xE2, 0xA0, 0x40, 1);
        assert_eq!(
            frame[..8],
            [0x51, 0x86, 0x03, 0xE2, 0xA0, 0x40, 0x00, 0x01]
        );
        assert_eq!(*frame.last().unwrap(), checksum(&frame[..8]));
    }

    /// Bytes reais capturados no U34G4C (bus 11): brightness 80/100.
    #[test]
    fn parse_real_brightness_u34g4c() {
        // 6e 88 02 00 10 00 00 64 00 50 90 …
        let buf = [
            0x6e, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x50, 0x90,
        ];
        let r = parse_get_reply(&buf).unwrap();
        assert_eq!(r.result_code, 0);
        assert_eq!(r.vcp_or_marker, 0x10);
        assert_eq!(r.vcp_type, 0x00);
        assert_eq!(r.maximum, 100);
        assert_eq!(r.current, 80);
        assert_eq!(
            reply_checksum(&buf[..10]),
            0x90,
            "checksum de resposta usa 0x50"
        );
    }

    /// Contrast 60/100 (captura real).
    #[test]
    fn parse_real_contrast_u34g4c() {
        let buf = [
            0x6e, 0x88, 0x02, 0x00, 0x12, 0x00, 0x00, 0x64, 0x00, 0x3c, 0xfe,
        ];
        let r = parse_get_reply(&buf).unwrap();
        assert_eq!(r.current, 60);
        assert_eq!(r.maximum, 100);
        assert_eq!(r.vcp_or_marker, 0x12);
    }

    /// Overdrive TPV F8/E1/0x51 → max=3 cur=1 (Fast).
    #[test]
    fn parse_real_tpv_overdrive_u34g4c() {
        let buf = [
            0x6e, 0x88, 0x02, 0x00, 0xf8, 0x51, 0x00, 0x03, 0x00, 0x01, 0x1f,
        ];
        let r = parse_get_reply(&buf).unwrap();
        assert_eq!(r.vcp_or_marker, 0xf8);
        assert_eq!(r.vcp_type, 0x51); // ext ecoado
        assert_eq!(r.maximum, 3);
        assert_eq!(r.current, 1);
    }

    /// ECO Mode TPV: max=7 cur=0 (standard).
    #[test]
    fn parse_real_tpv_eco_u34g4c() {
        let buf = [
            0x6e, 0x88, 0x02, 0x00, 0xf8, 0x11, 0x00, 0x07, 0x00, 0x00, 0x5a,
        ];
        let r = parse_get_reply(&buf).unwrap();
        assert_eq!(r.vcp_type, 0x11);
        assert_eq!(r.maximum, 7);
        assert_eq!(r.current, 0);
    }

    /// Sharpness unsupported (rc=0x01).
    #[test]
    fn parse_real_unsupported_sharpness() {
        let buf = [
            0x6e, 0x88, 0x02, 0x01, 0x87, 0x01, 0xff, 0xff, 0x00, 0x00, 0x33,
        ];
        let r = parse_get_reply(&buf).unwrap();
        assert!(r.is_unsupported());
        assert_eq!(r.vcp_or_marker, 0x87);
    }

    /// E2A0 Adaptive-Sync: unsupported neste painel.
    #[test]
    fn parse_real_e2a0_unsupported() {
        let buf = [
            0x6e, 0x88, 0x02, 0x01, 0xe2, 0x01, 0xff, 0xff, 0x00, 0x00, 0x56,
        ];
        let r = parse_get_reply(&buf).unwrap();
        assert!(r.is_unsupported());
    }

    /// Bitmask de suporte F8/E1/00: current=0xDD3F.
    #[test]
    fn parse_real_support_bitmask() {
        let buf = [
            0x6e, 0x88, 0x02, 0x00, 0xf8, 0x00, 0x00, 0x04, 0xdd, 0x3f, 0xaa,
        ];
        let r = parse_get_reply(&buf).unwrap();
        assert_eq!(r.current, 0xDD3F);
        assert_eq!(r.maximum, 4);
    }

    #[test]
    fn reject_null_message() {
        let buf = [0x6e, 0x80, 0xbe];
        assert!(parse_get_reply(&buf).is_err());
    }

    #[test]
    fn reject_all_ff() {
        assert!(parse_get_reply(&[0xff; 16]).is_err());
    }

    /// Frame válido deslocado após lixo — resync deve recuperar.
    #[test]
    fn resync_embedded_brightness_frame() {
        let mut buf = vec![0xD2, 0x78, 0x6D, 0x3B];
        buf.extend_from_slice(&[
            0x6e, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x50, 0x90,
        ]);
        let r = parse_get_reply(&buf).unwrap();
        assert_eq!(r.current, 80);
        assert_eq!(r.vcp_or_marker, 0x10);
    }

    /// Dump real reportado na GUI (barramento dessincronizado — sem opcode 0x02).
    #[test]
    fn reject_user_reported_garbage_tpv() {
        let buf = [
            0xD2, 0x78, 0x6D, 0x3B, 0xDE, 0xFD, 0x7E, 0x4D, 0xAA, 0xB6, 0x0C, 0x59, 0x4F, 0x74,
            0x3C, 0xF8,
        ];
        let err = parse_get_reply(&buf).unwrap_err();
        assert!(
            err.contains("opcode") || err.contains("length") || err.contains("curta") || err.contains("checksum") || err.contains("nula"),
            "expected validation failure, got: {err}"
        );
    }

    #[test]
    fn reject_short_and_empty() {
        assert!(parse_get_reply(&[]).is_err());
        assert!(parse_get_reply(&[0x6e]).is_err());
        assert!(parse_get_reply(&[0x6e, 0x88]).is_err());
    }

    #[test]
    fn delay_constants_tpv_slower() {
        const { assert!(TPV_DELAY_MS >= 80) };
        const { assert!(DDC_DELAY_MS >= 50) };
        assert_eq!(delay_after_write(Access::std(0x10)), DDC_DELAY_MS);
        assert_eq!(delay_after_write(Access::f8e1(81)), TPV_DELAY_MS);
        assert!(retry_backoff_ms(2, Access::f8e1(81)) > TPV_DELAY_MS);
    }

    #[test]
    fn reject_bad_checksum() {
        let buf = [
            0x6e, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x50, 0x00,
        ];
        assert!(parse_get_reply(&buf).unwrap_err().contains("checksum"));
    }

    #[test]
    fn parse_without_leader() {
        // Sem 0x6E: length + payload (checksum não validado sem source)
        let buf = [0x88, 0x02, 0x00, 0x12, 0x00, 0x00, 0x64, 0x00, 0x20];
        let r = parse_get_reply(&buf).unwrap();
        assert_eq!(r.current, 32);
        assert_eq!(r.maximum, 100);
    }

    #[test]
    fn access_dispatch() {
        let g = build_get(Access::std(0x10));
        assert_eq!(g, build_get_vcp(0x10));
        let s = build_set(Access::f8e1(81), 2);
        assert_eq!(s, build_set_tpv(0xF8, 0xE1, 81, 2));
    }

    /// Regressão: o bug antigo (ignorar type) lia brightness como 25600.
    #[test]
    fn old_bug_would_misread_type_as_max() {
        let buf = [
            0x6e, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x50, 0x90,
        ];
        // layout errado (sem type): max=type<<8|mh = 0, cur=mh<<8|ml = 0x6400
        let start = 2;
        let wrong_cur =
            u16::from(buf[start + 5]) << 8 | u16::from(buf[start + 6]);
        assert_eq!(wrong_cur, 0x6400);
        let r = parse_get_reply(&buf).unwrap();
        assert_eq!(r.current, 80);
    }
}
