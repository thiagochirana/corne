//! Complemento de RGB no hardware: renderiza com `corne-rgb` a ~60 fps e manda
//! para os LEDs SK6812 pelo PIO1 + DMA.

use corne_core::layout::Side;
use corne_rgb::color::Rgb;
use corne_rgb::{Frame, LED_COUNT, RgbEngine};
use embassy_rp::peripherals::PIO1;
use embassy_rp::pio::Common;
use embassy_rp::pio_programs::ws2812::{Grb, PioWs2812};
use embassy_time::{Duration, Ticker};
use smart_leds::RGB8;

use crate::keymap::LAYER_MASKS;
use crate::shared::{self, ADDON_KEYS};

pub type Leds = PioWs2812<'static, PIO1, 0, LED_COUNT, Grb>;

#[embassy_executor::task]
pub async fn rgb_task(_common: Common<'static, PIO1>, mut leds: Leds, side: Side) {
    let mut engine = RgbEngine::new(side);
    let mut keys = ADDON_KEYS.subscriber().unwrap();
    let mut ticker = Ticker::every(Duration::from_millis(16));
    let mut colors = [Rgb::default(); LED_COUNT];
    let mut out = [RGB8::default(); LED_COUNT];

    loop {
        let now = shared::now_ms();
        while let Some((pos, pressed)) = keys.try_next_message_pure() {
            engine.key_event(pos, pressed, now);
        }
        let state = shared::state();
        let top = state.layers.highest() as usize;
        let layer_keys = LAYER_MASKS.get(top).copied().unwrap_or(0);
        engine.render(&Frame { state: &state, now, layer_keys }, &mut colors);
        for (o, c) in out.iter_mut().zip(colors.iter()) {
            *o = RGB8 { r: c.r, g: c.g, b: c.b };
        }
        leds.write(&out).await;
        ticker.next().await;
    }
}
