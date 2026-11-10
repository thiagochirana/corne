//! Estado global do teclado, compartilhado com os complementos (RGB, OLED) e
//! sincronizado do lado central para o periférico.

use crate::layer::LayerState;

/// Configurações dos complementos. O core só guarda e transporta estes valores;
/// o significado de cada campo é definido pelo complemento dono dele.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settings {
    pub rgb_on: bool,
    pub rgb_effect: u8,
    pub rgb_theme: u8,
    pub rgb_hue: u8,
    pub rgb_sat: u8,
    pub rgb_val: u8,
    pub rgb_speed: u8,
    pub oled_on: bool,
    /// Cena do OLED esquerdo (posição na lista de cenas dele).
    pub oled_scene: u8,
    /// Cena do OLED direito.
    pub oled_scene_right: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Settings {
    pub const DEFAULT: Settings = Settings {
        rgb_on: true,
        rgb_effect: 0,
        rgb_theme: 0,
        rgb_hue: 0,
        rgb_sat: 255,
        rgb_val: 120,
        rgb_speed: 128,
        oled_on: true,
        oled_scene: 0,
        oled_scene_right: 0,
    };
}

/// Estado visível para os complementos.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyboardState {
    pub layers: LayerState,
    pub mods: u8,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub wpm: u8,
    /// Host USB conectado (no lado central).
    pub usb_connected: bool,
    /// Link com a outra metade funcionando.
    pub split_connected: bool,
    /// Pacotes do link perdidos e reenviados (diagnóstico, só no central; não vai pelo link).
    pub link_errors: u16,
    pub settings: Settings,
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardState {
    pub const fn new() -> Self {
        KeyboardState {
            layers: LayerState { default: 0, active: 0 },
            mods: 0,
            caps_lock: false,
            num_lock: false,
            wpm: 0,
            usb_connected: false,
            split_connected: false,
            link_errors: 0,
            settings: Settings::DEFAULT,
        }
    }

    pub const WIRE_LEN: usize = 16;

    /// Serializa para o link split.
    pub fn to_wire(&self) -> [u8; Self::WIRE_LEN] {
        let s = &self.settings;
        let flags = (self.caps_lock as u8)
            | (self.num_lock as u8) << 1
            | (self.usb_connected as u8) << 2
            | (self.split_connected as u8) << 3
            | (s.rgb_on as u8) << 4
            | (s.oled_on as u8) << 5;
        let a = self.layers.active.to_le_bytes();
        [
            a[0],
            a[1],
            self.layers.default,
            self.mods,
            flags,
            self.wpm,
            s.rgb_effect,
            s.rgb_theme,
            s.rgb_hue,
            s.rgb_sat,
            s.rgb_val,
            s.rgb_speed,
            s.oled_scene,
            s.oled_scene_right,
            0,
            0,
        ]
    }

    pub fn from_wire(b: &[u8; Self::WIRE_LEN]) -> Self {
        let flags = b[4];
        KeyboardState {
            layers: LayerState { active: u16::from_le_bytes([b[0], b[1]]), default: b[2] & 0x0F },
            mods: b[3],
            caps_lock: flags & 1 != 0,
            num_lock: flags & 2 != 0,
            usb_connected: flags & 4 != 0,
            split_connected: flags & 8 != 0,
            wpm: b[5],
            link_errors: 0,
            settings: Settings {
                rgb_on: flags & 16 != 0,
                oled_on: flags & 32 != 0,
                rgb_effect: b[6],
                rgb_theme: b[7],
                rgb_hue: b[8],
                rgb_sat: b[9],
                rgb_val: b[10],
                rgb_speed: b[11],
                oled_scene: b[12],
                oled_scene_right: b[13],
            },
        }
    }
}
