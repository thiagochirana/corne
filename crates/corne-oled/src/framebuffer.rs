//! Framebuffer 1 bit do SSD1306 128x32 com rotação.
//!
//! Os bytes ficam no formato nativo do controlador (4 "páginas" de 8 linhas,
//! cada byte é uma coluna de 8 pixels), então o firmware manda `raw()` direto
//! pelo I2C, sem conversão.

use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;

pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 32;
pub const BYTES: usize = WIDTH * HEIGHT / 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation {
    /// 128 de largura x 32 de altura.
    Landscape,
    /// 32 de largura x 128 de altura (como o QMK faz por padrão no Corne).
    Portrait,
    /// Igual a `Landscape`, girado 180°.
    LandscapeFlipped,
    /// Igual a `Portrait`, girado 180°.
    PortraitFlipped,
}

pub struct FrameBuffer {
    buf: [u8; BYTES],
    orientation: Orientation,
}

impl FrameBuffer {
    pub const fn new(orientation: Orientation) -> Self {
        FrameBuffer { buf: [0; BYTES], orientation }
    }

    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    pub fn raw(&self) -> &[u8; BYTES] {
        &self.buf
    }

    pub fn clear(&mut self) {
        self.buf = [0; BYTES];
    }

    /// Largura e altura lógicas (depois da rotação).
    pub fn dims(&self) -> (i32, i32) {
        match self.orientation {
            Orientation::Landscape | Orientation::LandscapeFlipped => (WIDTH as i32, HEIGHT as i32),
            _ => (HEIGHT as i32, WIDTH as i32),
        }
    }

    fn map(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        let (w, h) = self.dims();
        if x < 0 || y < 0 || x >= w || y >= h {
            return None;
        }
        let (px, py) = match self.orientation {
            Orientation::Landscape => (x, y),
            Orientation::LandscapeFlipped => (WIDTH as i32 - 1 - x, HEIGHT as i32 - 1 - y),
            Orientation::Portrait => (WIDTH as i32 - 1 - y, x),
            Orientation::PortraitFlipped => (y, HEIGHT as i32 - 1 - x),
        };
        Some((px as usize, py as usize))
    }

    pub fn set(&mut self, x: i32, y: i32, on: bool) {
        if let Some((px, py)) = self.map(x, y) {
            let idx = (py / 8) * WIDTH + px;
            let bit = 1 << (py % 8);
            if on {
                self.buf[idx] |= bit;
            } else {
                self.buf[idx] &= !bit;
            }
        }
    }

    pub fn get(&self, x: i32, y: i32) -> bool {
        match self.map(x, y) {
            Some((px, py)) => self.buf[(py / 8) * WIDTH + px] & (1 << (py % 8)) != 0,
            None => false,
        }
    }

    /// Pixel no espaço físico (sem rotação) — usado pelo preview.
    pub fn get_physical(&self, px: usize, py: usize) -> bool {
        self.buf[(py / 8) * WIDTH + px] & (1 << (py % 8)) != 0
    }

    pub fn invert(&mut self) {
        for b in self.buf.iter_mut() {
            *b = !*b;
        }
    }
}

impl OriginDimensions for FrameBuffer {
    fn size(&self) -> Size {
        let (w, h) = self.dims();
        Size::new(w as u32, h as u32)
    }
}

impl DrawTarget for FrameBuffer {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, c) in pixels {
            self.set(p.x, p.y, c.is_on());
        }
        Ok(())
    }
}
