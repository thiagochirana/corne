//! # corne-oled
//!
//! Complemento de OLED: cenas animadas para o SSD1306 128x32 de cada metade.
//! Assim como o `corne-rgb`, não conhece o hardware: desenha num
//! [`FrameBuffer`] que o firmware manda pelo I2C (e o preview mostra no terminal).
//!
//! Cada lado tem sua própria lista de cenas (configurada no firmware), e o
//! comando `OLED_NEXT` avança nas duas ao mesmo tempo.
#![no_std]

pub mod framebuffer;
pub mod scenes;

use corne_core::action::OledCmd;
use corne_core::layout::{KeyPos, Side};
use corne_core::state::{KeyboardState, Settings};
pub use framebuffer::{FrameBuffer, Orientation};
pub use scenes::Scene;

/// Desliga a tela depois deste tempo sem digitar (evita burn-in do OLED).
pub const IDLE_OFF_MS: u32 = 5 * 60 * 1000;
/// Quanto tempo o aviso de troca de tema fica na tela.
pub const POPUP_MS: u32 = 2000;

/// Tema de RGB atual (para o aviso de troca).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ThemeInfo {
    pub name: &'static str,
    /// Posição na lista (0..count).
    pub index: u8,
    pub count: u8,
}

/// Entrada de um quadro.
pub struct OledFrame<'a> {
    pub state: &'a KeyboardState,
    pub now: u32,
    /// Nomes das camadas (do `keymap.kbd`).
    pub layer_names: &'a [&'a str],
    /// Nome do efeito de RGB atual, se o complemento de RGB estiver ativo.
    pub rgb_effect: Option<&'static str>,
    /// Tema de RGB atual, se o complemento de RGB estiver ativo.
    pub rgb_theme: Option<ThemeInfo>,
}

#[derive(Clone, Copy, Default)]
pub struct Star {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Default)]
pub struct Drop {
    /// Posição da "cabeça" em 1/16 de pixel.
    pub y: i32,
    pub speed: i32,
    pub len: u8,
    pub seed: u32,
}

const LIFE_W: usize = 64;
const LIFE_H: usize = 64;

/// Estado das animações.
pub struct SceneState {
    pub side: Side,
    pub stars: [Star; 48],
    pub drops: [Drop; 32],
    life: [[u64; LIFE_W / 64]; LIFE_H],
    life_next_step: u32,
    pub wpm_hist: [u8; 128],
    pub wpm_head: usize,
    wpm_next_sample: u32,
    /// Teclas seguradas nesta metade (bit = linha * 6 + coluna local).
    pub held_local: u32,
    pub presses: u32,
    rng: u32,
}

pub fn hash(x: u32) -> u32 {
    let mut x = x ^ (x >> 16);
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// Seno 8 bits (0..255 -> 0..255), aproximação parabólica suficiente para efeitos.
pub fn sin8(x: u8) -> u8 {
    let x = x as i32;
    let half = if x < 128 { x } else { x - 128 };
    let v = half * (128 - half) / 32; // 0..128
    (if x < 128 { 128 + v.min(127) } else { 128 - v.min(128) }) as u8
}

impl SceneState {
    pub fn new(side: Side) -> Self {
        let mut s = SceneState {
            side,
            stars: [Star::default(); 48],
            drops: [Drop::default(); 32],
            life: [[0; LIFE_W / 64]; LIFE_H],
            life_next_step: 0,
            wpm_hist: [0; 128],
            wpm_head: 0,
            wpm_next_sample: 0,
            held_local: 0,
            presses: 0,
            rng: 0xC0FFEE ^ side.as_u8() as u32,
        };
        for i in 0..s.stars.len() {
            s.stars[i] = s.new_star(true);
        }
        for i in 0..s.drops.len() {
            s.drops[i] = s.new_drop(true);
        }
        s
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn new_star(&mut self, any_depth: bool) -> Star {
        let x = (self.rand() % 128) as i32 - 64;
        let y = (self.rand() % 128) as i32 - 64;
        let z = if any_depth { (self.rand() % 255) as i32 + 1 } else { 255 };
        Star { x, y, z }
    }

    fn new_drop(&mut self, anywhere: bool) -> Drop {
        let y = if anywhere { (self.rand() % 2048) as i32 } else { -((self.rand() % 512) as i32) };
        Drop { y, speed: 8 + (self.rand() % 24) as i32, len: 3 + (self.rand() % 8) as u8, seed: self.rand() }
    }

    pub fn life_get(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        let x = x.rem_euclid(w.min(LIFE_W as i32)) as usize;
        let y = y.rem_euclid(h.min(LIFE_H as i32)) as usize;
        self.life[y][x / 64] & (1 << (x % 64)) != 0
    }

    fn life_set(&mut self, x: usize, y: usize, on: bool) {
        let (x, y) = (x % LIFE_W, y % LIFE_H);
        if on {
            self.life[y][x / 64] |= 1 << (x % 64);
        } else {
            self.life[y][x / 64] &= !(1 << (x % 64));
        }
    }

    fn life_step(&mut self, w: i32, h: i32) {
        let mut next = [[0u64; LIFE_W / 64]; LIFE_H];
        let mut alive = 0;
        for y in 0..h {
            for x in 0..w {
                let mut n = 0;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if (dx != 0 || dy != 0) && self.life_get(x + dx, y + dy, w, h) {
                            n += 1;
                        }
                    }
                }
                let on = self.life_get(x, y, w, h);
                if n == 3 || (on && n == 2) {
                    next[y as usize][x as usize / 64] |= 1 << (x as usize % 64);
                    alive += 1;
                }
            }
        }
        self.life = next;
        if alive < 6 {
            self.life_seed(w, h, 40);
        }
    }

    fn life_seed(&mut self, w: i32, h: i32, n: usize) {
        let cx = (self.rand() % w as u32) as usize;
        let cy = (self.rand() % h as u32) as usize;
        for _ in 0..n {
            let dx = (self.rand() % 7) as usize;
            let dy = (self.rand() % 7) as usize;
            self.life_set(cx + dx, cy + dy, true);
        }
    }

    fn step(&mut self, now: u32, dt: u32, wpm: u8, dims: (i32, i32)) {
        let speed = 1 + wpm as i32 / 12;
        // estrelas vêm na direção da tela
        for i in 0..self.stars.len() {
            self.stars[i].z -= speed * dt as i32 / 16 + 1;
            if self.stars[i].z <= 1 {
                self.stars[i] = self.new_star(false);
            }
        }
        // chuva
        let (_, h) = dims;
        for i in 0..self.drops.len() {
            self.drops[i].y += self.drops[i].speed * (2 + wpm as i32 / 20) * dt as i32 / 32;
            if (self.drops[i].y >> 4) - self.drops[i].len as i32 * 3 > h {
                self.drops[i] = self.new_drop(false);
            }
        }
        // vida
        if now.wrapping_sub(self.life_next_step) < 0x8000_0000 {
            self.life_next_step = now.wrapping_add(120);
            self.life_step(dims.0 / 2, dims.1 / 2);
        }
        // histórico de WPM: uma amostra a cada 500 ms
        if now.wrapping_sub(self.wpm_next_sample) < 0x8000_0000 {
            self.wpm_next_sample = now.wrapping_add(500);
            self.wpm_head = (self.wpm_head + 1) % self.wpm_hist.len();
            self.wpm_hist[self.wpm_head] = wpm;
        }
    }
}

pub struct OledEngine {
    side: Side,
    scenes: &'static [Scene],
    st: SceneState,
    last: u32,
    last_activity: u32,
    last_state: Option<KeyboardState>,
    /// Este lado mostra avisos (ex.: troca de tema)?
    popups: bool,
    last_theme: Option<u8>,
    /// Aviso na tela: (início, tema).
    popup: Option<(u32, ThemeInfo)>,
}

impl OledEngine {
    /// `scenes`: cenas deste lado, na ordem em que `OLED_NEXT` passa por elas.
    pub fn new(side: Side, scenes: &'static [Scene]) -> Self {
        OledEngine {
            side,
            scenes,
            st: SceneState::new(side),
            last: 0,
            last_activity: 0,
            last_state: None,
            popups: false,
            last_theme: None,
            popup: None,
        }
    }

    /// Liga os avisos neste lado: ao trocar o tema de RGB, o nome aparece por
    /// [`POPUP_MS`] no lugar da cena.
    pub fn with_popups(mut self, on: bool) -> Self {
        self.popups = on;
        self
    }

    pub fn scene(&self, settings: &Settings) -> Scene {
        if self.scenes.is_empty() {
            return Scene::Status;
        }
        self.scenes[settings.oled_scene as usize % self.scenes.len()]
    }

    /// Avise quando uma tecla **desta metade** mudar.
    pub fn key_event(&mut self, pos: KeyPos, pressed: bool, now: u32) {
        let bit = 1u32 << (pos.row as u32 * 6 + (pos.col as u32 % 6));
        self.last_activity = now;
        if pressed {
            self.st.held_local |= bit;
            self.st.presses = self.st.presses.wrapping_add(1);
            self.st.life_seed(64, 64, 12);
        } else {
            self.st.held_local &= !bit;
        }
    }

    /// Desenha o quadro. Devolve `false` se a tela deve ficar desligada.
    pub fn render(&mut self, f: &OledFrame, fb: &mut FrameBuffer) -> bool {
        let dt = f.now.wrapping_sub(self.last).min(200);
        self.last = f.now;
        // Mudanças de estado vindas da outra metade também contam como atividade.
        let mut st_cmp = *f.state;
        st_cmp.wpm = 0;
        if self.last_state != Some(st_cmp) {
            self.last_state = Some(st_cmp);
            self.last_activity = f.now;
        }
        // Tema trocou (não conta o primeiro quadro, ao ligar): mostra o aviso.
        if let Some(t) = f.rgb_theme {
            if self.popups && self.last_theme.is_some_and(|prev| prev != t.index) {
                self.popup = Some((f.now, t));
            }
            self.last_theme = Some(t.index);
        }
        fb.clear();
        if !f.state.settings.oled_on || f.now.wrapping_sub(self.last_activity) > IDLE_OFF_MS {
            return false;
        }
        self.st.step(f.now, dt, f.state.wpm, fb.dims());
        if let Some((start, t)) = self.popup {
            let elapsed = f.now.wrapping_sub(start);
            if elapsed < POPUP_MS {
                scenes::theme_popup(fb, &t, POPUP_MS - elapsed);
                return true;
            }
            self.popup = None;
        }
        match self.scene(&f.state.settings) {
            Scene::Status => scenes::status(fb, f),
            Scene::WpmGraph => scenes::wpm_graph(fb, f, &self.st),
            Scene::Starfield => scenes::starfield(fb, &self.st),
            Scene::Rain => scenes::rain(fb, &self.st),
            Scene::Life => scenes::life(fb, &self.st),
            Scene::Plasma => scenes::plasma(fb, f.now),
            Scene::Keys => scenes::keys(fb, &self.st),
        }
        let _ = self.side;
        true
    }
}

/// Aplica um comando de teclado (OLED_NEXT...) às configurações.
pub fn apply_cmd(s: &mut Settings, cmd: OledCmd) {
    match cmd {
        OledCmd::Toggle => s.oled_on = !s.oled_on,
        OledCmd::NextScene => s.oled_scene = s.oled_scene.wrapping_add(1),
        OledCmd::PrevScene => s.oled_scene = s.oled_scene.wrapping_sub(1),
    }
}
