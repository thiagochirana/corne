//! # corne-core
//!
//! Núcleo do firmware, sem dependência de hardware (`no_std`, testável no PC):
//!
//! - [`keycode`], [`action`]: o que cada tecla pode fazer e sua codificação `u16`.
//! - [`layout`]: geometria do Corne 3x6+3 (posições lógicas, lados, coordenadas).
//! - [`layer`], [`keymap`]: sistema de camadas e keymap editável em RAM.
//! - [`keymap_file`]: parser do layout em texto (`keymap.kbd`).
//! - [`macros`]: macros (texto, teclas, esperas), lidas de `macros.kbd`.
//! - [`combo`]: combos (várias teclas juntas => uma ação), lidos de `combos.kbd`.
//! - [`host`]: layout do sistema (US, US internacional, ABNT2) para acentos e ç.
//! - [`engine`]: motor de teclas (camadas, tap-hold, relatório HID).
//! - [`debounce`], [`wpm`]: utilitários de matriz e estatística.
//! - [`state`]: estado compartilhado com os complementos (RGB, OLED).
//! - [`persist`]: formato das configurações gravadas na flash.
//! - [`split`]: protocolo entre as duas metades.
//! - [`protocol`]: configuração ao vivo pelo PC (base para um "Vial" próprio).
#![no_std]
// Loops por índice são mais legíveis em grades linha x coluna.
#![allow(clippy::needless_range_loop)]

pub mod action;
pub mod combo;
pub mod debounce;
pub mod engine;
pub mod hid;
pub mod host;
pub mod keycode;
pub mod keymap;
pub mod keymap_file;
pub mod layer;
pub mod layout;
pub mod macros;
pub mod persist;
pub mod protocol;
pub mod split;
pub mod state;
pub mod wpm;

pub use action::Action;
pub use engine::{Command, Engine, EngineConfig, KeyEvent, Output};
pub use host::Host;
pub use keymap::Keymap;
pub use layout::{KeyPos, Side};
pub use state::{KeyboardState, Settings};
