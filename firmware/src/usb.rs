//! USB: teclado HID (boot protocol) + interface raw HID para configuração.

use core::sync::atomic::Ordering;

use embassy_executor::Spawner;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_usb::class::hid::{
    self, HidBootProtocol, HidReader, HidReaderWriter, HidSubclass, HidWriter, ReportId, RequestHandler, State,
};
use embassy_usb::control::OutResponse;
use embassy_usb::{Builder, Handler, UsbDevice};
use static_cell::StaticCell;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

use crate::shared::{self, CONFIG_REQ, CONFIG_RES, HID_REPORTS, USB_CONFIGURED};

type UsbDriver = Driver<'static, USB>;

/// pid.codes: VID de testes. Troque se for distribuir.
const VID: u16 = 0x1209;
const PID: u16 = 0x0001;

/// Descritor raw HID (usage page 0xFF60 / usage 0x61, como o VIA/QMK), 32 bytes.
const RAW_HID_DESCRIPTOR: &[u8] = &[
    0x06, 0x60, 0xFF, // Usage Page (Vendor 0xFF60)
    0x09, 0x61, // Usage (0x61)
    0xA1, 0x01, // Collection (Application)
    0x09, 0x62, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 32, 0x75, 0x08, 0x81, 0x02, // Input: 32 bytes
    0x09, 0x63, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x95, 32, 0x75, 0x08, 0x91, 0x02, // Output: 32 bytes
    0xC0, // End Collection
];

struct DeviceHandler;

impl Handler for DeviceHandler {
    fn configured(&mut self, configured: bool) {
        USB_CONFIGURED.store(configured, Ordering::Relaxed);
    }
    fn reset(&mut self) {
        USB_CONFIGURED.store(false, Ordering::Relaxed);
    }
}

/// Recebe os LEDs do host (Caps Lock, Num Lock).
struct LedHandler;

impl RequestHandler for LedHandler {
    fn set_report(&mut self, _id: ReportId, data: &[u8]) -> OutResponse {
        if let Some(&leds) = data.first() {
            shared::update_state(|s| {
                s.num_lock = leds & 0x01 != 0;
                s.caps_lock = leds & 0x02 != 0;
            });
        }
        OutResponse::Accepted
    }
}

pub fn init(spawner: Spawner, driver: UsbDriver) {
    let mut config = embassy_usb::Config::new(VID, PID);
    config.manufacturer = Some("corne-rs");
    config.product = Some("Corne V3 (Rust)");
    config.serial_number = Some(match crate::user_config::SIDE {
        corne_core::layout::Side::Left => "left",
        corne_core::layout::Side::Right => "right",
    });
    config.max_power = 500;
    config.max_packet_size_0 = 64;
    config.supports_remote_wakeup = true;

    static CONFIG_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESC: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS_DESC: StaticCell<[u8; 0]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static HANDLER: StaticCell<DeviceHandler> = StaticCell::new();
    // Dois handlers: um para SET_REPORT pelo controle (Windows), outro para o endpoint OUT.
    static LED_HANDLER: StaticCell<LedHandler> = StaticCell::new();
    static LED_CTRL_HANDLER: StaticCell<LedHandler> = StaticCell::new();
    static KB_STATE: StaticCell<State> = StaticCell::new();
    static RAW_STATE: StaticCell<State> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESC.init([0; 256]),
        BOS_DESC.init([0; 256]),
        MSOS_DESC.init([0; 0]),
        CONTROL_BUF.init([0; 64]),
    );
    builder.handler(HANDLER.init(DeviceHandler));

    let kb_config = hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: Some(LED_CTRL_HANDLER.init(LedHandler)),
        poll_ms: 1,
        max_packet_size: 8,
        hid_subclass: HidSubclass::Boot,
        hid_boot_protocol: HidBootProtocol::Keyboard,
    };
    let keyboard = HidReaderWriter::<_, 1, 8>::new(&mut builder, KB_STATE.init(State::new()), kb_config);

    let raw_config = hid::Config {
        report_descriptor: RAW_HID_DESCRIPTOR,
        request_handler: None,
        poll_ms: 10,
        max_packet_size: 32,
        hid_subclass: HidSubclass::No,
        hid_boot_protocol: HidBootProtocol::None,
    };
    let raw = HidReaderWriter::<_, 32, 32>::new(&mut builder, RAW_STATE.init(State::new()), raw_config);

    let usb = builder.build();
    let (kb_reader, kb_writer) = keyboard.split();

    spawner.spawn(usb_task(usb).unwrap());
    spawner.spawn(hid_writer_task(kb_writer).unwrap());
    spawner.spawn(hid_reader_task(kb_reader, LED_HANDLER.init(LedHandler)).unwrap());
    spawner.spawn(raw_hid_task(raw).unwrap());
}

#[embassy_executor::task]
async fn usb_task(mut usb: UsbDevice<'static, UsbDriver>) -> ! {
    usb.run().await
}

#[embassy_executor::task]
async fn hid_writer_task(mut writer: HidWriter<'static, UsbDriver, 8>) -> ! {
    loop {
        let report = HID_REPORTS.receive().await;
        let _ = writer.write(&report.to_bytes()).await;
    }
}

#[embassy_executor::task]
async fn hid_reader_task(reader: HidReader<'static, UsbDriver, 1>, handler: &'static mut LedHandler) -> ! {
    reader.run(false, handler).await
}

#[embassy_executor::task]
async fn raw_hid_task(raw: HidReaderWriter<'static, UsbDriver, 32, 32>) -> ! {
    let (mut reader, mut writer) = raw.split();
    loop {
        let mut req = [0u8; 32];
        if let Ok(n) = reader.read(&mut req).await {
            if n == 0 {
                continue;
            }
            CONFIG_REQ.send(req).await;
            let res = CONFIG_RES.receive().await;
            let _ = writer.write(&res).await;
        }
    }
}
