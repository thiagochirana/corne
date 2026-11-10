//! Cenas do OLED. Cada cena desenha um quadro inteiro no framebuffer.
//! Funcionam tanto em retrato (32x128) quanto em paisagem (128x32).

use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_4X6, FONT_5X8, FONT_6X10, FONT_9X15_BOLD};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::{Alignment, Baseline, Text, TextStyleBuilder};

use crate::framebuffer::FrameBuffer;
use crate::{OledFrame, POPUP_MS as OLED_POPUP_MS, SceneState, ThemeInfo};

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
}

impl Scene {
    pub fn name(self) -> &'static str {
        match self {
            Scene::Status => "Status",
            Scene::WpmGraph => "WPM",
            Scene::Starfield => "Stars",
            Scene::Rain => "Rain",
            Scene::Life => "Life",
            Scene::Plasma => "Plasma",
            Scene::Keys => "Keys",
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

pub fn plasma(fb: &mut FrameBuffer, t: u32) {
    const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    let (w, h) = fb.dims();
    for y in 0..h {
        for x in 0..w {
            let a = crate::sin8((x * 4 + (t / 12) as i32) as u8) as u32;
            let b = crate::sin8((y * 5 - (t / 20) as i32) as u8) as u32;
            let c = crate::sin8(((x + y) * 3 + (t / 7) as i32) as u8) as u32;
            let v = ((a + b + c) / 3) as u8;
            let thr = BAYER[(y & 3) as usize][(x & 3) as usize] * 16 + 8;
            if v > thr {
                fb.set(x, y, true);
            }
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

/// Aviso de troca de tema de RGB: nome, posição na lista e uma barra que
/// esvazia até o aviso sumir. `left_ms`: tempo restante.
pub fn theme_popup(fb: &mut FrameBuffer, t: &ThemeInfo, left_ms: u32) {
    let (w, h) = fb.dims();
    let mut nb = [0u8; 10];
    let mut cb = [0u8; 10];
    let n = num(t.index as u32 + 1, &mut nb);
    let c = num(t.count as u32, &mut cb);
    let bar = |full: u32| left_ms * full / OLED_POPUP_MS;

    if w < 64 {
        // ---------- Retrato 32x128 ----------
        fill(fb, 0, 0, 32, 11);
        text_inverted(fb, "TEMA", 16, 2, Alignment::Center);
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
        text_inverted(fb, "TEMA RGB", 2, 1, Alignment::Left);
        text_inverted(fb, c, 125, 1, Alignment::Right);
        let cw = c.len() as i32 * 5;
        text_inverted(fb, "/", 125 - cw, 1, Alignment::Right);
        text_inverted(fb, n, 120 - cw, 1, Alignment::Right);
        text(fb, t.name, 64, 12, &FONT_9X15_BOLD, Alignment::Center);
        fill(fb, 0, h - 2, bar(128), 2);
    }
}
