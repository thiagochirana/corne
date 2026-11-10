//! Firmware do Corne V3 (3x6+3) para Pro Micro RP2040, em Rust com Embassy.
//!
//! Organização:
//! - `corne-core` (crate): camadas, keymap, tap-hold, protocolos. Sem hardware.
//! - `corne-rgb`, `corne-oled` (crates): complementos, também sem hardware.
//! - aqui: tarefas que ligam tudo ao RP2040 (matriz, USB, link TRS, LEDs, OLED).
//!
//! As duas metades rodam este mesmo código, compilado com `--features left` ou
//! `--features right`. Quem estiver na USB vira o lado central.
//!
//! Os dois núcleos do RP2040 dividem o trabalho:
//! - core 0: matriz, link TRS, motor de teclas e USB (o caminho da tecla).
//! - core 1: RGB e OLED. Renderizar efeitos pesados nunca atrasa uma tecla.
#![no_std]
#![no_main]

mod addons;
mod board;
mod keyboard;
mod keymap;
mod matrix;
mod shared;
mod split;
mod usb;
mod user_config;

use core::mem::MaybeUninit;

use corne_core::engine::Engine;
use corne_core::keymap::Keymap;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, I2C1, PIO0, PIO1, USB};
use embassy_rp::pio::Pio;
use embassy_time::Timer;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => embassy_rp::usb::InterruptHandler<USB>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
    PIO1_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO1>;
    I2C1_IRQ => embassy_rp::i2c::InterruptHandler<I2C1>;
    DMA_IRQ_0 => embassy_rp::dma::InterruptHandler<DMA_CH0>;
});

/// Reinicia no bootloader UF2 (aparece a unidade RPI-RP2).
pub fn enter_bootloader() -> ! {
    embassy_rp::rom_data::reset_to_usb_boot(0, 0);
    #[allow(clippy::empty_loop)]
    loop {}
}

// ---------------------------------------------------------------------------
// Botão de reset da PCB (liga o RST do Pro Micro ao GND).
//
// No RP2040 esse botão só reinicia o chip; o modo de gravação normalmente
// exige segurar o BOOT do controlador. Aqui, dois toques rápidos no reset
// entram no modo de gravação, como no QMK:
//
// Uma palavra na RAM que não é zerada no boot (seção `.uninit`) guarda um
// "carimbo" por `DOUBLE_TAP_MS`. O reset pelo pino RST mantém a RAM, então se o
// segundo toque vier nesse intervalo o carimbo ainda está lá e chamamos o
// bootloader da ROM. Ao ligar o cabo, a RAM começa com lixo e o carimbo não bate.
// ---------------------------------------------------------------------------
const DOUBLE_TAP_MAGIC: u32 = 0xF01D_AB1E;

#[unsafe(link_section = ".uninit.DOUBLE_TAP")]
static mut DOUBLE_TAP: MaybeUninit<u32> = MaybeUninit::uninit();

fn double_tap_check() {
    // SAFETY: acesso volátil a uma palavra própria, antes de qualquer tarefa.
    unsafe {
        let p = core::ptr::addr_of_mut!(DOUBLE_TAP) as *mut u32;
        if p.read_volatile() == DOUBLE_TAP_MAGIC {
            p.write_volatile(0);
            enter_bootloader();
        }
        p.write_volatile(DOUBLE_TAP_MAGIC);
    }
}

#[embassy_executor::task]
async fn double_tap_clear() {
    Timer::after_millis(user_config::DOUBLE_TAP_MS).await;
    // SAFETY: idem acima.
    unsafe { (core::ptr::addr_of_mut!(DOUBLE_TAP) as *mut u32).write_volatile(0) };
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // Em caso de erro grave, cai no bootloader para facilitar regravar.
    enter_bootloader()
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    double_tap_check();
    let p = embassy_rp::init(Default::default());
    spawner.spawn(double_tap_clear().unwrap());

    let side = user_config::SIDE;
    shared::update_state(|s| s.settings = user_config::SETTINGS);

    // --- Matriz ---
    let pins = board::matrix_pins!(p);
    let rows = pins.rows.map(|pin| Output::new(pin, Level::High));
    let cols = pins.cols.map(|pin| Input::new(pin, Pull::Up));
    spawner.spawn(matrix::matrix_task(rows, cols, side).unwrap());

    // --- USB ---
    usb::init(spawner, embassy_rp::usb::Driver::new(p.USB, Irqs));

    // --- Link entre as metades (PIO0) ---
    let Pio { common, sm0, sm1, .. } = Pio::new(p.PIO0, Irqs);
    let link = split::Link::new(common, sm0, sm1, board::split_pin!(p), user_config::SPLIT_BAUD);
    spawner.spawn(split::split_task(link).unwrap());

    // --- Motor de teclas (só age se esta metade for a central) ---
    let engine = Engine::new(Keymap::from_encoded(&keymap::DEFAULT_LAYERS), user_config::ENGINE)
        .with_combos(&keymap::COMBOS, keymap::COMBO_TERM_MS, keymap::COMBO_IDLE_MS)
        .with_macros(keymap::MACROS);
    spawner.spawn(keyboard::keyboard_task(engine).unwrap());

    // --- Complementos no core 1 (RGB: PIO1 + DMA, OLED: I2C1) ---
    // Os drivers são criados dentro do core 1 para que as interrupções deles
    // sejam atendidas lá.
    #[cfg(any(feature = "rgb", feature = "oled"))]
    {
        use core::ptr::addr_of_mut;
        use embassy_executor::Executor;
        use embassy_rp::multicore::{Stack, spawn_core1};
        use static_cell::StaticCell;

        // 16 KB: as cenas do OLED guardam o estado das animações (fogo, fogos...).
        static mut CORE1_STACK: Stack<16384> = Stack::new();
        static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

        #[cfg(feature = "rgb")]
        let (pio1, dma, rgb_pin) = (p.PIO1, p.DMA_CH0, board::rgb_pin!(p));
        #[cfg(feature = "oled")]
        let (i2c, scl, sda) = board::oled_i2c!(p);

        // SAFETY: a pilha do core 1 só é usada aqui, uma vez.
        let stack = unsafe { &mut *addr_of_mut!(CORE1_STACK) };
        spawn_core1(p.CORE1, stack, move || {
            EXECUTOR1.init(Executor::new()).run(|spawner| {
                #[cfg(feature = "rgb")]
                {
                    use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
                    let Pio { mut common, sm0, .. } = Pio::new(pio1, Irqs);
                    let program = PioWs2812Program::new(&mut common);
                    let leds = PioWs2812::new(&mut common, sm0, dma, Irqs, rgb_pin, &program);
                    spawner.spawn(addons::rgb::rgb_task(common, leds, side).unwrap());
                }

                #[cfg(feature = "oled")]
                {
                    let mut config = embassy_rp::i2c::Config::default();
                    config.frequency = 400_000;
                    let bus = embassy_rp::i2c::I2c::new_async(i2c, scl, sda, Irqs, config);
                    spawner.spawn(addons::oled::oled_task(bus, side).unwrap());
                }
            })
        });
    }
}
