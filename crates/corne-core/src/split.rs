//! Protocolo entre as duas metades (pelo fio de dados do cabo TRS).
//!
//! O link é half-duplex (um fio só), então quem fala é sempre o lado central:
//! ele manda um `Sync` periodicamente e o periférico responde com `Keys`.
//! Assim nunca os dois transmitem ao mesmo tempo.
//!
//! **Nenhuma tecla se perde:** cada lote de `Keys` tem um número (`seq`) e o
//! próximo `Sync` confirma o último lote aceito (`ack`). Enquanto não vier a
//! confirmação, o periférico reenvia o mesmo lote; o central ignora repetidos.
//!
//! Quadro: `[MAGIC, tipo, len, payload..., crc8]`

use crate::layout::KeyPos;
use crate::state::KeyboardState;

pub const MAGIC: u8 = 0xC5;
pub const MAX_PAYLOAD: usize = 24;
pub const MAX_FRAME: usize = MAX_PAYLOAD + 4;
/// Máximo de eventos de tecla numa resposta.
pub const MAX_KEY_EVENTS: usize = 8;

/// `ack` de quem ainda não aceitou nenhum lote (nunca é um `seq` válido).
pub const NO_ACK: u8 = 0xFF;

/// Próximo `seq`. O 0 só é usado no primeiro lote depois de ligar (assim o
/// central aceita o lote mesmo que a outra metade tenha reiniciado); depois o
/// ciclo é 1..=254.
pub fn next_seq(seq: u8) -> u8 {
    match seq {
        NO_ACK => 0,
        254.. => 1,
        s => s + 1,
    }
}

const SYNC_LEN: usize = 4 + KeyboardState::WIRE_LEN + 1;
const _: () = assert!(SYNC_LEN <= MAX_PAYLOAD);

const T_SYNC: u8 = 0x01;
const T_KEYS: u8 = 0x02;

/// Mensagens do link.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Message {
    /// Central → periférico: estado + relógio do central (ms) + último `seq`
    /// de `Keys` aceito.
    Sync { time_ms: u32, state: KeyboardState, ack: u8 },
    /// Periférico → central: lote `seq` de eventos de tecla (posição lógica, apertada?).
    Keys { seq: u8, count: u8, events: [(KeyPos, bool); MAX_KEY_EVENTS] },
}

/// CRC-8 (polinômio 0x07).
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
        }
    }
    crc
}

impl Message {
    /// Serializa num buffer; devolve o tamanho do quadro.
    pub fn encode(&self, out: &mut [u8; MAX_FRAME]) -> usize {
        let mut payload = [0u8; MAX_PAYLOAD];
        let (ty, len) = match self {
            Message::Sync { time_ms, state, ack } => {
                payload[..4].copy_from_slice(&time_ms.to_le_bytes());
                payload[4..4 + KeyboardState::WIRE_LEN].copy_from_slice(&state.to_wire());
                payload[SYNC_LEN - 1] = *ack;
                (T_SYNC, SYNC_LEN)
            }
            Message::Keys { seq, count, events } => {
                let n = (*count as usize).min(MAX_KEY_EVENTS);
                payload[0] = *seq;
                for (i, (pos, pressed)) in events.iter().take(n).enumerate() {
                    payload[1 + i] = pos.to_byte() | if *pressed { 0x80 } else { 0 };
                }
                (T_KEYS, 1 + n)
            }
        };
        out[0] = MAGIC;
        out[1] = ty;
        out[2] = len as u8;
        out[3..3 + len].copy_from_slice(&payload[..len]);
        out[3 + len] = crc8(&out[1..3 + len]);
        4 + len
    }

    fn decode(ty: u8, payload: &[u8]) -> Option<Message> {
        match ty {
            T_SYNC if payload.len() == SYNC_LEN => {
                let time_ms = u32::from_le_bytes(payload[..4].try_into().ok()?);
                let wire: &[u8; KeyboardState::WIRE_LEN] = payload[4..SYNC_LEN - 1].try_into().ok()?;
                Some(Message::Sync { time_ms, state: KeyboardState::from_wire(wire), ack: payload[SYNC_LEN - 1] })
            }
            T_KEYS if !payload.is_empty() && payload.len() <= 1 + MAX_KEY_EVENTS => {
                let mut events = [(KeyPos::default(), false); MAX_KEY_EVENTS];
                for (i, &b) in payload[1..].iter().enumerate() {
                    events[i] = (KeyPos::from_byte(b & 0x7F), b & 0x80 != 0);
                }
                Some(Message::Keys { seq: payload[0], count: (payload.len() - 1) as u8, events })
            }
            _ => None,
        }
    }
}

/// Decodificador byte a byte (resiliente a lixo e quadros cortados).
pub struct FrameDecoder {
    buf: [u8; MAX_FRAME],
    len: usize,
}

impl FrameDecoder {
    pub const fn new() -> Self {
        FrameDecoder { buf: [0; MAX_FRAME], len: 0 }
    }

    pub fn reset(&mut self) {
        self.len = 0;
    }

    /// Entrega um byte; devolve uma mensagem quando um quadro válido termina.
    pub fn push(&mut self, b: u8) -> Option<Message> {
        if self.len == 0 && b != MAGIC {
            return None;
        }
        if self.len >= MAX_FRAME {
            self.len = 0;
            return None;
        }
        self.buf[self.len] = b;
        self.len += 1;
        if self.len >= 3 {
            let plen = self.buf[2] as usize;
            if plen > MAX_PAYLOAD {
                self.len = 0;
                return None;
            }
            if self.len == 4 + plen {
                self.len = 0;
                let crc = crc8(&self.buf[1..3 + plen]);
                if crc != self.buf[3 + plen] {
                    return None;
                }
                return Message::decode(self.buf[1], &self.buf[3..3 + plen]);
            }
        }
        None
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}
