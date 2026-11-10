//! # corne-rgb
//!
//! Complemento de RGB: efeitos, temas por lado e sobreposições (camada,
//! modificadores, Caps Lock). Não conhece o hardware: recebe o estado do
//! teclado e o tempo, e preenche um buffer de cores. O firmware manda esse
//! buffer para os LEDs; o `corne-preview` desenha no terminal.
//!
//! Ordem de renderização de cada quadro:
//! 1. efeito de base ([`effects`]);
//! 2. sobreposições ([`RgbEngine::render`]);
//! 3. brilho (`rgb_val`), correção de gama e limitador de corrente.
#![no_std]

pub mod color;
pub mod effects;
pub mod leds;
pub mod math;
pub mod theme;

use color::{BLACK, Rgb, gamma_rgb};
use corne_core::action::RgbCmd;
use corne_core::layout::{KeyPos, Side};
use corne_core::state::{KeyboardState, Settings};
use effects::{Ctx, Effect, EffectState, Ripple};
use leds::{LEDS, Led, flags};
use math::{Rng, isqrt, sin8};

pub use leds::LEDS as LED_COUNT;

/// Fração máxima da corrente total permitida (0..=255). Cada LED SK6812 puxa
/// ~60 mA no branco máximo; 27 LEDs x 2 metades passariam de 3 A, bem acima
/// dos 500 mA de uma porta USB. 64/255 ≈ 25% mantém a média num valor seguro.
pub const POWER_LIMIT: u8 = 64;

/// Entrada de um quadro.
pub struct Frame<'a> {
    pub state: &'a KeyboardState,
    /// Tempo em ms no relógio do lado central.
    pub now: u32,
    /// Teclas (bit = `KeyPos::index`) que fazem algo na camada mais alta ativa.
    /// Usado para destacar só as teclas "vivas" da camada.
    pub layer_keys: u64,
}

pub struct RgbEngine {
    side: Side,
    leds: [Led; LEDS],
    fx: EffectState,
    rng: Rng,
    last: u32,
    anim_t: u32,
    /// Teclas desta metade seguradas agora (bit = `KeyPos::index`).
    held: u64,
}

impl RgbEngine {
    pub fn new(side: Side) -> Self {
        RgbEngine {
            side,
            leds: leds::build(side),
            fx: EffectState::new(),
            rng: Rng(0x1234_5678 ^ side.as_u8() as u32),
            last: 0,
            anim_t: 0,
            held: 0,
        }
    }

    pub fn leds(&self) -> &[Led; LEDS] {
        &self.leds
    }

    /// Avise o motor quando uma tecla **desta metade** mudar (efeitos reativos).
    pub fn key_event(&mut self, pos: KeyPos, pressed: bool, now: u32) {
        let bit = 1u64 << pos.index();
        if !pressed {
            self.held &= !bit;
            return;
        }
        self.held |= bit;
        let (kx, ky) = pos.xy();
        // Onda nova no lugar da mais antiga.
        let slot = self
            .fx
            .ripples
            .iter()
            .enumerate()
            .min_by_key(|(_, r)| if r.live { r.start.wrapping_sub(now) as i32 } else { i32::MIN })
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.fx.ripples[slot] = Ripple { x: kx, y: ky, start: now, live: true };
        // Calor na tecla e um pouco nos vizinhos.
        for (i, led) in self.leds.iter().enumerate() {
            let dx = led.x as i32 - kx as i32;
            let dy = led.y as i32 - ky as i32;
            let d = isqrt((dx * dx + dy * dy) as u32);
            let add = if led.key == Some(pos) {
                70
            } else if d < 24 {
                14
            } else {
                0
            };
            self.fx.heat[i] = self.fx.heat[i].saturating_add(add);
        }
    }

    fn step(&mut self, now: u32, settings: &Settings) {
        let dt = now.wrapping_sub(self.last).min(200);
        self.last = now;
        // Velocidade: 0..255 -> 0.25x .. ~2.25x
        self.anim_t = self.anim_t.wrapping_add(dt * (32 + settings.rgb_speed as u32) / 128);

        // Decaimento do calor (~8 s para apagar) e das estrelas.
        let heat_decay = (dt / 30) as u8;
        for h in self.fx.heat.iter_mut() {
            *h = h.saturating_sub(heat_decay);
        }
        let star_decay = (dt * (2 + settings.rgb_speed as u32 / 64) / 8).min(255) as u8;
        for s in self.fx.stars.iter_mut() {
            *s = s.saturating_sub(star_decay);
        }
        if self.rng.below(1000) < dt * 12 {
            let i = self.rng.below(LEDS as u32) as usize;
            self.fx.stars[i] = 255;
        }
        for r in self.fx.ripples.iter_mut() {
            if r.live && now.wrapping_sub(r.start) > 1200 {
                r.live = false;
            }
        }
    }

    /// Calcula as cores do quadro.
    pub fn render(&mut self, f: &Frame, out: &mut [Rgb; LEDS]) {
        let s = &f.state.settings;
        self.step(f.now, s);
        if !s.rgb_on {
            *out = [BLACK; LEDS];
            return;
        }

        let theme = theme::get(s.rgb_theme);
        let effect = Effect::from_index(s.rgb_effect);
        let ctx = Ctx { t: self.anim_t, now: f.now, side: self.side, theme, settings: s, state: &self.fx };

        let top = f.state.layers.highest();
        let layer_color = theme.layers[top as usize % theme.layers.len()];
        let layer_overlay = top != f.state.layers.default && layer_color != BLACK;
        let pulse = 170 + (sin8((f.now / 6) as u8) as u16 * 85 / 255) as u8;

        for (i, led) in self.leds.iter().enumerate() {
            let mut c = effects::render(effect, i, led, &ctx);

            // Sobreposição de camada: destaca as teclas que fazem algo nela.
            if layer_overlay {
                match led.key {
                    Some(k) if f.layer_keys & (1 << k.index()) != 0 => c = layer_color.scale(pulse),
                    Some(_) => c = c.scale(30),
                    None => c = layer_color.scale(pulse),
                }
            }

            // Modificadores segurados: teclas da coluna externa e polegares acendem.
            if f.state.mods != 0 && led.flags & flags::MODIFIER != 0 {
                c = c.lerp(theme.mods, 160);
            }

            // Tecla segurada nesta metade brilha um pouco mais.
            if let Some(k) = led.key
                && self.held & (1 << k.index()) != 0
            {
                c = c.lerp(Rgb::new(255, 255, 255), 90);
            }

            // Caps Lock: underglow pulsando na cor do tema.
            if f.state.caps_lock && led.flags & flags::UNDERGLOW != 0 {
                c = theme.caps.scale(pulse);
            }

            out[i] = gamma_rgb(c.scale(s.rgb_val));
        }

        limit_power(out);
    }
}

/// Reduz o brilho do quadro inteiro se passar do orçamento de corrente.
pub fn limit_power(out: &mut [Rgb; LEDS]) {
    let total: u32 = out.iter().map(|c| c.r as u32 + c.g as u32 + c.b as u32).sum();
    let budget = LEDS as u32 * 3 * POWER_LIMIT as u32;
    if total > budget {
        let f = (budget * 255 / total) as u8;
        for c in out.iter_mut() {
            *c = c.scale(f);
        }
    }
}

/// Aplica um comando de teclado (RGB_MOD, RGB_HUI...) às configurações.
pub fn apply_cmd(s: &mut Settings, cmd: RgbCmd) {
    const STEP: u8 = 16;
    let n = effects::EFFECTS.len() as u8;
    match cmd {
        RgbCmd::Toggle => s.rgb_on = !s.rgb_on,
        RgbCmd::NextEffect => s.rgb_effect = (s.rgb_effect + 1) % n,
        RgbCmd::PrevEffect => s.rgb_effect = (s.rgb_effect + n - 1) % n,
        RgbCmd::HueUp => s.rgb_hue = s.rgb_hue.wrapping_add(STEP),
        RgbCmd::HueDown => s.rgb_hue = s.rgb_hue.wrapping_sub(STEP),
        RgbCmd::SatUp => s.rgb_sat = s.rgb_sat.saturating_add(STEP),
        RgbCmd::SatDown => s.rgb_sat = s.rgb_sat.saturating_sub(STEP),
        RgbCmd::ValUp => s.rgb_val = s.rgb_val.saturating_add(STEP),
        RgbCmd::ValDown => s.rgb_val = s.rgb_val.saturating_sub(STEP),
        RgbCmd::SpeedUp => s.rgb_speed = s.rgb_speed.saturating_add(STEP),
        RgbCmd::SpeedDown => s.rgb_speed = s.rgb_speed.saturating_sub(STEP),
        RgbCmd::NextTheme => s.rgb_theme = (s.rgb_theme + 1) % theme::THEMES.len() as u8,
    }
}

/// Nome do efeito atual (para mostrar no OLED).
pub fn effect_name(s: &Settings) -> &'static str {
    Effect::from_index(s.rgb_effect).name()
}

pub fn theme_name(s: &Settings) -> &'static str {
    theme::get(s.rgb_theme).name
}

/// Tema atual: (nome, posição na lista, quantidade de temas).
pub fn theme_info(s: &Settings) -> (&'static str, u8, u8) {
    let count = theme::THEMES.len() as u8;
    (theme_name(s), s.rgb_theme % count, count)
}
