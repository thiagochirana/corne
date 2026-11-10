//! Efeitos de base. Cada efeito é uma função "cor do LED i no tempo t".
//!
//! O tempo vem do relógio do lado central (sincronizado pelo link split) e as
//! coordenadas `x` cobrem o teclado inteiro (0..223), então ondas e degradês
//! continuam de uma metade para a outra sem emenda.

use crate::color::{Hsv, Rgb};
use crate::leds::{LEDS, Led};
use crate::math::{isqrt, noise2, sin8};
use crate::theme::Theme;
use corne_core::layout::Side;
use corne_core::state::Settings;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Effect {
    /// Cor única (matiz/saturação ajustáveis com RGB_HUI/RGB_SAI).
    Solid,
    /// Degradê da paleta do tema, deslizando devagar.
    Gradient,
    /// Respiração com a paleta do tema.
    Breathing,
    /// Arco-íris em onda horizontal contínua entre as metades.
    Rainbow,
    /// Aurora: ruído suave fluindo pela paleta.
    Aurora,
    /// Plasma: soma de senos.
    Plasma,
    /// Ondas que saem das teclas apertadas.
    Ripple,
    /// Mapa de calor das teclas mais usadas.
    Heatmap,
    /// Estrelas piscando aleatoriamente.
    Starlight,
    /// Diagnóstico: acende os LEDs um por um na ordem da corrente.
    Test,
}

pub const EFFECTS: [Effect; 10] = [
    Effect::Solid,
    Effect::Gradient,
    Effect::Breathing,
    Effect::Rainbow,
    Effect::Aurora,
    Effect::Plasma,
    Effect::Ripple,
    Effect::Heatmap,
    Effect::Starlight,
    Effect::Test,
];

impl Effect {
    pub fn from_index(i: u8) -> Effect {
        EFFECTS[i as usize % EFFECTS.len()]
    }

    pub fn name(self) -> &'static str {
        match self {
            Effect::Solid => "Solid",
            Effect::Gradient => "Gradient",
            Effect::Breathing => "Breath",
            Effect::Rainbow => "Rainbow",
            Effect::Aurora => "Aurora",
            Effect::Plasma => "Plasma",
            Effect::Ripple => "Ripple",
            Effect::Heatmap => "Heat",
            Effect::Starlight => "Stars",
            Effect::Test => "Test",
        }
    }
}

/// Uma onda saindo de uma tecla.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ripple {
    pub x: u8,
    pub y: u8,
    pub start: u32,
    pub live: bool,
}

/// Estado mutável que alguns efeitos precisam.
pub struct EffectState {
    pub heat: [u8; LEDS],
    pub stars: [u8; LEDS],
    pub ripples: [Ripple; 8],
}

impl Default for EffectState {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectState {
    pub const fn new() -> Self {
        EffectState { heat: [0; LEDS], stars: [0; LEDS], ripples: [Ripple { x: 0, y: 0, start: 0, live: false }; 8] }
    }
}

pub struct Ctx<'a> {
    /// Tempo já escalado pela velocidade (ms "de animação").
    pub t: u32,
    /// Tempo real (ms), para efeitos reativos.
    pub now: u32,
    pub side: Side,
    pub theme: &'static Theme,
    pub settings: &'a Settings,
    pub state: &'a EffectState,
}

pub fn render(effect: Effect, i: usize, led: &Led, c: &Ctx) -> Rgb {
    let pal = c.theme.palette(c.side);
    let t = c.t;
    let (x, y) = (led.x as u32, led.y as u32);
    match effect {
        Effect::Solid => Hsv::new(c.settings.rgb_hue, c.settings.rgb_sat, 255).to_rgb(),
        Effect::Gradient => pal.sample((x + (t >> 5)) as u8),
        Effect::Breathing => {
            let b = sin8((t >> 3) as u8);
            let floor = 40u16;
            let lvl = (floor + (b as u16 * (255 - floor)) / 255) as u8;
            pal.sample((t >> 7) as u8).scale(lvl)
        }
        Effect::Rainbow => {
            let h = c.settings.rgb_hue.wrapping_add((x + (t >> 4)) as u8);
            Hsv::new(h, c.settings.rgb_sat, 255).to_rgb()
        }
        Effect::Aurora => {
            let n = noise2(x * 6 + t / 3, y * 6 + t / 5);
            let glow = noise2(x * 3 + 5000 + t / 7, y * 3 + t / 11);
            let lvl = 60u16 + (glow as u16 * 195) / 255;
            pal.sample(n).scale(lvl as u8)
        }
        Effect::Plasma => {
            let a = sin8((x + (t >> 4)) as u8) as u16;
            let b = sin8(((y * 3) as u8).wrapping_sub((t >> 5) as u8)) as u16;
            let d = sin8(((x + y * 2) as u8).wrapping_add((t >> 3) as u8)) as u16;
            pal.sample(((a + b + d) / 3) as u8)
        }
        Effect::Ripple => {
            let mut out = pal.sample((x >> 1) as u8).scale(24);
            for r in c.state.ripples.iter().filter(|r| r.live) {
                let age = c.now.wrapping_sub(r.start);
                let speed = 40 + c.settings.rgb_speed as u32 / 2; // px/s * ~0.4
                let radius = age * speed / 400;
                let dx = x as i32 - r.x as i32;
                let dy = (y as i32 - r.y as i32) * 2; // y é mais "comprimido"
                let dist = isqrt((dx * dx + dy * dy) as u32);
                let off = dist.abs_diff(radius);
                if off < 14 {
                    let ring = 255 - off * 18;
                    let fade = 255u32.saturating_sub(age / 4);
                    let lvl = (ring * fade / 255) as u8;
                    let col = pal.sample((radius * 2 + r.start / 8) as u8);
                    out = out.add(col.scale(lvl));
                }
            }
            out
        }
        Effect::Heatmap => {
            let h = c.state.heat[i];
            if h < 128 {
                pal.sample(0).scale(20).lerp(pal.sample(96), h << 1)
            } else {
                pal.sample(96).lerp(Rgb::new(255, 240, 200), (h - 128) << 1)
            }
        }
        Effect::Starlight => {
            let s = c.state.stars[i];
            let base = pal.sample((x as u8).wrapping_add((t >> 6) as u8)).scale(18);
            let star = pal.sample(crate::math::hash(i as u32 ^ (c.now / 2000)) as u8).lerp(Rgb::new(255, 255, 255), 80);
            base.add(star.scale(s))
        }
        Effect::Test => {
            let cur = (c.now / 400) as usize % LEDS;
            if i == cur {
                Rgb::new(255, 255, 255)
            } else if i < cur {
                Rgb::new(0, 40, 0)
            } else {
                Rgb::new(20, 0, 0)
            }
        }
    }
}
