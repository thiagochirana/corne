//! Cenas do OLED. Cada cena desenha um quadro inteiro no framebuffer.
//! Funcionam tanto em retrato (32x128) quanto em paisagem (128x32).

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_4X6, FONT_5X8, FONT_6X10, FONT_9X15_BOLD};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};

use crate::framebuffer::FrameBuffer;
use crate::{EQ_BARS, OledFrame, POPUP_MS as OLED_POPUP_MS, SceneState, ThemeInfo, pong_half_pad, sin_s};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scene {
    /// Camada, modificadores, Caps Lock, WPM e efeito de RGB.
    Status,
    /// Gráfico rolando do WPM.
    WpmGraph,
    /// Campo de estrelas em 3D; acelera com o WPM.
    Starfield,
    /// "Chuva digital"; acelera com o WPM.
    Rain,
    /// Jogo da Vida; cada tecla apertada semeia células.
    Life,
    /// Plasma em tons de cinza simulados por dithering.
    Plasma,
    /// Mostra as teclas desta metade sendo apertadas.
    Keys,
    /// Fogo subindo da base; cresce ao digitar.
    Fire,
    /// Fogos de artifício: cada tecla lança um foguete.
    Fireworks,
    /// Equalizador: as barras pulam com as teclas.
    Equalizer,
    /// Pong jogando sozinho; acelera com o WPM.
    Pong,
    /// Túnel 3D em wireframe; acelera com o WPM.
    Tunnel,
    /// Bolhas de "lava" que se fundem (metaballs).
    Metaballs,
    /// Hélice de DNA girando.
    Helix,
    /// Osciloscópio desenhando curvas de Lissajous.
    Scope,
}

impl Scene {
    /// Todas as cenas, na ordem da documentação.
    pub const ALL: [Scene; 15] = [
        Scene::Status,
        Scene::WpmGraph,
        Scene::Keys,
        Scene::Starfield,
        Scene::Rain,
        Scene::Life,
        Scene::Plasma,
        Scene::Fire,
        Scene::Fireworks,
        Scene::Equalizer,
        Scene::Pong,
        Scene::Tunnel,
        Scene::Metaballs,
        Scene::Helix,
        Scene::Scope,
    ];

    /// Procura pelo nome (maiúsculas ou minúsculas).
    pub fn from_name(s: &str) -> Option<Scene> {
        Self::ALL.iter().copied().find(|sc| sc.name().eq_ignore_ascii_case(s))
    }

    pub fn name(self) -> &'static str {
        match self {
            Scene::Status => "Status",
            Scene::WpmGraph => "WPM",
            Scene::Starfield => "Stars",
            Scene::Rain => "Rain",
            Scene::Life => "Life",
            Scene::Plasma => "Plasma",
            Scene::Keys => "Keys",
            Scene::Fire => "Fire",
            Scene::Fireworks => "Burst",
            Scene::Equalizer => "EQ",
            Scene::Pong => "Pong",
            Scene::Tunnel => "Warp",
            Scene::Metaballs => "Blobs",
            Scene::Helix => "DNA",
            Scene::Scope => "Scope",
        }
    }
}

const ON: BinaryColor = BinaryColor::On;

fn text(
    fb: &mut FrameBuffer,
    s: &str,
    x: i32,
    y: i32,
    font: &'static embedded_graphics::mono_font::MonoFont<'static>,
    align: Alignment,
) {
    let style = MonoTextStyle::new(font, ON);
    let ts = TextStyleBuilder::new().baseline(Baseline::Top).alignment(align).build();
    let _ = Text::with_text_style(s, Point::new(x, y), style, ts).draw(fb);
}

fn fill(fb: &mut FrameBuffer, x: i32, y: i32, w: u32, h: u32) {
    let _ = Rectangle::new(Point::new(x, y), Size::new(w, h)).into_styled(PrimitiveStyle::with_fill(ON)).draw(fb);
}

fn frame_rect(fb: &mut FrameBuffer, x: i32, y: i32, w: u32, h: u32) {
    let _ = Rectangle::new(Point::new(x, y), Size::new(w, h)).into_styled(PrimitiveStyle::with_stroke(ON, 1)).draw(fb);
}

/// Escreve um número curto sem alocar.
fn num(n: u32, buf: &mut [u8; 10]) -> &str {
    let mut i = buf.len();
    let mut v = n;
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 || i == 0 {
            break;
        }
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

/// Texto invertido (fundo aceso) para "badges" como C S A G.
fn badge(fb: &mut FrameBuffer, s: &str, x: i32, y: i32, on: bool) {
    if on {
        fill(fb, x, y, 7, 9);
        let style = MonoTextStyle::new(&FONT_5X8, BinaryColor::Off);
        let _ = Text::with_baseline(s, Point::new(x + 1, y + 1), style, Baseline::Top).draw(fb);
    } else {
        frame_rect(fb, x, y, 7, 9);
        let _ = Line::new(Point::new(x + 2, y + 4), Point::new(x + 4, y + 4))
            .into_styled(PrimitiveStyle::with_stroke(ON, 1))
            .draw(fb);
    }
}

pub fn status(fb: &mut FrameBuffer, f: &OledFrame) {
    let (w, _h) = fb.dims();
    let st = f.state;
    let top = st.layers.highest() as usize;
    let name = f.layer_names.get(top).copied().unwrap_or("?");
    let mut nb = [0u8; 10];
    let wpm = num(st.wpm as u32, &mut nb);
    let m = corne_core::keycode::Mods(st.mods);

    if w < 64 {
        // ---------- Retrato 32x128 ----------
        text(fb, "LAYER", 16, 0, &FONT_4X6, Alignment::Center);
        fill(fb, 0, 8, 32, 12);
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
        let ts = TextStyleBuilder::new().baseline(Baseline::Top).alignment(Alignment::Center).build();
        let short = &name[..name.len().min(5)];
        let _ = Text::with_text_style(short, Point::new(16, 9), style, ts).draw(fb);

        badge(fb, "C", 0, 26, m.ctrl());
        badge(fb, "S", 8, 26, m.shift());
        badge(fb, "A", 16, 26, m.alt());
        badge(fb, "G", 24, 26, m.gui());

        if st.caps_lock {
            fill(fb, 0, 39, 32, 9);
            let style = MonoTextStyle::new(&FONT_5X8, BinaryColor::Off);
            let _ = Text::with_text_style("CAPS", Point::new(16, 40), style, ts).draw(fb);
        }

        text(fb, "WPM", 16, 52, &FONT_4X6, Alignment::Center);
        text(fb, wpm, 16, 59, &FONT_6X10, Alignment::Center);
        // barra vertical de WPM (0..150)
        let bar = (st.wpm as u32).min(150) * 40 / 150;
        frame_rect(fb, 12, 72, 8, 42);
        fill(fb, 13, 113 - bar as i32, 6, bar);

        if st.link_errors > 0 {
            // Diagnóstico do cabo TRS: pacotes reenviados.
            let mut eb = [0u8; 10];
            text(fb, "E", 2, 118, &FONT_4X6, Alignment::Left);
            text(fb, num(st.link_errors as u32, &mut eb), 7, 118, &FONT_4X6, Alignment::Left);
        } else {
            let fx = f.rgb_effect.unwrap_or("");
            text(fb, &fx[..fx.len().min(6)], 16, 118, &FONT_4X6, Alignment::Center);
        }
    } else {
        // ---------- Paisagem 128x32 ----------
        fill(fb, 0, 0, 52, 12);
        let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::Off);
        let _ = Text::with_baseline(&name[..name.len().min(8)], Point::new(2, 1), style, Baseline::Top).draw(fb);
        badge(fb, "C", 56, 1, m.ctrl());
        badge(fb, "S", 64, 1, m.shift());
        badge(fb, "A", 72, 1, m.alt());
        badge(fb, "G", 80, 1, m.gui());
        if st.caps_lock {
            text(fb, "CAPS", 92, 2, &FONT_5X8, Alignment::Left);
        }
        text(fb, "WPM", 0, 16, &FONT_4X6, Alignment::Left);
        text(fb, wpm, 14, 14, &FONT_6X10, Alignment::Left);
        let bar = (st.wpm as u32).min(150) * 60 / 150;
        frame_rect(fb, 34, 16, 62, 7);
        fill(fb, 35, 17, bar, 5);
        if let Some(fx) = f.rgb_effect {
            text(fb, fx, 0, 25, &FONT_4X6, Alignment::Left);
        }
        if st.link_errors > 0 {
            // Diagnóstico do cabo TRS: pacotes reenviados.
            let mut eb = [0u8; 10];
            text(fb, "ERR", 92, 25, &FONT_4X6, Alignment::Left);
            text(fb, num(st.link_errors as u32, &mut eb), 106, 25, &FONT_4X6, Alignment::Left);
        }
    }
}

pub fn wpm_graph(fb: &mut FrameBuffer, f: &OledFrame, s: &SceneState) {
    let (w, h) = fb.dims();
    let mut nb = [0u8; 10];
    let n = num(f.state.wpm as u32, &mut nb);
    let header = 12;
    text(fb, n, 1, 1, &FONT_6X10, Alignment::Left);
    let _ = Line::new(Point::new(0, header), Point::new(w - 1, header))
        .into_styled(PrimitiveStyle::with_stroke(ON, 1))
        .draw(fb);
    let gh = h - header - 2;
    let len = s.wpm_hist.len();
    let cols = (w as usize).min(len);
    for i in 0..cols {
        let v = s.wpm_hist[(s.wpm_head + len - cols + i) % len] as i32;
        let bar = (v.min(150) * gh) / 150;
        let x = i as i32 + (w - cols as i32);
        if bar > 0 {
            let _ = Line::new(Point::new(x, h - 1), Point::new(x, h - 1 - bar))
                .into_styled(PrimitiveStyle::with_stroke(ON, 1))
                .draw(fb);
        }
    }
}

pub fn starfield(fb: &mut FrameBuffer, s: &SceneState) {
    let (w, h) = fb.dims();
    let (cx, cy) = (w / 2, h / 2);
    for st in s.stars.iter() {
        if st.z <= 0 {
            continue;
        }
        let sx = cx + st.x * 64 / st.z;
        let sy = cy + st.y * 64 / st.z;
        fb.set(sx, sy, true);
        if st.z < 40 {
            // estrelas próximas ficam maiores e deixam rastro
            fb.set(sx + 1, sy, true);
            let px = cx + st.x * 64 / (st.z + 6);
            let py = cy + st.y * 64 / (st.z + 6);
            let _ = Line::new(Point::new(px, py), Point::new(sx, sy))
                .into_styled(PrimitiveStyle::with_stroke(ON, 1))
                .draw(fb);
        }
    }
}

pub fn rain(fb: &mut FrameBuffer, s: &SceneState) {
    let (w, h) = fb.dims();
    let cols = (w / 4) as usize;
    for (i, d) in s.drops.iter().take(cols).enumerate() {
        let x = i as i32 * 4 + 1;
        let head = d.y >> 4;
        for k in 0..d.len as i32 {
            let y = head - k * 3;
            if y < 0 || y >= h {
                continue;
            }
            // "caracteres" de 3x2 que mudam com o hash da posição
            let g = crate::hash((i as u32) << 16 ^ (y as u32 / 3) ^ d.seed);
            let fade = k < 2 || g & 3 != 0;
            if fade {
                fb.set(x, y, g & 1 != 0 || k == 0);
                fb.set(x + 1, y, g & 2 != 0 || k == 0);
                fb.set(x, y + 1, g & 4 != 0);
                fb.set(x + 1, y + 1, g & 8 != 0 || k == 0);
            }
        }
    }
}

pub fn life(fb: &mut FrameBuffer, s: &SceneState) {
    let (w, h) = fb.dims();
    // grade em células de 2x2 pixels
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            if s.life_get(x, y, w / 2, h / 2) {
                fill(fb, x * 2, y * 2, 2, 2);
            }
        }
    }
}

/// Matriz de Bayer 4x4 para simular tons de cinza (dithering).
const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Acende o pixel se `v` (0..=255) passar do limiar de Bayer daquela posição.
fn dither(fb: &mut FrameBuffer, x: i32, y: i32, v: u32) {
    if v > BAYER[(y & 3) as usize][(x & 3) as usize] as u32 * 16 + 8 {
        fb.set(x, y, true);
    }
}

fn line(fb: &mut FrameBuffer, a: (i32, i32), b: (i32, i32)) {
    let _ =
        Line::new(Point::new(a.0, a.1), Point::new(b.0, b.1)).into_styled(PrimitiveStyle::with_stroke(ON, 1)).draw(fb);
}

pub fn plasma(fb: &mut FrameBuffer, t: u32) {
    let (w, h) = fb.dims();
    for y in 0..h {
        for x in 0..w {
            let a = crate::sin8((x * 4 + (t / 12) as i32) as u8) as u32;
            let b = crate::sin8((y * 5 - (t / 20) as i32) as u8) as u32;
            let c = crate::sin8(((x + y) * 3 + (t / 7) as i32) as u8) as u32;
            dither(fb, x, y, (a + b + c) / 3);
        }
    }
}

pub fn keys(fb: &mut FrameBuffer, s: &SceneState) {
    let (w, h) = fb.dims();
    // grade 6x4 com o desenho da metade (polegares na última linha)
    let portrait = w < 64;
    let (cell, gap) = if portrait { (4, 1) } else { (7, 2) };
    let total_w = 6 * (cell + gap);
    let ox = (w - total_w) / 2;
    let oy = if portrait { h / 2 - 12 } else { 4 };
    for r in 0..4 {
        for c in 0..6 {
            // polegares: lado interno da metade
            let thumb_missing = match s.side {
                corne_core::layout::Side::Left => c < 3,
                corne_core::layout::Side::Right => c > 2,
            };
            if r == 3 && thumb_missing {
                continue;
            }
            let x = ox + c * (cell + gap);
            let y = oy + r * (cell + gap) + if r == 3 { 2 } else { 0 };
            if s.held_local & (1 << (r * 6 + c)) != 0 {
                fill(fb, x, y, cell as u32, cell as u32);
            } else {
                frame_rect(fb, x, y, cell as u32, cell as u32);
            }
        }
    }
    let label = "KEYS";
    let mut nb = [0u8; 10];
    let total = num(s.presses, &mut nb);
    if portrait {
        text(fb, label, w / 2, 2, &FONT_4X6, Alignment::Center);
        text(fb, total, w / 2, h - 10, &FONT_4X6, Alignment::Center);
    } else {
        text(fb, total, w - 2, 24, &FONT_4X6, Alignment::Right);
    }
}

/// Texto apagado sobre fundo aceso (cabeçalho do aviso).
fn text_inverted(fb: &mut FrameBuffer, s: &str, x: i32, y: i32, align: Alignment) {
    let style = MonoTextStyle::new(&FONT_5X8, BinaryColor::Off);
    let ts = TextStyleBuilder::new().baseline(Baseline::Top).alignment(align).build();
    let _ = Text::with_text_style(s, Point::new(x, y), style, ts).draw(fb);
}

/// Aviso de troca (tema de RGB, cena): título, nome, posição na lista e uma
/// barra que esvazia até o aviso sumir. `left_ms`: tempo restante.
pub fn popup(fb: &mut FrameBuffer, title: &str, t: &ThemeInfo, left_ms: u32) {
    let (w, h) = fb.dims();
    let mut nb = [0u8; 10];
    let mut cb = [0u8; 10];
    let n = num(t.index as u32 + 1, &mut nb);
    let c = num(t.count as u32, &mut cb);
    let bar = |full: u32| left_ms * full / OLED_POPUP_MS;

    if w < 64 {
        // ---------- Retrato 32x128 ----------
        fill(fb, 0, 0, 32, 11);
        text_inverted(fb, title, 16, 2, Alignment::Center);
        // Nome em linhas de 5 letras (cabem 5 de 6 px em 32 px).
        let name = t.name.as_bytes();
        for (i, chunk) in name.chunks(5).take(5).enumerate() {
            let line = core::str::from_utf8(chunk).unwrap_or("?");
            text(fb, line, 16, 18 + i as i32 * 12, &FONT_6X10, Alignment::Center);
        }
        // "2/5"
        text(fb, n, 14, 84, &FONT_6X10, Alignment::Right);
        text(fb, "/", 16, 84, &FONT_6X10, Alignment::Center);
        text(fb, c, 18, 84, &FONT_6X10, Alignment::Left);
        // Um quadrado por tema, o atual aceso.
        if t.count <= 5 {
            let x0 = 16 - (t.count as i32 * 6 - 1) / 2;
            for i in 0..t.count as i32 {
                if i == t.index as i32 {
                    fill(fb, x0 + i * 6, 100, 5, 5);
                } else {
                    frame_rect(fb, x0 + i * 6, 100, 5, 5);
                }
            }
        }
        frame_rect(fb, 2, h - 8, 28, 5);
        fill(fb, 3, h - 7, bar(26), 3);
    } else {
        // ---------- Paisagem 128x32 ----------
        fill(fb, 0, 0, 128, 10);
        text_inverted(fb, title, 2, 1, Alignment::Left);
        text_inverted(fb, c, 125, 1, Alignment::Right);
        let cw = c.len() as i32 * 5;
        text_inverted(fb, "/", 125 - cw, 1, Alignment::Right);
        text_inverted(fb, n, 120 - cw, 1, Alignment::Right);
        text(fb, t.name, 64, 12, &FONT_9X15_BOLD, Alignment::Center);
        fill(fb, 0, h - 2, bar(128), 2);
    }
}

// ------------------------------------------------------------- cenas novas

/// Converte coordenadas (eixo curto `u`, eixo comprido `v`) em (x, y).
fn uv(fb: &FrameBuffer, u: i32, v: i32) -> (i32, i32) {
    let (w, h) = fb.dims();
    if w < h { (u, v) } else { (v, u) }
}

/// (eixo curto, eixo comprido) da tela.
fn short_long(fb: &FrameBuffer) -> (i32, i32) {
    let (w, h) = fb.dims();
    (w.min(h), w.max(h))
}

pub fn fire(fb: &mut FrameBuffer, s: &SceneState) {
    let (w, h) = fb.dims();
    let cw = w / 2;
    for y in 0..h {
        for x in 0..w {
            let v = s.heat[((y / 2) * cw + x / 2) as usize] as u32;
            dither(fb, x, y, v);
        }
    }
}

pub fn fireworks(fb: &mut FrameBuffer, s: &SceneState) {
    for (i, p) in s.sparks.iter().enumerate() {
        if p.life == 0 {
            continue;
        }
        let (x, y) = (p.x as i32 >> 4, p.y as i32 >> 4);
        if p.rocket {
            // Foguete com rastro.
            fb.set(x, y, true);
            fb.set(x, y + 1, true);
            fb.set(x, y + 3, i % 2 == 0);
        } else if p.life > 8 || (p.life as usize + i).is_multiple_of(2) {
            // Faíscas piscam no fim da vida.
            fb.set(x, y, true);
            if p.life > 26 {
                fb.set(x + 1, y, true);
            }
        }
    }
}

pub fn equalizer(fb: &mut FrameBuffer, s: &SceneState) {
    let (w, h) = fb.dims();
    // Retrato: 8 barras (pares juntos); paisagem: 16.
    let n = if w < h { EQ_BARS / 2 } else { EQ_BARS };
    let group = EQ_BARS / n;
    let bw = w / n as i32;
    let seg = if h > 64 { 4 } else { 3 }; // segmento + 1 px de espaço
    for i in 0..n {
        let level = (0..group).map(|k| s.eq[i * group + k]).max().unwrap_or(0) as i32;
        let peak = (0..group).map(|k| s.eq_peak[i * group + k]).max().unwrap_or(0) as i32;
        let x = i as i32 * bw;
        let top = h - 1 - level * (h - 3) / 1000;
        let mut y = h - seg;
        while y >= top {
            fill(fb, x, y, (bw - 1) as u32, (seg - 1) as u32);
            y -= seg;
        }
        let py = h - 2 - peak * (h - 3) / 1000;
        line(fb, (x, py), (x + bw - 2, py));
    }
}

pub fn pong(fb: &mut FrameBuffer, s: &SceneState) {
    let (short, long) = short_long(fb);
    let p = &s.pong;
    // Rede no meio.
    for u in (0..short).step_by(4) {
        let (x, y) = uv(fb, u, long / 2);
        fb.set(x, y, true);
        let (x, y) = uv(fb, u + 1, long / 2);
        fb.set(x, y, true);
    }
    // Raquetes (2 px de espessura) nas pontas do lado comprido.
    let half = pong_half_pad(short);
    for (k, v) in [(0usize, 1), (1, long - 3)] {
        let c = p.pad[k] >> 4;
        for u in c - half..=c + half {
            for dv in 0..2 {
                let (x, y) = uv(fb, u, v + dv);
                fb.set(x, y, true);
            }
        }
    }
    // Bola 2x2.
    let (bu, bv) = (p.u >> 4, p.v >> 4);
    for (du, dv) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let (x, y) = uv(fb, bu + du, bv + dv);
        fb.set(x, y, true);
    }
    // Placar perto da rede.
    let digit = |n: u8| -> &'static str { ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"][n as usize % 10] };
    let (w, _) = fb.dims();
    if w < 64 {
        text(fb, digit(p.score[0]), short - 6, long / 2 - 8, &FONT_4X6, Alignment::Left);
        text(fb, digit(p.score[1]), short - 6, long / 2 + 3, &FONT_4X6, Alignment::Left);
    } else {
        text(fb, digit(p.score[0]), long / 2 - 7, 1, &FONT_4X6, Alignment::Left);
        text(fb, digit(p.score[1]), long / 2 + 4, 1, &FONT_4X6, Alignment::Left);
    }
}

pub fn tunnel(fb: &mut FrameBuffer, now: u32, wpm: u8) {
    const RINGS: usize = 7;
    let (w, h) = fb.dims();
    let (cx, cy) = (w / 2, h / 2);
    let travel = now.wrapping_mul(1 + wpm as u32 / 25) / 6;
    // Profundidade de cada anel (0 = longe, 255 = perto), do mais longe ao mais perto.
    let mut zs = [0u32; RINGS];
    for (i, z) in zs.iter_mut().enumerate() {
        *z = (travel + i as u32 * 256 / RINGS as u32) % 256;
    }
    zs.sort_unstable();
    let mut prev: Option<[(i32, i32); 4]> = None;
    for z in zs {
        let scale = (z * z / 255) as i32; // perspectiva
        let hw = 1 + (w / 2 + 4) * scale / 255;
        let hh = 1 + (h / 2 + 4) * scale / 255;
        // Anéis distantes saem do centro: o túnel faz curvas.
        let far = 255 - z as i32;
        let ox = sin_s((now / 23) as u8) * (w / 3) / 128 * far / 255;
        let oy = sin_s((now / 31) as u8 ^ 0x40) * (h / 3) / 128 * far / 255;
        let (x0, y0, x1, y1) = (cx + ox - hw, cy + oy - hh, cx + ox + hw, cy + oy + hh);
        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
        for k in 0..4 {
            line(fb, corners[k], corners[(k + 1) % 4]);
        }
        if let Some(p) = prev {
            for k in 0..4 {
                line(fb, p[k], corners[k]);
            }
        }
        prev = Some(corners);
    }
}

pub fn metaballs(fb: &mut FrameBuffer, now: u32) {
    let (w, h) = fb.dims();
    let short = w.min(h);
    // Três bolhas em órbitas diferentes: (raio, velocidades, fase).
    let balls = [(short / 4 + 1, 17, 23, 0u8), (short / 5 + 1, 29, 13, 90), (short / 6 + 1, 11, 31, 170)];
    let mut pos = [(0i32, 0i32, 0i32); 3];
    for (i, &(r, a, b, ph)) in balls.iter().enumerate() {
        let x = w / 2 + sin_s(((now / a) as u8).wrapping_add(ph)) * (w / 2 - r).max(2) / 128;
        let y = h / 2 + sin_s(((now / b) as u8).wrapping_add(ph ^ 0x40)) * (h / 2 - r).max(2) / 128;
        pos[i] = (x, y, r * r * 256);
    }
    // Campo calculado em blocos de 2x2 (metade do custo de CPU por eixo).
    for by in (0..h).step_by(2) {
        for bx in (0..w).step_by(2) {
            let mut sum = 0i32;
            for &(x, y, r2) in &pos {
                let (dx, dy) = (bx + 1 - x, by + 1 - y);
                sum += r2 / (dx * dx + dy * dy + 1);
            }
            if sum >= 256 {
                fill(fb, bx, by, 2, 2);
            } else if sum > 128 {
                // Brilho em volta das bolhas.
                let v = ((sum - 128) * 2) as u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    dither(fb, bx + dx, by + dy, v);
                }
            }
        }
    }
}

pub fn helix(fb: &mut FrameBuffer, now: u32, wpm: u8) {
    let (short, long) = short_long(fb);
    let c = short / 2;
    let amp = short / 2 - 3;
    let t = now.wrapping_mul(1 + wpm as u32 / 30) / 7;
    for v in 0..long {
        let a = ((v * 5) as u32).wrapping_add(t) as u8;
        let off = sin_s(a) * amp / 128;
        let (u1, u2) = (c + off, c - off);
        // A fita da frente é grossa; a de trás, pontilhada.
        let front1 = sin_s(a.wrapping_add(64)) > 0;
        let (front, back) = if front1 { (u1, u2) } else { (u2, u1) };
        for du in 0..2 {
            let (x, y) = uv(fb, front + du, v);
            fb.set(x, y, true);
        }
        if v % 2 == 0 {
            let (x, y) = uv(fb, back, v);
            fb.set(x, y, true);
        }
        // Degraus entre as fitas.
        if v % 6 == 0 {
            let (lo, hi) = (u1.min(u2), u1.max(u2));
            for u in (lo..hi).step_by(2) {
                let (x, y) = uv(fb, u, v);
                fb.set(x, y, true);
            }
        }
    }
}

pub fn scope(fb: &mut FrameBuffer, now: u32) {
    const RATIOS: [(u32, u32); 6] = [(1, 2), (3, 2), (3, 4), (5, 4), (2, 3), (1, 3)];
    let (w, h) = fb.dims();
    // Grade do osciloscópio.
    for y in (0..h).step_by(8) {
        for x in (0..w).step_by(8) {
            fb.set(x, y, true);
        }
    }
    let (fa, fb_) = RATIOS[(now / 6000) as usize % RATIOS.len()];
    let phase = (now / 24) as u8;
    let (cx, cy) = (w / 2, h / 2);
    let point = |i: u32| {
        let a = (i * 2) as u8;
        let x = cx + sin_s(((a as u32 * fa) as u8).wrapping_add(phase)) * (w / 2 - 2) / 128;
        let y = cy + sin_s((a as u32 * fb_) as u8) * (h / 2 - 2) / 128;
        (x, y)
    };
    let mut prev = point(0);
    for i in 1..=128 {
        let p = point(i);
        line(fb, prev, p);
        prev = p;
    }
}
