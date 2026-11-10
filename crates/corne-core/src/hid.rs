//! Relatório HID de teclado (boot protocol, 6KRO).

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct KeyboardReport {
    pub mods: u8,
    pub keys: [u8; 6],
}

impl KeyboardReport {
    /// Bytes no formato boot: `[mods, reservado, k1..k6]`.
    pub fn to_bytes(&self) -> [u8; 8] {
        let k = self.keys;
        [self.mods, 0, k[0], k[1], k[2], k[3], k[4], k[5]]
    }

    /// Adiciona uma tecla (ignora repetidas; ignora se já houver 6).
    pub fn push_key(&mut self, code: u8) {
        if code == 0 || self.keys.contains(&code) {
            return;
        }
        if let Some(slot) = self.keys.iter_mut().find(|k| **k == 0) {
            *slot = code;
        }
    }
}
