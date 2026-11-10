//! Debounce por tecla.
//!
//! - [`DebounceMode::Eager`] (padrão): a mudança vale **na primeira leitura** e
//!   a tecla fica travada por N ms, ignorando o repique do contato. No fim da
//!   trava, se a leitura estiver diferente, a nova mudança é emitida, então um
//!   toque mais curto que a trava não se perde. Latência ~0 ms.
//! - [`DebounceMode::Defer`]: a mudança só vale depois de N ms estável. Mais
//!   imune a ruído elétrico, mas soma N ms em todo toque.

use crate::layout::{HALF_COLS, HALF_ROWS};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DebounceMode {
    Eager,
    Defer,
}

pub struct Debouncer {
    stable: [[bool; HALF_COLS]; HALF_ROWS],
    /// Eager: início da trava. Defer: início da leitura diferente.
    since: [[Option<u32>; HALF_COLS]; HALF_ROWS],
    ms: u32,
    mode: DebounceMode,
}

impl Debouncer {
    pub const fn new(mode: DebounceMode, ms: u32) -> Self {
        Debouncer { stable: [[false; HALF_COLS]; HALF_ROWS], since: [[None; HALF_COLS]; HALF_ROWS], ms, mode }
    }

    /// Recebe uma leitura crua da matriz e chama `on_change(row, col, pressed)`
    /// para cada tecla cuja mudança se confirmou.
    pub fn update(&mut self, raw: &[[bool; HALF_COLS]; HALF_ROWS], now: u32, mut on_change: impl FnMut(u8, u8, bool)) {
        for r in 0..HALF_ROWS {
            for c in 0..HALF_COLS {
                let changed = match self.mode {
                    DebounceMode::Eager => self.eager(r, c, raw[r][c], now),
                    DebounceMode::Defer => self.defer(r, c, raw[r][c], now),
                };
                if changed {
                    on_change(r as u8, c as u8, raw[r][c]);
                }
            }
        }
    }

    fn eager(&mut self, r: usize, c: usize, raw: bool, now: u32) -> bool {
        if let Some(t) = self.since[r][c] {
            if now.wrapping_sub(t) < self.ms {
                return false;
            }
            self.since[r][c] = None;
        }
        if raw == self.stable[r][c] {
            return false;
        }
        self.stable[r][c] = raw;
        self.since[r][c] = Some(now);
        true
    }

    fn defer(&mut self, r: usize, c: usize, raw: bool, now: u32) -> bool {
        if raw == self.stable[r][c] {
            self.since[r][c] = None;
            return false;
        }
        match self.since[r][c] {
            None => {
                self.since[r][c] = Some(now);
                false
            }
            Some(t) if now.wrapping_sub(t) >= self.ms => {
                self.stable[r][c] = raw;
                self.since[r][c] = None;
                true
            }
            _ => false,
        }
    }
}
