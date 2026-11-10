//! Link entre as metades pelo fio de dados do cabo TRS.
//!
//! **Camada física:** UART 8N1 half-duplex num único GPIO, feita com dois
//! state machines do PIO0 no mesmo pino: um só recebe (sempre ligado) e o outro
//! só transmite. Para transmitir, o pino vira saída; ao terminar volta a ser
//! entrada com pull-up. O receptor "escuta" o próprio eco enquanto transmitimos,
//! e é esse eco que diz exatamente quando o último bit saiu.
//!
//! **Protocolo:** veja `corne_core::split`. O central manda `Sync` (estado +
//! relógio) e o periférico responde com `Keys`. Só um fala por vez. Cada lote
//! de `Keys` é reenviado até o central confirmar (`ack`), então ruído no cabo
//! atrasa a tecla em ~1 ms mas não a perde.

use corne_core::engine::KeyEvent;
use corne_core::layout::{KEY_COUNT, KeyPos};
use corne_core::split::{FrameDecoder, MAX_FRAME, MAX_KEY_EVENTS, Message, NO_ACK, next_seq};
use embassy_futures::join::join;
use embassy_rp::Peri;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::gpio::{Level, Pull};
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{Common, Config, Direction, FifoJoin, Pin, PioPin, ShiftConfig, ShiftDirection, StateMachine};
use embassy_time::{Duration, Timer, with_timeout};
use fixed::traits::ToFixed;

use crate::shared::{self, LOCAL_KEYS, REMOTE_KEYS, Role, USB_CONFIGURED};

pub struct Link {
    // Mantido vivo: soltar o `Common` desliga o PIO.
    _common: Common<'static, PIO0>,
    rx: StateMachine<'static, PIO0, 0>,
    tx: StateMachine<'static, PIO0, 1>,
    pin: Pin<'static, PIO0>,
}

impl Link {
    pub fn new(
        mut common: Common<'static, PIO0>,
        mut rx: StateMachine<'static, PIO0, 0>,
        mut tx: StateMachine<'static, PIO0, 1>,
        pin: Peri<'static, impl PioPin>,
        baud: u32,
    ) -> Self {
        let mut pin = common.make_pio_pin(pin);
        pin.set_pull(Pull::Up);
        let divider = (clk_sys_freq() / (8 * baud)).to_fixed();

        // ---- Transmissor (8N1, 8 ciclos por bit) ----
        let tx_prg = pio::pio_asm!(
            ".side_set 1 opt",
            "    pull       side 1 [7]", // bit de parada / linha em repouso
            "    set x, 7   side 0 [7]", // bit de início
            "bitloop:",
            "    out pins, 1",
            "    jmp x-- bitloop   [6]",
        );
        let tx_prg = common.load_program(&tx_prg.program);
        let mut cfg = Config::default();
        cfg.set_out_pins(&[&pin]);
        cfg.use_program(&tx_prg, &[&pin]);
        cfg.shift_out = ShiftConfig { auto_fill: false, threshold: 32, direction: ShiftDirection::Right };
        cfg.fifo_join = FifoJoin::TxOnly;
        cfg.clock_divider = divider;
        tx.set_config(&cfg);
        tx.set_pins(Level::High, &[&pin]);
        tx.set_pin_dirs(Direction::In, &[&pin]); // começa soltando a linha
        tx.set_enable(true);

        // ---- Receptor (o mesmo do embassy-rp, sem mexer no nível do pino) ----
        let rx_prg = pio::pio_asm!(
            "start:",
            "    wait 0 pin 0",
            "    set x, 7    [10]",
            "rx_bitloop:",
            "    in pins, 1",
            "    jmp x-- rx_bitloop [6]",
            "    jmp pin good_rx_stop",
            "    irq 4 rel",
            "    wait 1 pin 0",
            "    jmp start",
            "good_rx_stop:",
            "    in null 24",
            "    push",
        );
        let rx_prg = common.load_program(&rx_prg.program);
        let mut cfg = Config::default();
        cfg.use_program(&rx_prg, &[]);
        cfg.set_in_pins(&[&pin]);
        cfg.set_jmp_pin(&pin);
        cfg.shift_in = ShiftConfig { auto_fill: false, threshold: 32, direction: ShiftDirection::Right };
        cfg.fifo_join = FifoJoin::RxOnly;
        cfg.clock_divider = divider;
        rx.set_config(&cfg);
        rx.set_pin_dirs(Direction::In, &[&pin]);
        rx.set_enable(true);

        Link { _common: common, rx, tx, pin }
    }

    fn drain_rx(&mut self) {
        while self.rx.rx().try_pull().is_some() {}
    }

    /// Transmite um quadro e devolve a linha (pino como entrada).
    async fn send(&mut self, data: &[u8]) {
        self.drain_rx();
        self.tx.set_pin_dirs(Direction::Out, &[&self.pin]);
        let Link { rx, tx, .. } = self;
        let push = async {
            for &b in data {
                tx.tx().wait_push(b as u32).await;
            }
        };
        // Espera o eco de cada byte: quando o último volta, a linha está livre.
        let echo = async {
            let _ = with_timeout(Duration::from_millis(3), async {
                for _ in data {
                    rx.rx().wait_pull().await;
                }
            })
            .await;
        };
        join(push, echo).await;
        self.tx.set_pin_dirs(Direction::In, &[&self.pin]);
    }

    async fn recv_byte(&mut self) -> u8 {
        self.rx.rx().wait_pull().await as u8
    }
}

#[embassy_executor::task]
pub async fn split_task(mut link: Link) {
    let mut dec = FrameDecoder::new();

    // Descobre o papel: recebeu Sync => periférico; USB configurada e ninguém
    // falando no cabo => central. Escutar primeiro evita que as duas metades
    // transmitam ao mesmo tempo se ambas estiverem ligadas na USB.
    let mut quiet_since = shared::local_ms();
    loop {
        match with_timeout(Duration::from_millis(10), link.recv_byte()).await {
            Ok(b) => {
                quiet_since = shared::local_ms();
                if let Some(Message::Sync { time_ms, state, .. }) = dec.push(b) {
                    shared::set_role(Role::Peripheral);
                    on_sync(time_ms, state);
                    peripheral(link, dec).await;
                }
            }
            Err(_) => {
                let quiet = shared::local_ms().wrapping_sub(quiet_since) >= 50;
                if quiet && USB_CONFIGURED.load(core::sync::atomic::Ordering::Relaxed) {
                    shared::set_role(Role::Central);
                    central(link).await;
                }
            }
        }
    }
}

async fn central(mut link: Link) -> ! {
    let mut dec = FrameDecoder::new();
    let mut buf = [0u8; MAX_FRAME];
    let mut misses = 0u32;
    let mut connected = false;
    // Teclas remotas seguradas, para soltar tudo se o cabo desconectar.
    let mut remote_held = 0u64;
    // Último lote aceito (None = ainda não conectado: aceita qualquer um).
    let mut last_seq: Option<u8> = None;
    let mut errors = 0u16;

    loop {
        let state = shared::state();
        let ack = last_seq.unwrap_or(NO_ACK);
        let n = Message::Sync { time_ms: shared::now_ms(), state, ack }.encode(&mut buf);
        link.send(&buf[..n]).await;

        dec.reset();
        let reply = with_timeout(Duration::from_millis(3), async {
            loop {
                let b = link.recv_byte().await;
                if let Some(m) = dec.push(b) {
                    return m;
                }
            }
        })
        .await;

        match reply {
            Ok(Message::Keys { seq, count, events }) => {
                misses = 0;
                connected = true;
                // O mesmo seq de novo é reenvio de um lote já aceito (o ack se perdeu).
                let fresh = last_seq != Some(seq);
                last_seq = Some(seq);
                let now = shared::now_ms();
                for &(pos, pressed) in events.iter().take(if fresh { count as usize } else { 0 }) {
                    if !pos.exists() {
                        continue;
                    }
                    let bit = 1u64 << pos.index();
                    if pressed {
                        remote_held |= bit
                    } else {
                        remote_held &= !bit
                    }
                    // Espera se a fila encher: o lote já foi confirmado, não pode ser descartado.
                    REMOTE_KEYS.send(KeyEvent { pos, pressed, time: now }).await;
                }
            }
            _ => {
                misses += 1;
                if connected {
                    errors = errors.saturating_add(1);
                }
                if connected && misses > 100 {
                    connected = false;
                    last_seq = None;
                    let now = shared::now_ms();
                    for i in 0..KEY_COUNT {
                        if remote_held & (1 << i) != 0 {
                            let pos = KeyPos::from_index(i);
                            REMOTE_KEYS.send(KeyEvent { pos, pressed: false, time: now }).await;
                        }
                    }
                    remote_held = 0;
                }
            }
        }

        shared::update_state(|s| {
            s.split_connected = connected;
            s.link_errors = errors;
            s.usb_connected = USB_CONFIGURED.load(core::sync::atomic::Ordering::Relaxed);
        });
        // ~1 kHz de troca com a outra metade (latência de tecla remota ~1–2 ms).
        Timer::after_micros(400).await;
    }
}

fn on_sync(time_ms: u32, mut state: corne_core::state::KeyboardState) {
    shared::sync_clock(time_ms);
    state.split_connected = true;
    shared::update_state(|s| *s = state);
}

async fn peripheral(mut link: Link, mut dec: FrameDecoder) -> ! {
    let mut buf = [0u8; MAX_FRAME];
    // Lote enviado e ainda não confirmado pelo central.
    let mut seq = NO_ACK;
    let mut batch = [(KeyPos::default(), false); MAX_KEY_EVENTS];
    let mut count = 0usize;
    let mut confirmed = true;

    // Responde ao primeiro Sync que já recebemos (o central aceita o primeiro lote).
    let mut answer = true;
    loop {
        if answer {
            // Dá tempo do central soltar a linha.
            Timer::after_micros(40).await;
            if confirmed {
                // Lote novo com as próximas teclas (as que não couberem ficam no canal).
                seq = next_seq(seq);
                count = 0;
                while count < MAX_KEY_EVENTS {
                    match LOCAL_KEYS.try_receive() {
                        Ok(ev) => {
                            batch[count] = (ev.pos, ev.pressed);
                            count += 1;
                        }
                        Err(_) => break,
                    }
                }
                confirmed = false;
            }
            let n = Message::Keys { seq, count: count as u8, events: batch }.encode(&mut buf);
            link.send(&buf[..n]).await;
            answer = false;
        }

        // Sem notícias do central por um tempo: marca desconectado.
        match with_timeout(Duration::from_millis(200), link.recv_byte()).await {
            Ok(b) => {
                if let Some(Message::Sync { time_ms, state, ack }) = dec.push(b) {
                    on_sync(time_ms, state);
                    if ack == seq {
                        confirmed = true;
                    }
                    answer = true;
                }
            }
            Err(_) => {
                dec.reset();
                shared::update_state(|s| s.split_connected = false);
            }
        }
    }
}
