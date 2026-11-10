//! Complementos opcionais, ligados por features do Cargo (`rgb`, `oled`).
//! Cada um só lê o estado compartilhado e os eventos de tecla locais; o core
//! não depende deles.

#[cfg(feature = "oled")]
pub mod oled;
#[cfg(feature = "rgb")]
pub mod rgb;
