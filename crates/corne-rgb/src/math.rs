//! Funções rápidas em inteiros para animação: seno, ruído, raiz, aleatório.

/// Seno de 8 bits: entrada 0..=255 (uma volta), saída 0..=255 (centro 128).
pub const fn sin8(x: u8) -> u8 {
    // Quarto de onda tabelado (64 amostras de 0 a 127).
    const Q: [u8; 64] = [
        0, 3, 6, 9, 12, 16, 19, 22, 25, 28, 31, 34, 37, 40, 43, 46, 49, 51, 54, 57, 60, 63, 65, 68, 71, 73, 76, 78, 81,
        83, 85, 88, 90, 92, 94, 96, 98, 100, 102, 104, 106, 107, 109, 111, 112, 113, 115, 116, 117, 118, 120, 121, 122,
        122, 123, 124, 125, 125, 126, 126, 126, 127, 127, 127,
    ];
    let i = x & 0x3F;
    let v = match x >> 6 {
        0 => Q[i as usize],
        1 => Q[63 - i as usize],
        2 => 0u8.wrapping_sub(Q[i as usize]),
        _ => 0u8.wrapping_sub(Q[63 - i as usize]),
    };
    (v as i8 as i16 + 128) as u8
}

pub const fn cos8(x: u8) -> u8 {
    sin8(x.wrapping_add(64))
}

/// Onda triangular 0..255..0.
pub const fn tri8(x: u8) -> u8 {
    if x < 128 { x << 1 } else { (255 - x) << 1 }
}

/// Raiz quadrada inteira.
pub const fn isqrt(n: u32) -> u32 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Hash inteiro rápido (para ruído e brilhos aleatórios determinísticos).
pub const fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Ruído de valor 2D suave (0..=255). `x`, `y` em 1/256 de célula.
pub fn noise2(x: u32, y: u32) -> u8 {
    let (xi, yi) = (x >> 8, y >> 8);
    let (xf, yf) = ((x & 0xFF) as i32, (y & 0xFF) as i32);
    let h = |a: u32, b: u32| (hash(a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B)) & 0xFF) as i32;
    // suavização (smoothstep) em inteiros
    let s = |t: i32| (t * t * (768 - 2 * t)) >> 16;
    let (sx, sy) = (s(xf), s(yf));
    let lerp = |a: i32, b: i32, t: i32| a + (((b - a) * t) >> 8);
    let top = lerp(h(xi, yi), h(xi + 1, yi), sx);
    let bot = lerp(h(xi, yi + 1), h(xi + 1, yi + 1), sx);
    lerp(top, bot, sy).clamp(0, 255) as u8
}

/// Gerador pseudoaleatório simples (xorshift32).
#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u32);

impl Rng {
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: u32) -> u32 {
        self.next_u32() % n.max(1)
    }
}
