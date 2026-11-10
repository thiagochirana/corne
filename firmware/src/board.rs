//! Mapa de pinos: Corne (crkbd) V3 + Pro Micro RP2040.
//!
//! **Este é o único lugar para mudar pinos.**
//!
//! Os pinos do Corne são definidos em nomes do Pro Micro original (ATmega32U4).
//! A tradução para GPIO abaixo segue a pinagem "RP2040 Community Edition"
//! (Elite-Pi, Sea-Picro, a maioria dos "Pro Micro RP2040" genéricos), a mesma
//! do conversor `rp2040_ce` do QMK:
//!
//! | Função           | Pro Micro | GPIO  |
//! |------------------|-----------|-------|
//! | Linha 0..3       | D4 C6 D7 E6 | 4 5 6 7 |
//! | Coluna 0..5      | F4 F5 F6 F7 B1 B3 | 29 28 27 26 22 20 |
//! | Dados do TRS     | D2        | 1     |
//! | RGB (WS2812)     | D3        | 0     |
//! | OLED SDA         | D1        | 2 (I2C1) |
//! | OLED SCL         | D0        | 3 (I2C1) |
//!
//! Se a sua placa for de outra família (ex.: SparkFun Pro Micro RP2040, que tem
//! pinagem diferente), troque os `PIN_n` nos macros abaixo.

use embassy_rp::Peri;
use embassy_rp::gpio::AnyPin;

/// Diodos COL2ROW: linhas são saídas (ativas em nível baixo) e colunas são
/// entradas com pull-up.
pub struct MatrixPins {
    pub rows: [Peri<'static, AnyPin>; 4],
    pub cols: [Peri<'static, AnyPin>; 6],
}

macro_rules! matrix_pins {
    ($p:ident) => {
        $crate::board::MatrixPins {
            rows: [$p.PIN_4.into(), $p.PIN_5.into(), $p.PIN_6.into(), $p.PIN_7.into()],
            cols: [
                $p.PIN_29.into(),
                $p.PIN_28.into(),
                $p.PIN_27.into(),
                $p.PIN_26.into(),
                $p.PIN_22.into(),
                $p.PIN_20.into(),
            ],
        }
    };
}

/// Fio de dados do cabo TRS (link half-duplex entre as metades).
macro_rules! split_pin {
    ($p:ident) => {
        $p.PIN_1
    };
}

/// Entrada de dados dos LEDs.
#[allow(unused_macros)]
macro_rules! rgb_pin {
    ($p:ident) => {
        $p.PIN_0
    };
}

/// OLED: (periférico I2C, SCL, SDA).
#[allow(unused_macros)]
macro_rules! oled_i2c {
    ($p:ident) => {
        ($p.I2C1, $p.PIN_3, $p.PIN_2)
    };
}

pub(crate) use matrix_pins;
pub(crate) use split_pin;
#[allow(unused_imports)]
pub(crate) use {oled_i2c, rgb_pin};
