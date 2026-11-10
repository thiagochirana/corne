//! Cores RGB/HSV e operações de mistura em inteiros (sem ponto flutuante,
//! o RP2040 não tem FPU).

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

const fn scale8(c: u8, f: u8) -> u8 {
    ((c as u16 * (f as u16 + 1)) >> 8) as u8
}

const fn lerp8(a: u8, b: u8, t: u8) -> u8 {
    let a = a as i32;
    let b = b as i32;
    (a + (((b - a) * t as i32) >> 8)) as u8
}

const fn max8(a: u8, b: u8) -> u8 {
    if a > b { a } else { b }
}

pub const BLACK: Rgb = Rgb::new(0, 0, 0);
pub const WHITE: Rgb = Rgb::new(255, 255, 255);

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// A partir de `0xRRGGBB`.
    pub const fn hex(v: u32) -> Self {
        Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// Multiplica o brilho por `f / 255`.
    pub const fn scale(self, f: u8) -> Self {
        Rgb::new(scale8(self.r, f), scale8(self.g, f), scale8(self.b, f))
    }

    /// Interpola de `self` até `other` (`t` = 0..=255).
    pub const fn lerp(self, other: Rgb, t: u8) -> Self {
        Rgb::new(lerp8(self.r, other.r, t), lerp8(self.g, other.g, t), lerp8(self.b, other.b, t))
    }

    /// Soma com saturação (para luz "aditiva", ex.: ondas sobre o fundo).
    pub const fn add(self, o: Rgb) -> Self {
        Rgb::new(self.r.saturating_add(o.r), self.g.saturating_add(o.g), self.b.saturating_add(o.b))
    }

    /// Mantém o maior valor de cada canal.
    pub const fn max(self, o: Rgb) -> Self {
        Rgb::new(max8(self.r, o.r), max8(self.g, o.g), max8(self.b, o.b))
    }

    pub const fn luma(self) -> u8 {
        ((self.r as u16 * 54 + self.g as u16 * 183 + self.b as u16 * 19) >> 8) as u8
    }
}

/// HSV com tudo em 0..=255 (matiz dá a volta completa em 256 passos).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Hsv {
    pub h: u8,
    pub s: u8,
    pub v: u8,
}

impl Hsv {
    pub const fn new(h: u8, s: u8, v: u8) -> Self {
        Hsv { h, s, v }
    }

    pub const fn to_rgb(self) -> Rgb {
        if self.s == 0 {
            return Rgb::new(self.v, self.v, self.v);
        }
        let region = self.h / 43;
        let rem = (self.h - region * 43) as u16 * 6;
        let v = self.v as u16;
        let s = self.s as u16;
        let p = ((v * (255 - s)) >> 8) as u8;
        let q = ((v * (255 - ((s * rem) >> 8))) >> 8) as u8;
        let t = ((v * (255 - ((s * (255 - rem)) >> 8))) >> 8) as u8;
        let v = self.v;
        match region {
            0 => Rgb::new(v, t, p),
            1 => Rgb::new(q, v, p),
            2 => Rgb::new(p, v, t),
            3 => Rgb::new(p, q, v),
            4 => Rgb::new(t, p, v),
            _ => Rgb::new(v, p, q),
        }
    }
}

/// Correção de gama aproximada (~2.0): faz degradês parecerem lineares ao olho.
pub const fn gamma(c: u8) -> u8 {
    let c = c as u16;
    ((c * c + c) >> 8) as u8
}

pub const fn gamma_rgb(c: Rgb) -> Rgb {
    Rgb::new(gamma(c.r), gamma(c.g), gamma(c.b))
}

/// Paleta com 4 paradas; amostrada continuamente (0..=255, dá a volta).
#[derive(Clone, Copy, Debug)]
pub struct Palette(pub [Rgb; 4]);

impl Palette {
    pub const fn sample(&self, t: u8) -> Rgb {
        let seg = (t >> 6) as usize; // 0..3
        let frac = (t & 0x3F) << 2; // 0..252
        let a = self.0[seg];
        let b = self.0[(seg + 1) & 3];
        a.lerp(b, frac)
    }
}
