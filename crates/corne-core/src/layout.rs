//! Geometria do Corne 3x6+3: posições lógicas, lados e coordenadas físicas.
//!
//! O keymap usa uma grade lógica única de 4 linhas x 12 colunas:
//!
//! ```text
//!  col:  0  1  2  3  4  5 |  6  7  8  9 10 11
//!  row0  .  .  .  .  .  . |  .  .  .  .  .  .
//!  row1  .  .  .  .  .  . |  .  .  .  .  .  .
//!  row2  .  .  .  .  .  . |  .  .  .  .  .  .
//!  row3           .  .  . |  .  .  .            <- polegares (cols 3..=8)
//! ```

/// Linhas lógicas.
pub const ROWS: usize = 4;
/// Colunas lógicas (6 por metade).
pub const COLS: usize = 12;
/// Linhas da matriz de cada metade.
pub const HALF_ROWS: usize = 4;
/// Colunas da matriz de cada metade.
pub const HALF_COLS: usize = 6;
/// Teclas por metade (18 + 3 polegares).
pub const KEYS_PER_HALF: usize = 21;
/// Total de teclas.
pub const KEY_COUNT: usize = 42;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Posição lógica de uma tecla na grade 4x12.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct KeyPos {
    pub row: u8,
    pub col: u8,
}

impl KeyPos {
    pub const fn new(row: u8, col: u8) -> Self {
        KeyPos { row, col }
    }

    /// Converte uma posição da matriz física de uma metade em posição lógica.
    ///
    /// A PCB do Corne é reversível: na metade direita a coluna física 0 é a
    /// mais externa (à direita), por isso o espelhamento.
    pub const fn from_matrix(side: Side, row: u8, col: u8) -> Self {
        match side {
            Side::Left => KeyPos { row, col },
            Side::Right => KeyPos { row, col: (COLS as u8 - 1) - col },
        }
    }

    /// Se a posição corresponde a uma tecla que existe fisicamente.
    pub const fn exists(self) -> bool {
        if self.row as usize >= ROWS || self.col as usize >= COLS {
            return false;
        }
        self.row < 3 || (self.col >= 3 && self.col <= 8)
    }

    pub const fn side(self) -> Side {
        if (self.col as usize) < HALF_COLS { Side::Left } else { Side::Right }
    }

    /// Índice compacto 0..42 (útil para tabelas por tecla).
    pub const fn index(self) -> usize {
        if self.row < 3 { self.row as usize * COLS + self.col as usize } else { 36 + (self.col as usize - 3) }
    }

    /// Inverso de [`KeyPos::index`].
    pub const fn from_index(i: usize) -> Self {
        if i < 36 {
            KeyPos { row: (i / COLS) as u8, col: (i % COLS) as u8 }
        } else {
            KeyPos { row: 3, col: (i - 36 + 3) as u8 }
        }
    }

    /// Byte compacto usado no link split: bit7 livre, bits 4..6 linha, 0..3 coluna.
    pub const fn to_byte(self) -> u8 {
        ((self.row & 0x07) << 4) | (self.col & 0x0F)
    }
    pub const fn from_byte(b: u8) -> Self {
        KeyPos { row: (b >> 4) & 0x07, col: b & 0x0F }
    }

    /// Coordenadas físicas aproximadas no espaço 0..=223 x 0..=63 (como o QMK).
    /// Considera o escalonamento de colunas e a abertura dos polegares.
    pub fn xy(self) -> (u8, u8) {
        // Escalonamento (em 1/16 de tecla) das colunas 0..5 da metade esquerda.
        const STAGGER: [u8; 6] = [6, 6, 2, 0, 2, 4];
        let half_col = if (self.col as usize) < HALF_COLS { self.col } else { 11 - self.col };
        let left = (self.col as usize) < HALF_COLS;
        let (x_half, y) = if self.row < 3 {
            let x = half_col as u16 * 16 + 8;
            let y = self.row as u16 * 16 + STAGGER[half_col as usize] as u16 + 4;
            (x, y)
        } else {
            // polegares: cols 3,4,5 da metade (de fora para dentro)
            let x = match half_col {
                3 => 60,
                4 => 78,
                _ => 96,
            };
            let y = match half_col {
                3 => 56,
                4 => 58,
                _ => 62,
            };
            (x, y)
        };
        let x = if left { x_half } else { 223 - x_half };
        (x.min(223) as u8, y.min(63) as u8)
    }
}

/// Itera sobre todas as posições que existem fisicamente.
pub fn all_keys() -> impl Iterator<Item = KeyPos> {
    (0..KEY_COUNT).map(KeyPos::from_index)
}
