//! Keymap em RAM: camadas de ações, editável em tempo real.
//!
//! Os dados vêm do layout padrão (gerado de `keymap.kbd` em tempo de
//! compilação) e podem ser alterados pelo protocolo de configuração.

use crate::action::Action;
use crate::layer::LayerState;
use crate::layout::{COLS, KeyPos, ROWS};

/// Máximo de camadas guardadas em RAM. 8 camadas x 48 posições x 2 bytes = 768 B.
pub const KEYMAP_LAYERS: usize = 8;

/// Uma camada codificada (formato persistido).
pub type EncodedLayer = [[u16; COLS]; ROWS];

#[derive(Clone)]
pub struct Keymap {
    layers: [[[Action; COLS]; ROWS]; KEYMAP_LAYERS],
    count: u8,
}

impl Keymap {
    pub const fn empty() -> Self {
        Keymap { layers: [[[Action::No; COLS]; ROWS]; KEYMAP_LAYERS], count: 1 }
    }

    /// Monta o keymap a partir de camadas codificadas em `u16`.
    pub fn from_encoded(data: &[EncodedLayer]) -> Self {
        let mut km = Keymap::empty();
        km.load_encoded(data);
        km
    }

    pub fn load_encoded(&mut self, data: &[EncodedLayer]) {
        let n = data.len().min(KEYMAP_LAYERS);
        *self = Keymap::empty();
        self.count = n.max(1) as u8;
        for (l, layer) in data.iter().take(n).enumerate() {
            for r in 0..ROWS {
                for c in 0..COLS {
                    self.layers[l][r][c] = Action::decode(layer[r][c]);
                }
            }
        }
    }

    pub fn layer_count(&self) -> u8 {
        self.count
    }

    pub fn get(&self, layer: u8, pos: KeyPos) -> Action {
        if (layer as usize) < KEYMAP_LAYERS && pos.exists() {
            self.layers[layer as usize][pos.row as usize][pos.col as usize]
        } else {
            Action::No
        }
    }

    /// Altera uma tecla. Devolve `false` se a posição/camada for inválida.
    pub fn set(&mut self, layer: u8, pos: KeyPos, action: Action) -> bool {
        if (layer as usize) >= KEYMAP_LAYERS || !pos.exists() {
            return false;
        }
        self.layers[layer as usize][pos.row as usize][pos.col as usize] = action;
        if layer >= self.count {
            self.count = layer + 1;
        }
        true
    }

    /// Resolve a ação efetiva de uma tecla, olhando as camadas ativas de cima
    /// para baixo e pulando as transparentes (`___`).
    pub fn resolve(&self, layers: &LayerState, pos: KeyPos) -> Action {
        for l in layers.iter_top_down() {
            if l >= self.count {
                continue;
            }
            match self.get(l, pos) {
                Action::Trans => continue,
                a => return a,
            }
        }
        Action::No
    }

    /// Máscara (bit = `KeyPos::index`) das teclas que fazem algo na camada,
    /// ou seja, que não são `___` nem `XXX`.
    pub fn layer_mask(&self, layer: u8) -> u64 {
        let mut m = 0u64;
        for pos in crate::layout::all_keys() {
            if !matches!(self.get(layer, pos), Action::Trans | Action::No) {
                m |= 1 << pos.index();
            }
        }
        m
    }

    /// Exporta uma camada no formato codificado.
    pub fn encode_layer(&self, layer: u8) -> EncodedLayer {
        let mut out = [[0u16; COLS]; ROWS];
        if (layer as usize) < KEYMAP_LAYERS {
            for r in 0..ROWS {
                for c in 0..COLS {
                    out[r][c] = self.layers[layer as usize][r][c].encode();
                }
            }
        }
        out
    }
}
