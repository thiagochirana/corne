//! Varredura da matriz desta metade (a cada `SCAN_US`, 4 kHz por padrão) com debounce.

use corne_core::debounce::Debouncer;
use corne_core::engine::KeyEvent;
use corne_core::layout::{HALF_COLS, HALF_ROWS, KeyPos, Side};
use embassy_rp::gpio::{Input, Output};
use embassy_time::{Duration, Ticker};

use crate::shared::{self, ADDON_KEYS, LOCAL_KEYS};
use crate::user_config;

#[embassy_executor::task]
pub async fn matrix_task(mut rows: [Output<'static>; HALF_ROWS], cols: [Input<'static>; HALF_COLS], side: Side) {
    let mut debouncer = Debouncer::new(user_config::DEBOUNCE_MODE, user_config::DEBOUNCE_MS);
    let mut ticker = Ticker::every(Duration::from_micros(user_config::SCAN_US));
    let addons = ADDON_KEYS.immediate_publisher();

    loop {
        let mut raw = [[false; HALF_COLS]; HALF_ROWS];
        for (r, row) in rows.iter_mut().enumerate() {
            row.set_low();
            // ~2 µs para o sinal estabilizar antes de ler as colunas.
            cortex_m::asm::delay(250);
            for (c, col) in cols.iter().enumerate() {
                raw[r][c] = col.is_low();
            }
            row.set_high();
            // Espera as colunas voltarem a nível alto antes da próxima linha
            // (senão a tecla desta linha pode "aparecer" na seguinte).
            // Limite de ~10 µs caso alguma tecla esteja presa em outra linha.
            for _ in 0..40 {
                if cols.iter().all(|c| c.is_high()) {
                    break;
                }
                cortex_m::asm::delay(30);
            }
        }

        let now = shared::now_ms();
        debouncer.update(&raw, now, |r, c, pressed| {
            let pos = KeyPos::from_matrix(side, r, c);
            if !pos.exists() {
                return;
            }
            let _ = LOCAL_KEYS.try_send(KeyEvent { pos, pressed, time: now });
            addons.publish_immediate((pos, pressed));
        });

        ticker.next().await;
    }
}
