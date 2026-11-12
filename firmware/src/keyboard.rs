//! Tarefa do lado central: junta as teclas das duas metades, roda o motor de
//! camadas/tap-hold, manda relatórios para a USB e publica o estado para os
//! complementos e para a outra metade.

use corne_core::engine::{Command, Engine, Output};
use corne_core::keymap::Keymap;
use corne_core::protocol::{self, ConfigTarget};
use corne_core::state::Settings;
use corne_core::wpm::Wpm;
use embassy_futures::select::{Either4, select4};
use embassy_time::{Duration, Ticker, Timer};

use crate::keymap::DEFAULT_LAYERS;
use core::sync::atomic::Ordering;

use crate::shared::{self, CONFIG_REQ, CONFIG_RES, HID_REPORTS, LOCAL_KEYS, REMOTE_KEYS, Role, USB_CONFIGURED};
use crate::user_config;

#[embassy_executor::task]
pub async fn keyboard_task(mut engine: Engine) {
    // Só o central processa teclas.
    while shared::role() != Role::Central {
        Timer::after_millis(10).await;
    }
    LOCAL_KEYS.clear();

    let mut wpm = Wpm::new();
    let mut tick = Ticker::every(Duration::from_millis(1));
    let mut wpm_div = 0u8;

    loop {
        match select4(LOCAL_KEYS.receive(), REMOTE_KEYS.receive(), CONFIG_REQ.receive(), tick.next()).await {
            Either4::First(ev) | Either4::Second(ev) => {
                if ev.pressed {
                    wpm.key_pressed(ev.time);
                }
                engine.process(ev);
            }
            Either4::Third(req) => {
                let res = protocol::handle(&req, &mut Target { engine: &mut engine });
                let _ = CONFIG_RES.try_send(res);
            }
            Either4::Fourth(()) => {
                let now = shared::now_ms();
                engine.tick(now);
                wpm_div = wpm_div.wrapping_add(1);
                if wpm_div.is_multiple_of(100) {
                    let w = wpm.update(now);
                    shared::update_state(|s| s.wpm = w);
                }
            }
        }

        while let Some(out) = engine.poll_output() {
            match out {
                Output::Report(r) => {
                    if USB_CONFIGURED.load(Ordering::Relaxed) {
                        // Espera a USB se a fila encher: nenhum toque é descartado.
                        HID_REPORTS.send(r).await;
                    } else if HID_REPORTS.try_send(r).is_err() {
                        // Sem USB: descarta os antigos, fica só com o último.
                        HID_REPORTS.clear();
                        let _ = HID_REPORTS.try_send(r);
                    }
                }
                Output::Command(c) => run_command(c),
            }
        }

        let layers = engine.layers();
        let mods = engine.mods().0;
        shared::update_state(|s| {
            s.layers = layers;
            s.mods = mods;
        });
    }
}

fn run_command(c: Command) {
    match c {
        #[cfg(feature = "rgb")]
        Command::Rgb(cmd) => shared::update_state(|s| corne_rgb::apply_cmd(&mut s.settings, cmd)),
        #[cfg(feature = "oled")]
        Command::Oled(cmd) => shared::update_state(|s| {
            let counts = (user_config::oled::LEFT_SCENES.len(), user_config::oled::RIGHT_SCENES.len());
            corne_oled::apply_cmd(&mut s.settings, cmd, counts)
        }),
        Command::Bootloader => crate::enter_bootloader(),
        Command::Reboot => cortex_m::peripheral::SCB::sys_reset(),
        Command::User(n, pressed) => user_config::on_user_action(n, pressed),
        #[allow(unreachable_patterns)]
        _ => {}
    }
}

/// Liga o protocolo de configuração ao motor.
struct Target<'a> {
    engine: &'a mut Engine,
}

impl ConfigTarget for Target<'_> {
    fn keymap(&mut self) -> &mut Keymap {
        &mut self.engine.keymap
    }

    fn reset_keymap(&mut self) {
        self.engine.keymap.load_encoded(&DEFAULT_LAYERS);
    }

    fn get_settings(&mut self) -> Settings {
        shared::state().settings
    }

    fn set_settings(&mut self, s: Settings) {
        shared::update_state(|st| st.settings = s);
    }

    fn save(&mut self) -> bool {
        // Grava as configurações de RGB/OLED agora (o keymap ainda não é salvo).
        crate::persist::SAVE_NOW.signal(());
        user_config::SAVE_SETTINGS
    }

    fn bootloader(&mut self) {
        crate::enter_bootloader();
    }

    fn side(&self) -> corne_core::layout::Side {
        user_config::SIDE
    }
}
