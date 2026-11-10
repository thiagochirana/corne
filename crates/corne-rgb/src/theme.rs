//! Temas de cor. Cada tema tem uma paleta para **cada lado** e uma cor por
//! camada (usada na sobreposição de camada). Edite ou acrescente temas aqui.

use crate::color::{Palette, Rgb};
use corne_core::layout::Side;

pub struct Theme {
    pub name: &'static str,
    pub left: Palette,
    pub right: Palette,
    /// Cor de destaque de cada camada (índice = número da camada).
    pub layers: [Rgb; 8],
    /// Cor das teclas modificadoras quando seguradas.
    pub mods: Rgb,
    /// Cor do indicador de Caps Lock.
    pub caps: Rgb,
}

impl Theme {
    pub fn palette(&self, side: Side) -> &Palette {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }
}

const fn p(a: u32, b: u32, c: u32, d: u32) -> Palette {
    Palette([Rgb::hex(a), Rgb::hex(b), Rgb::hex(c), Rgb::hex(d)])
}

const LAYERS_DEFAULT: [Rgb; 8] = [
    Rgb::hex(0x000000), // base: sem sobreposição
    Rgb::hex(0x00B4FF), // nav
    Rgb::hex(0xFF2E88), // sym
    Rgb::hex(0xFFB000), // adjust
    Rgb::hex(0x7CFF4F),
    Rgb::hex(0xB06CFF),
    Rgb::hex(0xFF5A1F),
    Rgb::hex(0x00FFD0),
];

pub const THEMES: &[Theme] = &[
    Theme {
        name: "Synthwave",
        left: p(0xFF2E88, 0x8A2BE2, 0x2D1B69, 0xFF6EC7),
        right: p(0x00E5FF, 0x2D1B69, 0x8A2BE2, 0x00FFB3),
        layers: LAYERS_DEFAULT,
        mods: Rgb::hex(0xFFFFFF),
        caps: Rgb::hex(0xFF0000),
    },
    Theme {
        name: "Ocean",
        left: p(0x003B73, 0x0074B7, 0x60A3D9, 0x00C2C7),
        right: p(0x00C2C7, 0x60A3D9, 0x0074B7, 0x002B55),
        layers: LAYERS_DEFAULT,
        mods: Rgb::hex(0xBFF8FF),
        caps: Rgb::hex(0xFF6A00),
    },
    Theme {
        name: "Ember",
        left: p(0xFF2200, 0xFF7A00, 0x7A0A00, 0xFFC300),
        right: p(0xFFC300, 0x7A0A00, 0xFF7A00, 0xFF2200),
        layers: LAYERS_DEFAULT,
        mods: Rgb::hex(0xFFF1C1),
        caps: Rgb::hex(0x00A2FF),
    },
    Theme {
        name: "Forest",
        left: p(0x0B6E4F, 0x08A045, 0x6BBF59, 0x1B3B2F),
        right: p(0x1B3B2F, 0x6BBF59, 0x08A045, 0xD4E09B),
        layers: LAYERS_DEFAULT,
        mods: Rgb::hex(0xF0FFE0),
        caps: Rgb::hex(0xFF2D55),
    },
    Theme {
        name: "Duo",
        // Um lado quente, outro frio: deixa bem visível qual metade é qual.
        left: p(0xFF4D00, 0xFF9E00, 0xFF0055, 0xFF4D00),
        right: p(0x0057FF, 0x00D4FF, 0x6A00FF, 0x0057FF),
        layers: LAYERS_DEFAULT,
        mods: Rgb::hex(0xFFFFFF),
        caps: Rgb::hex(0x00FF66),
    },
];

pub fn get(index: u8) -> &'static Theme {
    &THEMES[index as usize % THEMES.len()]
}
