//! # corne-oled
//!
//! Complemento de OLED: cenas animadas para o SSD1306 128x32 de cada metade.
//! Assim como o `corne-rgb`, não conhece o hardware: desenha num
//! [`FrameBuffer`] que o firmware manda pelo I2C (e o preview mostra no terminal).
//!
//! Cada lado tem sua própria lista de cenas (configurada no firmware) e sua
//! própria posição nela. `OLED_NEXT` avança nos dois lados; `OLED_NEXT_RIGHT`
//! e `OLED_NEXT_LEFT` só num deles.
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
/// Quanto tempo o aviso de troca de tema (ou de cena) fica na tela.
pub const POPUP_MS: u32 = 2000;

/// Item de uma lista (tema de RGB, cena) para o aviso de troca.
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

/// Fogo: grade de calor em blocos de 2x2 pixels (16x64 ou 64x16).
pub const FIRE_CELLS: usize = 16 * 64;

/// Faísca dos fogos de artifício (posição e velocidade em 1/16 de pixel).
#[derive(Clone, Copy, Default)]
pub struct Spark {
    pub x: i16,
    pub y: i16,
    pub vx: i16,
    pub vy: i16,
    /// Quadros de vida restantes (0 = livre).
    pub life: u8,
    /// Foguete subindo (explode no ponto mais alto).
    pub rocket: bool,
}

/// Pong automático. Coordenadas ao longo do lado comprido (`v`) e do curto
/// (`u`), em 1/16 de pixel, para funcionar em retrato e paisagem.
#[derive(Clone, Copy, Default)]
pub struct Pong {
    pub u: i32,
    pub v: i32,
    pub du: i32,
    pub dv: i32,
    /// Centro das raquetes (no eixo curto), em pixels × 16.
    pub pad: [i32; 2],
    /// Erro de mira de cada raquete nesta jogada (às vezes elas erram).
    pub aim: [i32; 2],
    pub score: [u8; 2],
    pub serve_at: u32,
}

/// Barras do equalizador.
pub const EQ_BARS: usize = 16;

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
    // --- cenas reativas ---
    /// Teclas apertadas desde o último quadro, e a coluna local da última.
    kicks: u8,
    kick_col: u8,
    /// Energia extra das teclas (fogo), decai com o tempo.
    pub boost: u16,
    pub heat: [u8; FIRE_CELLS],
    pub sparks: [Spark; 64],
    next_auto: u32,
    /// Nível e pico de cada barra (0..=1000).
    pub eq: [u16; EQ_BARS],
    pub eq_peak: [u16; EQ_BARS],
    pub pong: Pong,
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
            kicks: 0,
            kick_col: 0,
            boost: 0,
            heat: [0; FIRE_CELLS],
            sparks: [Spark::default(); 64],
            next_auto: 0,
            eq: [0; EQ_BARS],
            eq_peak: [0; EQ_BARS],
            pong: Pong::default(),
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

    /// Avança a animação da cena visível (as outras ficam paradas, sem gastar CPU).
    fn animate(&mut self, scene: Scene, now: u32, dt: u32, wpm: u8, dims: (i32, i32)) {
        let kicks = core::mem::take(&mut self.kicks) as u32;
        match scene {
            Scene::Fire => self.fire_step(kicks, wpm, dims),
            Scene::Fireworks => self.fireworks_step(kicks, now, dt, dims),
            Scene::Equalizer => self.eq_step(kicks, now, wpm),
            Scene::Pong => self.pong_step(kicks, now, dt, wpm, dims),
            _ => {}
        }
    }

    /// Fogo clássico: a base esquenta (mais ao digitar) e o calor sobe esfriando.
    fn fire_step(&mut self, kicks: u32, wpm: u8, (w, h): (i32, i32)) {
        let (cw, ch) = ((w / 2) as usize, (h / 2) as usize);
        self.boost = (self.boost + kicks as u16 * 60).min(600).saturating_sub(8);
        let intensity = (60 + wpm as u32 / 2 + self.boost as u32 / 3).min(230);
        // Base: brasas que piscam.
        for x in 0..cw {
            let r = self.rand();
            self.heat[(ch - 1) * cw + x] = if r % 256 < intensity { 255 } else { (r >> 8) as u8 % 96 };
        }
        // Telas compridas (retrato) esfriam menos por linha, para a chama subir alto.
        let cool = 3 + 480 / ch as u32;
        for y in 0..ch - 1 {
            for x in 0..cw {
                let below = (y + 1) * cw;
                let l = self.heat[below + if x > 0 { x - 1 } else { x }] as u32;
                let c = self.heat[below + x] as u32;
                let r = self.heat[below + if x + 1 < cw { x + 1 } else { x }] as u32;
                let far = self.heat[if y + 2 < ch { (y + 2) * cw + x } else { below + x }] as u32;
                let v = (l + c * 2 + r + far) / 5;
                let loss = self.rand() % cool;
                self.heat[y * cw + x] = v.saturating_sub(loss) as u8;
            }
        }
    }

    /// Lança um foguete da base da tela, na coluna `x`.
    fn launch_rocket(&mut self, x: i32, (_w, h): (i32, i32)) {
        // Sobe entre 45% e 80% da altura: v² = 2 · desaceleração · distância (em 1/16 px).
        let travel = h * (45 + (self.rand() % 35) as i32) / 100;
        let vy = -isqrt(32 * travel) - 2;
        let vx = (self.rand() % 7) as i16 - 3;
        if let Some(s) = self.sparks.iter_mut().find(|s| s.life == 0) {
            *s = Spark { x: (x * 16) as i16, y: (h * 16) as i16, vx, vy: vy as i16, life: 200, rocket: true };
        }
    }

    fn explode(&mut self, x: i16, y: i16, short: i32) {
        let n = 24u32;
        let phase = self.rand() as u8;
        let power = short / 3 + (self.rand() % (short as u32 / 4 + 1)) as i32;
        for i in 0..n {
            let a = ((i * 256 / n) as u8).wrapping_add(phase);
            let vx = sin_s(a.wrapping_add(64)) * power / 128;
            let vy = sin_s(a) * power / 128;
            let life = 26 + (self.rand() % 16) as u8;
            if let Some(s) = self.sparks.iter_mut().find(|s| s.life == 0) {
                *s = Spark { x, y, vx: vx as i16, vy: vy as i16, life, rocket: false };
            }
        }
    }

    /// Fogos de artifício: cada tecla lança um foguete; parado, lança sozinho.
    fn fireworks_step(&mut self, kicks: u32, now: u32, dt: u32, dims: (i32, i32)) {
        let (w, h) = dims;
        for _ in 0..kicks.min(3) {
            let x = 4 + self.kick_col as i32 * (w - 8) / 5 + (self.rand() % 5) as i32 - 2;
            self.launch_rocket(x, dims);
        }
        if now.wrapping_sub(self.next_auto) < 0x8000_0000 {
            self.next_auto = now.wrapping_add(700 + self.rand() % 900);
            let x = 4 + (self.rand() % (w as u32 - 8)) as i32;
            self.launch_rocket(x, dims);
        }
        let steps = (dt / 33).clamp(1, 4);
        for _ in 0..steps {
            for i in 0..self.sparks.len() {
                let mut s = self.sparks[i];
                if s.life == 0 {
                    continue;
                }
                s.x += s.vx;
                s.y += s.vy;
                s.vy += 1; // gravidade
                s.life -= 1;
                if s.rocket {
                    // Explode no ponto mais alto.
                    if s.vy >= -2 || s.life == 0 {
                        self.sparks[i].life = 0;
                        self.explode(s.x, s.y, w.min(h));
                        continue;
                    }
                } else if s.life.is_multiple_of(4) {
                    // O ar freia as faíscas.
                    s.vx -= s.vx.signum();
                    s.vy -= s.vy.signum();
                }
                let (px, py) = (s.x as i32 >> 4, s.y as i32 >> 4);
                if px < -8 || px > w + 8 || py > h + 8 {
                    s.life = 0;
                }
                self.sparks[i] = s;
            }
        }
    }

    /// Equalizador: as teclas fazem as barras pularem; parado, fica um "chiado".
    fn eq_step(&mut self, kicks: u32, now: u32, wpm: u8) {
        for _ in 0..kicks.min(4) {
            let center = (self.kick_col as usize * (EQ_BARS - 1)) / 5;
            for i in 0..EQ_BARS {
                let d = center.abs_diff(i) as u32;
                let bump = 900u32.saturating_sub(d * 160 + self.rand() % 200);
                self.eq[i] = (self.eq[i] as u32 + bump).min(1000) as u16;
            }
        }
        for i in 0..EQ_BARS {
            // Chiado de fundo, mais forte com o WPM.
            let wave = sin8((now / 9) as u8 ^ (i as u8).wrapping_mul(37)) as u32;
            let floor = (wave * (60 + wpm as u32 * 3) / 255) as u16;
            let decay = self.eq[i] / 10 + 6;
            self.eq[i] = self.eq[i].saturating_sub(decay).max(floor);
            if self.eq[i] >= self.eq_peak[i] {
                self.eq_peak[i] = self.eq[i];
            } else {
                self.eq_peak[i] = self.eq_peak[i].saturating_sub(12);
            }
        }
    }

    fn serve(&mut self, now: u32, toward: usize, (w, h): (i32, i32)) {
        let (short, long) = (w.min(h), w.max(h));
        let p = &mut self.pong;
        p.u = short * 8;
        p.v = long * 8;
        let r = hash(self.rng ^ now);
        p.du = (r % 25) as i32 - 12;
        p.dv = if toward == 0 { -20 } else { 20 };
        p.serve_at = now.wrapping_add(600);
    }

    /// Pong: duas raquetes jogando sozinhas; acelera com o WPM e a cada tecla.
    fn pong_step(&mut self, kicks: u32, now: u32, dt: u32, wpm: u8, dims: (i32, i32)) {
        let (w, h) = dims;
        let (short, long) = (w.min(h), w.max(h));
        if self.pong.dv == 0 {
            self.pong.pad = [short * 8, short * 8];
            self.serve(now, 1, dims);
        }
        if (now.wrapping_sub(self.pong.serve_at) as i32) < 0 {
            return;
        }
        let half_pad = pong_half_pad(short) * 16;
        let speed = 16 + wpm as i32 / 6 + kicks as i32 * 4;
        let steps = (dt / 33).clamp(1, 4) as i32;
        for _ in 0..steps {
            let p = &mut self.pong;
            let len = p.dv.abs().max(1);
            let before = p.v;
            p.u += p.du * speed / len;
            p.v += p.dv.signum() * speed;
            // Paredes laterais.
            if p.u < 16 || p.u > (short - 2) * 16 {
                p.du = -p.du;
                p.u = p.u.clamp(16, (short - 2) * 16);
            }
            // Raquetes: perseguem a bola com velocidade limitada (e um erro de mira).
            for k in 0..2 {
                let coming = (k == 0) == (p.dv < 0);
                let target = if coming { p.u + p.aim[k] } else { short * 8 };
                let d = (target - p.pad[k]).clamp(-22, 22);
                p.pad[k] = (p.pad[k] + d).clamp(half_pad, short * 16 - half_pad);
            }
            let wall = [3 * 16, (long - 4) * 16];
            let k = if p.dv < 0 { 0 } else { 1 };
            let crossing = if k == 0 { before > wall[0] && p.v <= wall[0] } else { before < wall[1] && p.v >= wall[1] };
            if crossing && (p.u - p.pad[k]).abs() <= half_pad + 8 {
                // Rebate: o ângulo depende de onde bateu na raquete.
                p.dv = -p.dv;
                p.du = ((p.u - p.pad[k]) * 24 / half_pad.max(1)).clamp(-18, 18);
                p.v = wall[k];
                let other = 1 - k;
                let r = hash(now ^ p.u as u32);
                // Às vezes a outra raquete mira errado e perde o ponto.
                p.aim[other] = if r.is_multiple_of(5) { (half_pad + 24) * if r & 8 != 0 { 1 } else { -1 } } else { 0 };
            } else if if k == 0 { p.v < 0 } else { p.v > long * 16 } {
                // Passou da raquete: ponto do outro lado.
                let winner = 1 - k;
                p.score[winner] = (p.score[winner] + 1) % 10;
                p.aim = [0, 0];
                self.serve(now, k, dims);
                return;
            }
        }
    }
}

/// Metade do tamanho da raquete do pong (px).
pub fn pong_half_pad(short: i32) -> i32 {
    short / 8 + 1
}

fn isqrt(n: i32) -> i32 {
    let mut r = 0;
    while (r + 1) * (r + 1) <= n {
        r += 1;
    }
    r
}

/// Seno com sinal: -128..=127.
pub fn sin_s(x: u8) -> i32 {
    sin8(x) as i32 - 128
}

pub struct OledEngine {
    side: Side,
    scenes: &'static [Scene],
    st: SceneState,
    last: u32,
    last_activity: u32,
    last_state: Option<KeyboardState>,
    /// Este lado mostra avisos (troca de tema ou de cena)?
    popups: bool,
    last_theme: Option<u8>,
    last_scene: Option<usize>,
    /// Aviso na tela: (início, título, item).
    popup: Option<(u32, &'static str, ThemeInfo)>,
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
            last_scene: None,
            popup: None,
        }
    }

    /// Liga os avisos neste lado: ao trocar o tema de RGB ou a cena, o nome
    /// aparece por [`POPUP_MS`] no lugar da cena.
    pub fn with_popups(mut self, on: bool) -> Self {
        self.popups = on;
        self
    }

    /// Posição da cena deste lado na lista.
    fn scene_index(&self, settings: &Settings) -> usize {
        let n = match self.side {
            Side::Left => settings.oled_scene,
            Side::Right => settings.oled_scene_right,
        };
        n as usize % self.scenes.len().max(1)
    }

    pub fn scene(&self, settings: &Settings) -> Scene {
        if self.scenes.is_empty() {
            return Scene::Status;
        }
        self.scenes[self.scene_index(settings)]
    }

    /// Avise quando uma tecla **desta metade** mudar.
    pub fn key_event(&mut self, pos: KeyPos, pressed: bool, now: u32) {
        let bit = 1u32 << (pos.row as u32 * 6 + (pos.col as u32 % 6));
        self.last_activity = now;
        if pressed {
            self.st.held_local |= bit;
            self.st.presses = self.st.presses.wrapping_add(1);
            self.st.kicks = self.st.kicks.saturating_add(1);
            // Coluna 0 = lado de fora, 5 = lado de dentro (igual nas duas metades).
            let col = pos.col % 6;
            self.st.kick_col = if pos.col < 6 { col } else { 5 - col };
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
                self.popup = Some((f.now, "TEMA", t));
            }
            self.last_theme = Some(t.index);
        }
        // Cena trocou: mostra o nome da nova.
        let idx = self.scene_index(&f.state.settings);
        if self.popups && self.scenes.len() > 1 && self.last_scene.is_some_and(|prev| prev != idx) {
            let info = ThemeInfo { name: self.scenes[idx].name(), index: idx as u8, count: self.scenes.len() as u8 };
            self.popup = Some((f.now, "CENA", info));
        }
        self.last_scene = Some(idx);
        fb.clear();
        if !f.state.settings.oled_on || f.now.wrapping_sub(self.last_activity) > IDLE_OFF_MS {
            return false;
        }
        let scene = self.scene(&f.state.settings);
        self.st.step(f.now, dt, f.state.wpm, fb.dims());
        self.st.animate(scene, f.now, dt, f.state.wpm, fb.dims());
        if let Some((start, title, t)) = self.popup {
            let elapsed = f.now.wrapping_sub(start);
            if elapsed < POPUP_MS {
                scenes::popup(fb, title, &t, POPUP_MS - elapsed);
                return true;
            }
            self.popup = None;
        }
        match scene {
            Scene::Status => scenes::status(fb, f),
            Scene::WpmGraph => scenes::wpm_graph(fb, f, &self.st),
            Scene::Starfield => scenes::starfield(fb, &self.st),
            Scene::Rain => scenes::rain(fb, &self.st),
            Scene::Life => scenes::life(fb, &self.st),
            Scene::Plasma => scenes::plasma(fb, f.now),
            Scene::Keys => scenes::keys(fb, &self.st),
            Scene::Fire => scenes::fire(fb, &self.st),
            Scene::Fireworks => scenes::fireworks(fb, &self.st),
            Scene::Equalizer => scenes::equalizer(fb, &self.st),
            Scene::Pong => scenes::pong(fb, &self.st),
            Scene::Tunnel => scenes::tunnel(fb, f.now, f.state.wpm),
            Scene::Metaballs => scenes::metaballs(fb, f.now),
            Scene::Helix => scenes::helix(fb, f.now, f.state.wpm),
            Scene::Scope => scenes::scope(fb, f.now),
        }
        let _ = self.side;
        true
    }
}

/// Aplica um comando de teclado (OLED_NEXT...) às configurações.
/// `counts`: quantas cenas cada lado tem (esquerdo, direito), para dar a volta
/// na lista certa nos dois sentidos.
pub fn apply_cmd(s: &mut Settings, cmd: OledCmd, counts: (usize, usize)) {
    let step = |v: &mut u8, n: usize, fwd: bool| {
        let n = n.clamp(1, 255) as u8;
        let cur = *v % n;
        *v = if fwd { (cur + 1) % n } else { (cur + n - 1) % n };
    };
    let (l, r) = counts;
    match cmd {
        OledCmd::Toggle => s.oled_on = !s.oled_on,
        OledCmd::NextScene => {
            step(&mut s.oled_scene, l, true);
            step(&mut s.oled_scene_right, r, true);
        }
        OledCmd::PrevScene => {
            step(&mut s.oled_scene, l, false);
            step(&mut s.oled_scene_right, r, false);
        }
        OledCmd::NextLeft => step(&mut s.oled_scene, l, true),
        OledCmd::PrevLeft => step(&mut s.oled_scene, l, false),
        OledCmd::NextRight => step(&mut s.oled_scene_right, r, true),
        OledCmd::PrevRight => step(&mut s.oled_scene_right, r, false),
    }
}
