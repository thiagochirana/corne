//! Formato das configurações gravadas na flash (RGB e OLED).
//!
//! A flash só troca bits de 1 para 0 sem apagar, e apagar (setor de 4 KB)
//! demora ~45 ms e gasta a flash. Por isso as gravações viram um "diário":
//! cada gravação ocupa o próximo registro livre de [`RECORD_LEN`] bytes do
//! setor, e o setor só é apagado quando enche (uma vez a cada 128 gravações).
//! Ao ligar, vale o último registro válido.
//!
//! Registro: `[MAGIC0, MAGIC1, VERSÃO, id_padrão (2), configurações..., crc16 (2)]`.
//! Um registro pela metade (falta de energia no meio da gravação) falha no CRC
//! e é ignorado. `id_padrão` é um resumo dos valores iniciais compilados no
//! firmware: se você mudar `SETTINGS` em `user_config.rs` e regravar, o que
//! estava salvo é descartado e os valores novos passam a valer.

use crate::state::Settings;

pub const RECORD_LEN: usize = 32;
const MAGIC: [u8; 2] = [0x5E, 0x77];
const VERSION: u8 = 1;
const SETTINGS_LEN: usize = 10;
const ERASED: u8 = 0xFF;

/// CRC-16/CCITT-FALSE.
pub fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

fn settings_bytes(s: &Settings) -> [u8; SETTINGS_LEN] {
    [
        s.rgb_on as u8,
        s.rgb_effect,
        s.rgb_theme,
        s.rgb_hue,
        s.rgb_sat,
        s.rgb_val,
        s.rgb_speed,
        s.oled_on as u8,
        s.oled_scene,
        s.oled_scene_right,
    ]
}

fn settings_from(b: &[u8]) -> Settings {
    Settings {
        rgb_on: b[0] != 0,
        rgb_effect: b[1],
        rgb_theme: b[2],
        rgb_hue: b[3],
        rgb_sat: b[4],
        rgb_val: b[5],
        rgb_speed: b[6],
        oled_on: b[7] != 0,
        oled_scene: b[8],
        oled_scene_right: b[9],
    }
}

/// Resumo dos valores iniciais do firmware (veja o topo do módulo).
pub fn defaults_id(defaults: &Settings) -> u16 {
    crc16(&settings_bytes(defaults))
}

/// Monta um registro.
pub fn encode(s: &Settings, defaults: &Settings) -> [u8; RECORD_LEN] {
    let mut r = [0u8; RECORD_LEN];
    r[..2].copy_from_slice(&MAGIC);
    r[2] = VERSION;
    r[3..5].copy_from_slice(&defaults_id(defaults).to_le_bytes());
    r[5..5 + SETTINGS_LEN].copy_from_slice(&settings_bytes(s));
    let crc = crc16(&r[..RECORD_LEN - 2]);
    r[RECORD_LEN - 2..].copy_from_slice(&crc.to_le_bytes());
    r
}

/// Lê um registro: `Some((id_padrão, configurações))` se for válido.
pub fn decode(r: &[u8; RECORD_LEN]) -> Option<(u16, Settings)> {
    if r[..2] != MAGIC || r[2] != VERSION {
        return None;
    }
    let crc = u16::from_le_bytes([r[RECORD_LEN - 2], r[RECORD_LEN - 1]]);
    if crc != crc16(&r[..RECORD_LEN - 2]) {
        return None;
    }
    Some((u16::from_le_bytes([r[3], r[4]]), settings_from(&r[5..5 + SETTINGS_LEN])))
}

/// Registro ainda apagado (pode ser gravado sem apagar o setor).
pub fn is_blank(r: &[u8; RECORD_LEN]) -> bool {
    r.iter().all(|&b| b == ERASED)
}

/// Resultado da leitura do setor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scan {
    /// Configurações salvas, se houver e se forem destes valores iniciais.
    pub saved: Option<Settings>,
    /// Próximo registro livre; `None` = setor cheio (apagar e gravar no 0).
    pub next_slot: Option<usize>,
}

/// Percorre os `slots` registros do setor. `read(i)` devolve o registro `i`.
pub fn scan(slots: usize, defaults: &Settings, mut read: impl FnMut(usize) -> [u8; RECORD_LEN]) -> Scan {
    let mut last = None;
    let mut next_slot = None;
    for i in 0..slots {
        let r = read(i);
        if is_blank(&r) {
            // A gravação é sequencial: depois do primeiro livre, o resto também está.
            next_slot = Some(i);
            break;
        }
        if let Some(v) = decode(&r) {
            last = Some(v);
        }
    }
    let id = defaults_id(defaults);
    let saved = last.and_then(|(rid, s)| (rid == id).then_some(s));
    Scan { saved, next_slot }
}
