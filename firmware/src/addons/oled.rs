//! Complemento de OLED no hardware: driver mínimo do SSD1306 128x32 por I2C
//! assíncrono + cenas do `corne-oled` a ~30 fps.

use corne_core::layout::Side;
#[cfg(feature = "rgb")]
use corne_oled::ThemeInfo;
use corne_oled::framebuffer::BYTES;
use corne_oled::{FrameBuffer, OledEngine, OledFrame};
use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::I2C1;
use embassy_time::{Duration, Ticker, Timer};

use crate::keymap::LAYER_NAMES;
use crate::shared::{self, ADDON_KEYS};
use crate::user_config;

const ADDR: u16 = 0x3C;

/// Sequência de inicialização do SSD1306 128x32.
const INIT: &[u8] = &[
    0x00, // fluxo de comandos
    0xAE, // tela desligada
    0xD5, 0x80, // clock
    0xA8, 0x1F, // multiplex: 32 linhas
    0xD3, 0x00, // offset
    0x40, // linha inicial 0
    0x8D, 0x14, // charge pump ligado
    0x20, 0x00, // endereçamento horizontal
    0xA1, // remap de segmentos
    0xC8, // varredura COM invertida
    0xDA, 0x02, // pinos COM para 128x32
    0x81, 0x8F, // contraste
    0xD9, 0xF1, // pré-carga
    0xDB, 0x40, // VCOMH
    0xA4, // mostra a RAM
    0xA6, // modo normal (não invertido)
    0x2E, // sem scroll
    0xAF, // tela ligada
];

/// Janela de escrita = tela inteira.
const WINDOW: &[u8] = &[0x00, 0x21, 0, 127, 0x22, 0, 3];

type Bus = I2c<'static, I2C1, Async>;

async fn command(i2c: &mut Bus, cmd: u8) -> bool {
    i2c.write_async(ADDR, [0x00, cmd]).await.is_ok()
}

async fn init(i2c: &mut Bus) -> bool {
    i2c.write_async(ADDR, INIT.iter().copied()).await.is_ok()
}

async fn flush(i2c: &mut Bus, fb: &FrameBuffer) -> bool {
    if i2c.write_async(ADDR, WINDOW.iter().copied()).await.is_err() {
        return false;
    }
    let data: &[u8; BYTES] = fb.raw();
    i2c.write_async(ADDR, core::iter::once(0x40).chain(data.iter().copied())).await.is_ok()
}

#[embassy_executor::task]
pub async fn oled_task(mut i2c: Bus, side: Side) {
    let mut engine = OledEngine::new(side, user_config::oled::SCENES).with_popups(user_config::oled::POPUPS);
    let mut fb = FrameBuffer::new(user_config::oled::ORIENTATION);
    let mut keys = ADDON_KEYS.subscriber().unwrap();

    // Espera a tela ligar; se não houver tela, tenta de novo devagar.
    Timer::after_millis(50).await;
    while !init(&mut i2c).await {
        Timer::after_secs(2).await;
    }

    let mut ticker = Ticker::every(Duration::from_millis(33));
    let mut display_on = true;
    loop {
        let now = shared::now_ms();
        while let Some((pos, pressed)) = keys.try_next_message_pure() {
            engine.key_event(pos, pressed, now);
        }
        let state = shared::state();

        #[cfg(feature = "rgb")]
        let (rgb_effect, rgb_theme) = {
            let (name, index, count) = corne_rgb::theme_info(&state.settings);
            (Some(corne_rgb::effect_name(&state.settings)), Some(ThemeInfo { name, index, count }))
        };
        #[cfg(not(feature = "rgb"))]
        let (rgb_effect, rgb_theme) = (None, None);

        let frame = OledFrame { state: &state, now, layer_names: &LAYER_NAMES, rgb_effect, rgb_theme };
        let visible = engine.render(&frame, &mut fb);

        if visible != display_on {
            display_on = visible;
            command(&mut i2c, if visible { 0xAF } else { 0xAE }).await;
        }
        if visible && !flush(&mut i2c, &fb).await {
            // Erro no barramento (tela solta?): reinicia a tela.
            Timer::after_millis(500).await;
            init(&mut i2c).await;
        }
        ticker.next().await;
    }
}
