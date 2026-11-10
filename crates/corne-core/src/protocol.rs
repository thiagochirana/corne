//! Protocolo de configuração em tempo real (base para um configurador estilo Vial).
//!
//! Pacotes de 32 bytes trocados por uma interface HID "raw" (usage page 0xFF60,
//! a mesma convenção do QMK/VIA). O PC manda um pedido e o teclado responde
//! com o mesmo byte de comando no início (ou `0xFF` em erro).
//!
//! | cmd  | pedido                        | resposta                                   |
//! |------|-------------------------------|--------------------------------------------|
//! | 0x01 | `GetInfo`                     | `[01, versão, camadas, linhas, colunas, máx, lado]` (lado: 0 = esquerda, 1 = direita; desde a versão 2) |
//! | 0x02 | `GetKey l r c`                | `[02, l, r, c, hi, lo]`                    |
//! | 0x03 | `SetKey l r c hi lo`          | `[03, l, r, c, hi, lo]`                    |
//! | 0x04 | `GetRow l r`                  | `[04, l, r, 12 x (hi, lo)]`                |
//! | 0x05 | `ResetKeymap`                 | `[05]`                                     |
//! | 0x06 | `Save`                        | `[06, ok]`                                 |
//! | 0x07 | `GetSettings`                 | `[07, 16 bytes de estado]`                 |
//! | 0x08 | `SetSettings ...`             | `[08]`                                     |
//! | 0x09 | `Bootloader`                  | `[09]` (e reinicia)                        |

use crate::action::Action;
use crate::keymap::{KEYMAP_LAYERS, Keymap};
use crate::layout::{COLS, KeyPos, ROWS, Side};
use crate::state::{KeyboardState, Settings};

pub const PACKET: usize = 32;
pub const VERSION: u8 = 2;
pub const ERR: u8 = 0xFF;

/// O que o firmware precisa oferecer para o protocolo funcionar.
pub trait ConfigTarget {
    fn keymap(&mut self) -> &mut Keymap;
    fn reset_keymap(&mut self);
    fn get_settings(&mut self) -> Settings;
    fn set_settings(&mut self, s: Settings);
    /// Grava keymap e configurações na flash. `false` se não suportado/falhou.
    fn save(&mut self) -> bool;
    fn bootloader(&mut self);
    /// Metade que está respondendo (a ligada na USB).
    fn side(&self) -> Side;
}

pub fn handle(req: &[u8; PACKET], t: &mut impl ConfigTarget) -> [u8; PACKET] {
    let mut res = [0u8; PACKET];
    res[0] = req[0];
    let pos = |r: u8, c: u8| KeyPos::new(r, c);
    let err = |mut res: [u8; PACKET]| {
        res[0] = ERR;
        res
    };

    match req[0] {
        0x01 => {
            res[1] = VERSION;
            res[2] = t.keymap().layer_count();
            res[3] = ROWS as u8;
            res[4] = COLS as u8;
            res[5] = KEYMAP_LAYERS as u8;
            res[6] = match t.side() {
                Side::Left => 0,
                Side::Right => 1,
            };
        }
        0x02 => {
            let (l, r, c) = (req[1], req[2], req[3]);
            if l as usize >= KEYMAP_LAYERS || !pos(r, c).exists() {
                return err(res);
            }
            let v = t.keymap().get(l, pos(r, c)).encode();
            res[1..4].copy_from_slice(&req[1..4]);
            res[4..6].copy_from_slice(&v.to_be_bytes());
        }
        0x03 => {
            let (l, r, c) = (req[1], req[2], req[3]);
            let action = Action::decode(u16::from_be_bytes([req[4], req[5]]));
            if !t.keymap().set(l, pos(r, c), action) {
                return err(res);
            }
            res[1..4].copy_from_slice(&req[1..4]);
            res[4..6].copy_from_slice(&action.encode().to_be_bytes());
        }
        0x04 => {
            let (l, r) = (req[1], req[2]);
            if l as usize >= KEYMAP_LAYERS || r as usize >= ROWS {
                return err(res);
            }
            res[1] = l;
            res[2] = r;
            for c in 0..COLS {
                let v = t.keymap().get(l, pos(r, c as u8)).encode();
                res[3 + c * 2..5 + c * 2].copy_from_slice(&v.to_be_bytes());
            }
        }
        0x05 => t.reset_keymap(),
        0x06 => res[1] = t.save() as u8,
        0x07 => {
            let st = KeyboardState { settings: t.get_settings(), ..Default::default() };
            res[1..1 + KeyboardState::WIRE_LEN].copy_from_slice(&st.to_wire());
        }
        0x08 => {
            let wire: [u8; KeyboardState::WIRE_LEN] = req[1..1 + KeyboardState::WIRE_LEN].try_into().unwrap();
            t.set_settings(KeyboardState::from_wire(&wire).settings);
        }
        0x09 => t.bootloader(),
        _ => return err(res),
    }
    res
}
