//! Estimativa de palavras por minuto (5 teclas = 1 palavra), janela deslizante.

const BUCKETS: usize = 10;
const BUCKET_MS: u32 = 500; // janela total de 5 s

pub struct Wpm {
    buckets: [u16; BUCKETS],
    current: usize,
    bucket_start: u32,
    smoothed: u16,
}

impl Wpm {
    pub const fn new() -> Self {
        Wpm { buckets: [0; BUCKETS], current: 0, bucket_start: 0, smoothed: 0 }
    }

    fn advance(&mut self, now: u32) {
        while now.wrapping_sub(self.bucket_start) >= BUCKET_MS {
            self.bucket_start = self.bucket_start.wrapping_add(BUCKET_MS);
            self.current = (self.current + 1) % BUCKETS;
            self.buckets[self.current] = 0;
            // Se ficou muito tempo parado, não precisa girar bucket por bucket.
            if now.wrapping_sub(self.bucket_start) > BUCKET_MS * BUCKETS as u32 {
                self.buckets = [0; BUCKETS];
                self.bucket_start = now;
            }
        }
    }

    pub fn key_pressed(&mut self, now: u32) {
        self.advance(now);
        self.buckets[self.current] = self.buckets[self.current].saturating_add(1);
    }

    /// Atualiza e devolve o WPM (0..=255).
    pub fn update(&mut self, now: u32) -> u8 {
        self.advance(now);
        let presses: u32 = self.buckets.iter().map(|&b| b as u32).sum();
        let window_s = (BUCKET_MS * BUCKETS as u32) / 1000;
        let raw = (presses * 60 / 5 / window_s) as u16;
        // Suavização exponencial simples (alpha = 1/4).
        self.smoothed = (self.smoothed * 3 + raw) / 4;
        self.smoothed.min(255) as u8
    }
}

impl Default for Wpm {
    fn default() -> Self {
        Self::new()
    }
}
