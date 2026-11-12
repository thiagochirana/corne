//! Guarda as configurações de RGB e OLED (efeito, tema, brilho, cenas...) na
//! flash, para que voltem iguais depois de desligar o teclado.
//!
//! Usa o primeiro setor (4 KB) dos 64 KB reservados no fim da flash
//! (`memory.x`). Regravar o firmware (.uf2) não apaga esse setor. O formato
//! fica em `corne_core::persist`.
//!
//! Quando as configurações mudam, a gravação espera `user_config::SAVE_DELAY_MS`
//! sem novas mudanças (ex.: segurar RGB_VAI não grava a cada passo). As duas
//! metades gravam: o periférico recebe as configurações do central pelo cabo,
//! então qualquer lado que for ligado na USB depois começa com as mesmas.
//!
//! Durante a gravação o chip para por ~1 ms (ou ~45 ms quando o setor enche e
//! precisa ser apagado, a cada 128 gravações). Uma tecla apertada nesse
//! instante não se perde, só é lida logo depois.

use corne_core::persist::{self, RECORD_LEN, Scan};
use corne_core::state::Settings;
use embassy_rp::flash::{Blocking, ERASE_SIZE, Flash};
use embassy_rp::peripherals::FLASH;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};

use crate::shared;
use crate::user_config;

/// Tamanho da flash do Pro Micro RP2040 (igual ao `memory.x`).
const FLASH_SIZE: usize = 2 * 1024 * 1024;
/// Início da área reservada (últimos 64 KB). O primeiro setor é das configurações.
const SECTOR: u32 = (FLASH_SIZE - 64 * 1024) as u32;
const SLOTS: usize = ERASE_SIZE / RECORD_LEN;

pub type Storage = Flash<'static, FLASH, Blocking, FLASH_SIZE>;

/// Pedido de gravação imediata (protocolo de configuração pelo PC).
pub static SAVE_NOW: Signal<CriticalSectionRawMutex, ()> = Signal::new();

fn scan(flash: &mut Storage) -> Scan {
    persist::scan(SLOTS, &user_config::SETTINGS, |i| {
        let mut r = [0xFF; RECORD_LEN];
        // Se a leitura falhar, o registro parece apagado e a busca para ali.
        let _ = flash.blocking_read(SECTOR + (i * RECORD_LEN) as u32, &mut r);
        r
    })
}

/// Configurações para começar: as salvas, ou as de `user_config::SETTINGS`.
pub fn load(flash: &mut Storage) -> Settings {
    if !user_config::SAVE_SETTINGS {
        return user_config::SETTINGS;
    }
    scan(flash).saved.unwrap_or(user_config::SETTINGS)
}

fn save(flash: &mut Storage, s: &Settings) -> bool {
    let slot = match scan(flash).next_slot {
        Some(i) => i,
        None => {
            if flash.blocking_erase(SECTOR, SECTOR + ERASE_SIZE as u32).is_err() {
                return false;
            }
            0
        }
    };
    let record = persist::encode(s, &user_config::SETTINGS);
    flash.blocking_write(SECTOR + (slot * RECORD_LEN) as u32, &record).is_ok()
}

#[embassy_executor::task]
pub async fn persist_task(mut flash: Storage, mut saved: Settings) {
    if !user_config::SAVE_SETTINGS {
        return;
    }
    // Última mudança ainda não gravada: (configurações, quando mudou).
    let mut pending: Option<(Settings, Instant)> = None;
    loop {
        let now_requested = embassy_futures::select::select(Timer::after_millis(250), SAVE_NOW.wait()).await;
        let force = matches!(now_requested, embassy_futures::select::Either::Second(()));

        let current = shared::state().settings;
        if current == saved {
            pending = None;
            continue;
        }
        match pending {
            Some((s, _)) if s == current => {}
            _ => pending = Some((current, Instant::now())),
        }
        let since = pending.map(|(_, t)| t.elapsed()).unwrap_or_default();
        if force || since >= Duration::from_millis(user_config::SAVE_DELAY_MS) {
            if save(&mut flash, &current) {
                saved = current;
            }
            pending = None;
        }
    }
}
