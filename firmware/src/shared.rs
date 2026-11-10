//! Estado e canais compartilhados entre as tarefas.

use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use corne_core::engine::KeyEvent;
use corne_core::hid::KeyboardReport;
use corne_core::layout::KeyPos;
use corne_core::state::KeyboardState;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex as Raw;
use embassy_sync::channel::Channel;
use embassy_sync::pubsub::PubSubChannel;
use embassy_time::Instant;

/// Papel desta metade, decidido em tempo de execução: quem está ligado ao PC
/// pela USB é o central; quem recebe `Sync` pelo cabo TRS é o periférico.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    Unknown = 0,
    Central = 1,
    Peripheral = 2,
}

static ROLE: AtomicU8 = AtomicU8::new(Role::Unknown as u8);
pub static USB_CONFIGURED: AtomicBool = AtomicBool::new(false);
/// Diferença entre o relógio do central e o local (ms).
static TIME_OFFSET: AtomicU32 = AtomicU32::new(0);

pub fn role() -> Role {
    match ROLE.load(Ordering::Relaxed) {
        1 => Role::Central,
        2 => Role::Peripheral,
        _ => Role::Unknown,
    }
}

pub fn set_role(r: Role) {
    ROLE.store(r as u8, Ordering::Relaxed);
}

/// Tempo local em ms.
pub fn local_ms() -> u32 {
    Instant::now().as_millis() as u32
}

/// Tempo em ms no relógio do central (igual nas duas metades).
pub fn now_ms() -> u32 {
    local_ms().wrapping_add(TIME_OFFSET.load(Ordering::Relaxed))
}

pub fn sync_clock(central_ms: u32) {
    TIME_OFFSET.store(central_ms.wrapping_sub(local_ms()), Ordering::Relaxed);
}

static STATE: Mutex<Raw, RefCell<KeyboardState>> = Mutex::new(RefCell::new(KeyboardState::new()));

/// Cópia do estado atual.
pub fn state() -> KeyboardState {
    STATE.lock(|s| *s.borrow())
}

pub fn update_state(f: impl FnOnce(&mut KeyboardState)) {
    STATE.lock(|s| f(&mut s.borrow_mut()));
}

/// Teclas desta metade (matriz → motor no central, ou → link no periférico).
pub static LOCAL_KEYS: Channel<Raw, KeyEvent, 32> = Channel::new();
/// Teclas da outra metade, recebidas pelo link (só no central).
pub static REMOTE_KEYS: Channel<Raw, KeyEvent, 32> = Channel::new();
/// Relatórios HID para a USB.
pub static HID_REPORTS: Channel<Raw, KeyboardReport, 32> = Channel::new();
/// Pacotes do protocolo de configuração (raw HID).
pub static CONFIG_REQ: Channel<Raw, [u8; 32], 2> = Channel::new();
pub static CONFIG_RES: Channel<Raw, [u8; 32], 2> = Channel::new();
/// Teclas desta metade para os complementos (RGB, OLED). Até 2 assinantes.
pub static ADDON_KEYS: PubSubChannel<Raw, (KeyPos, bool), 16, 2, 1> = PubSubChannel::new();
