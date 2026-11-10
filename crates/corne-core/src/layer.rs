//! Estado das camadas: uma camada padrão (base) + máscara de camadas ativas.

/// Máximo de camadas suportado pelo formato (`u16` de máscara).
pub const MAX_LAYERS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LayerState {
    /// Camada base atual (trocada com `DF(n)`).
    pub default: u8,
    /// Camadas ligadas por cima da base (bit n = camada n).
    pub active: u16,
}

impl LayerState {
    /// Máscara efetiva: camadas ativas + base.
    pub fn mask(&self) -> u16 {
        self.active | (1 << self.default)
    }

    pub fn is_on(&self, layer: u8) -> bool {
        self.mask() & (1 << layer) != 0
    }

    /// Camada mais alta ativa (a que "manda" visualmente, ex.: na cor do RGB).
    pub fn highest(&self) -> u8 {
        15 - self.mask().leading_zeros() as u8
    }

    pub fn on(&mut self, layer: u8) {
        self.active |= 1 << layer;
    }

    pub fn off(&mut self, layer: u8) {
        self.active &= !(1 << layer);
    }

    pub fn toggle(&mut self, layer: u8) {
        self.active ^= 1 << layer;
    }

    /// Ativa apenas esta camada (por cima da base).
    pub fn to(&mut self, layer: u8) {
        self.active = 1 << layer;
    }

    pub fn set_default(&mut self, layer: u8) {
        self.default = layer;
        self.active = 0;
    }

    /// Camadas ativas da mais alta para a mais baixa.
    pub fn iter_top_down(&self) -> impl Iterator<Item = u8> {
        let m = self.mask();
        (0..MAX_LAYERS as u8).rev().filter(move |l| m & (1 << l) != 0)
    }
}
