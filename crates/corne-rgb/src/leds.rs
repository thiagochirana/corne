//! Mapa de LEDs de cada metade: ordem na corrente WS2812/SK6812 e posição física.
//!
//! A ordem padrão segue a do QMK para o `crkbd` (6 LEDs de underglow e depois
//! 21 por tecla, em "serpentina" pelas colunas). A PCB é reversível, então a
//! metade direita tem a mesma ordem em coordenadas da matriz local.
//!
//! **Se as cores aparecerem nas teclas erradas**, ajuste [`CHAIN`]. O efeito
//! `Test` (escolha no preview ou no firmware) acende os LEDs um por um, na
//! ordem da corrente, para facilitar conferir.
//!
//! Se a sua placa **não tiver underglow**, apague as 6 entradas `Under` do
//! início de [`CHAIN`]; o número de LEDs se ajusta sozinho.

use corne_core::layout::{KeyPos, Side};

#[derive(Clone, Copy, Debug)]
pub enum Chain {
    /// LED embaixo da placa, posição (x, y) no espaço 0..223 x 0..63 da metade esquerda.
    Under(u8, u8),
    /// LED de uma tecla (linha, coluna) da matriz local da metade.
    Key(u8, u8),
}

use Chain::{Key, Under};

/// Ordem da corrente na metade (vale para os dois lados).
#[rustfmt::skip]
pub const CHAIN: [Chain; 27] = [
    // underglow
    Under(85, 16), Under(50, 13), Under(16, 20), Under(16, 38), Under(50, 48), Under(85, 52),
    // teclas, em serpentina pelas colunas (de dentro para fora)
    Key(3, 5), Key(2, 5), Key(1, 5), Key(0, 5),
    Key(0, 4), Key(1, 4), Key(2, 4), Key(3, 4),
    Key(3, 3), Key(2, 3), Key(1, 3), Key(0, 3),
    Key(0, 2), Key(1, 2), Key(2, 2),
    Key(2, 1), Key(1, 1), Key(0, 1),
    Key(0, 0), Key(1, 0), Key(2, 0),
];

/// LEDs por metade.
pub const LEDS: usize = CHAIN.len();

pub mod flags {
    pub const UNDERGLOW: u8 = 1;
    pub const KEY: u8 = 2;
    /// Tecla na coluna externa ou polegar (onde costumam ficar modificadores).
    pub const MODIFIER: u8 = 4;
}

#[derive(Clone, Copy, Debug)]
pub struct Led {
    /// Tecla lógica (se for LED de tecla).
    pub key: Option<KeyPos>,
    pub x: u8,
    pub y: u8,
    pub flags: u8,
}

/// Monta o mapa de LEDs de um lado.
pub fn build(side: Side) -> [Led; LEDS] {
    let mut out = [Led { key: None, x: 0, y: 0, flags: 0 }; LEDS];
    for (i, c) in CHAIN.iter().enumerate() {
        out[i] = match *c {
            Under(x, y) => {
                let x = if side == Side::Left { x } else { 223 - x };
                Led { key: None, x, y, flags: flags::UNDERGLOW }
            }
            Key(r, col) => {
                let pos = KeyPos::from_matrix(side, r, col);
                let (x, y) = pos.xy();
                let m = if col == 0 || r == 3 { flags::MODIFIER } else { 0 };
                Led { key: Some(pos), x, y, flags: flags::KEY | m }
            }
        };
    }
    out
}
