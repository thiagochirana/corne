//! Motor de teclas: transforma eventos (tecla X apertada/solta) em relatórios
//! HID e comandos, tratando combos, camadas e tap-hold.
//!
//! Etapas de um evento: combos ([`crate::combo`]) → tap-hold → ação da tecla.
//! Macros ([`crate::macros`]) são tocadas aos poucos em [`Engine::tick`]; teclas
//! apertadas enquanto uma macro toca esperam numa fila e saem depois, na ordem.
//!
//! É puro (sem hardware nem relógio próprio): o firmware entrega eventos com
//! carimbo de tempo em ms e chama [`Engine::tick`] periodicamente. Isso permite
//! testar tudo no PC com `cargo test`.

use heapless::Deque;

use crate::action::{Action, OledCmd, RgbCmd};
use crate::combo::{Combo, DEFAULT_TERM_MS, MAX_COMBO_KEYS};
use crate::hid::KeyboardReport;
use crate::keycode::{Keycode, Mods};
use crate::keymap::Keymap;
use crate::layer::LayerState;
use crate::layout::{COLS, KeyPos, ROWS};
use crate::macros::{MacroTable, Step};

/// Evento de tecla vindo da matriz (local ou da outra metade).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyEvent {
    pub pos: KeyPos,
    pub pressed: bool,
    /// Tempo em ms (relógio do lado central).
    pub time: u32,
}

/// Como decidir entre "toque" e "segurar" numa tecla tap-hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TapHoldMode {
    /// Vira "segurar" só pelo tempo (`tapping_term_ms`).
    TimeoutOnly,
    /// Vira "segurar" se outra tecla for apertada E solta enquanto esta está
    /// pressionada. Ao digitar rápido as teclas se sobrepõem (`SPC↓ E↓ SPC↑
    /// E↑`), e isso continua sendo toque. Recomendado para `LT` e home row mods.
    PermissiveHold,
    /// Vira "segurar" assim que outra tecla for apertada. Mais rápido para usar
    /// a camada, mas engasga ao digitar rápido (a próxima letra desce antes de
    /// o espaço subir).
    HoldOnOtherKeyPress,
}

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    pub tapping_term_ms: u16,
    pub mod_tap_mode: TapHoldMode,
    pub layer_tap_mode: TapHoldMode,
    /// "Flow tap": uma tecla tap-hold apertada até `flow_tap_ms` depois de uma
    /// letra/número/pontuação vale como **toque na hora**, sem esperar decisão.
    /// No meio de uma palavra ninguém quer a camada, e assim o espaço depois de
    /// uma palavra nunca vira camada. 0 desliga.
    pub flow_tap_ms: u16,
    /// Um `LT` segurado além do `tapping_term_ms` sem apertar nenhuma outra
    /// tecla envia o toque ao soltar (o espaço segurado "pensando" não some).
    pub retro_tap: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            tapping_term_ms: 200,
            mod_tap_mode: TapHoldMode::PermissiveHold,
            layer_tap_mode: TapHoldMode::PermissiveHold,
            flow_tap_ms: 150,
            retro_tap: true,
        }
    }
}

/// Teclas com nada antes delas: nenhum "flow tap" nem bloqueio de combo.
fn recent(last: Option<u32>, now: u32, window: u16) -> bool {
    window > 0 && last.is_some_and(|t| now.wrapping_sub(t) < window as u32)
}

/// O que o toque de um LT envia: a tecla, com Shift se for o caso (ex.: `?`).
fn tap_active(m: Mods, k: Keycode) -> Active {
    if m.is_empty() { Active::Key(k) } else { Active::ModKey(m, k) }
}

/// O toque de uma tecla tap-hold (`MT`/`LT`).
fn tap_of(action: Action) -> Active {
    match action {
        Action::ModTap(_, k) => Active::Key(k),
        Action::LayerTap(_, m, k) => tap_active(m, k),
        _ => Active::Consumed,
    }
}

/// Comandos que o motor emite para o resto do sistema (addons, firmware).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Rgb(RgbCmd),
    Oled(OledCmd),
    Bootloader,
    Reboot,
    /// Ação do usuário: (número, apertada?).
    User(u8, bool),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Output {
    Report(KeyboardReport),
    Command(Command),
}

/// O que uma tecla pressionada está fazendo agora (lembrado até soltar, para
/// que trocar de camada no meio não deixe teclas "presas").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Active {
    Key(Keycode),
    ModKey(Mods, Keycode),
    HoldMods(Mods),
    Layer(u8),
    User(u8),
    Consumed,
}

#[derive(Clone, Copy, Debug)]
struct Pending {
    pos: KeyPos,
    action: Action,
    time: u32,
}

const BUFFER: usize = 16;
const OUTPUTS: usize = 64;
/// Combos disparados ao mesmo tempo.
const HELD_COMBOS: usize = 8;
/// Teclas guardadas enquanto uma macro toca.
const MACRO_QUEUE: usize = 64;
/// Folga mínima na fila de saídas para a macro continuar no mesmo `tick`.
const MACRO_ROOM: usize = 4;

/// Macro tocando: posição atual nos passos.
#[derive(Clone, Copy, Debug)]
struct Player {
    steps: &'static [u16],
    pos: usize,
    /// Esperando até este instante (WAIT ou `delay` entre teclas).
    wait_until: Option<u32>,
}

/// Combo disparado cujas teclas ainda estão seguradas.
#[derive(Clone, Copy, Debug)]
struct HeldCombo {
    /// Teclas do combo ainda apertadas (as soltas depois são ignoradas).
    remaining: u64,
    /// Ação em andamento; `None` depois que a primeira tecla foi solta.
    active: Option<Active>,
}

pub struct Engine {
    pub keymap: Keymap,
    pub config: EngineConfig,
    layers: LayerState,
    active: [[Option<Active>; COLS]; ROWS],
    pending: Option<Pending>,
    buffer: Deque<KeyEvent, BUFFER>,
    last_report: KeyboardReport,
    outputs: Deque<Output, OUTPUTS>,
    // --- combos ---
    combos: &'static [Combo],
    combo_term: u16,
    /// Teclas apertadas esperando para ver se formam um combo.
    combo_buf: Deque<KeyEvent, MAX_COMBO_KEYS>,
    combo_mask: u64,
    combo_start: u32,
    combo_layer: u8,
    /// Combos só começam se a última tecla de digitação foi há pelo menos isso (ms).
    combo_idle: u16,
    held_combos: heapless::Vec<HeldCombo, HELD_COMBOS>,
    // --- digitação ---
    /// Instante da última letra/número/pontuação enviada (flow tap, combos).
    last_typed: Option<u32>,
    /// `LT` que virou "segurar" só pelo tempo: envia o toque ao soltar, se
    /// nenhuma outra tecla for apertada antes.
    retro: Option<(KeyPos, Active)>,
    // --- macros ---
    macros: MacroTable,
    player: Option<Player>,
    /// Próximas macros a tocar (disparadas enquanto outra tocava).
    macro_next: Deque<u8, 4>,
    /// Teclas apertadas enquanto a macro toca.
    macro_queue: Deque<KeyEvent, MACRO_QUEUE>,
    /// Estado de teclas da macro (DOWN/UP e o toque atual).
    macro_mods: u8,
    macro_keys: heapless::Vec<u8, 6>,
    macro_tap: Option<(u8, u8)>,
    /// Último instante conhecido (dos eventos e do `tick`).
    now: u32,
}

impl Engine {
    pub fn new(keymap: Keymap, config: EngineConfig) -> Self {
        Engine {
            keymap,
            config,
            layers: LayerState::default(),
            active: [[None; COLS]; ROWS],
            pending: None,
            buffer: Deque::new(),
            last_report: KeyboardReport::default(),
            outputs: Deque::new(),
            combos: &[],
            combo_term: DEFAULT_TERM_MS,
            combo_buf: Deque::new(),
            combo_mask: 0,
            combo_start: 0,
            combo_layer: 0,
            combo_idle: 0,
            held_combos: heapless::Vec::new(),
            last_typed: None,
            retro: None,
            macros: MacroTable::EMPTY,
            player: None,
            macro_next: Deque::new(),
            macro_queue: Deque::new(),
            macro_mods: 0,
            macro_keys: heapless::Vec::new(),
            macro_tap: None,
            now: 0,
        }
    }

    /// Liga as macros (ver [`crate::macros`]).
    pub fn with_macros(mut self, macros: MacroTable) -> Self {
        self.macros = macros;
        self
    }

    /// Uma macro está tocando?
    pub fn macro_playing(&self) -> bool {
        self.player.is_some()
    }

    /// Liga os combos (ver [`crate::combo`]). `term_ms`: janela para apertar
    /// todas as teclas. `idle_ms`: um combo só começa se a última letra foi há
    /// pelo menos isso (no meio de uma palavra, as teclas saem normais). 0 desliga.
    pub fn with_combos(mut self, combos: &'static [Combo], term_ms: u16, idle_ms: u16) -> Self {
        self.combos = combos;
        self.combo_term = term_ms;
        self.combo_idle = idle_ms;
        self
    }

    pub fn layers(&self) -> LayerState {
        self.layers
    }

    /// Modificadores que estão sendo enviados agora.
    pub fn mods(&self) -> Mods {
        Mods(self.last_report.mods)
    }

    /// Retira a próxima saída (relatório HID ou comando).
    pub fn poll_output(&mut self) -> Option<Output> {
        self.outputs.pop_front()
    }

    /// Entrega um evento de tecla.
    pub fn process(&mut self, ev: KeyEvent) {
        if !ev.pos.exists() {
            return;
        }
        self.now = ev.time;
        // Durante uma macro, as teclas esperam a vez (nenhuma se perde).
        if self.player.is_some() && self.macro_queue.push_back(ev).is_ok() {
            return;
        }
        if self.combos.is_empty() {
            self.process_key(ev);
        } else {
            self.combo_event(ev);
        }
    }

    // ------------------------------------------------------------------ combos

    fn combo_event(&mut self, ev: KeyEvent) {
        let bit = 1u64 << ev.pos.index();
        if !ev.pressed {
            // Tecla de um combo já disparado: a primeira a soltar solta a ação.
            if let Some(i) = self.held_combos.iter().position(|h| h.remaining & bit != 0) {
                self.held_combos[i].remaining &= !bit;
                if let Some(a) = self.held_combos[i].active.take() {
                    self.release_active(a);
                    self.emit_report();
                }
                if self.held_combos[i].remaining == 0 {
                    self.held_combos.swap_remove(i);
                }
                return;
            }
            // Soltou uma tecla que esperava o combo: decide agora.
            if self.combo_mask & bit != 0 {
                self.combo_resolve();
                return self.combo_event(ev);
            }
            return self.process_key(ev);
        }

        // Passou da janela: decide o que estava esperando.
        if self.combo_mask != 0 && ev.time.wrapping_sub(self.combo_start) >= self.combo_term as u32 {
            self.combo_resolve();
        }
        let layer = if self.combo_mask == 0 { self.layers.highest() } else { self.combo_layer };
        let want = self.combo_mask | bit;
        // No meio de uma palavra, um combo novo não começa (ex.: "po" de "tempo").
        let typing = self.combo_mask == 0 && recent(self.last_typed, ev.time, self.combo_idle);
        if !typing && self.combo_mask & bit == 0 && self.combo_candidate(want, layer, false) {
            if self.combo_mask == 0 {
                self.combo_start = ev.time;
                self.combo_layer = layer;
            }
            self.combo_mask = want;
            let _ = self.combo_buf.push_back(ev);
            // Completo e sem combo maior possível: dispara sem esperar.
            if self.combo_exact(want, layer).is_some() && !self.combo_candidate(want, layer, true) {
                self.combo_resolve();
            }
            return;
        }
        if self.combo_mask != 0 {
            // Tecla que não forma combo com as que esperavam.
            self.combo_resolve();
            return self.combo_event(ev);
        }
        self.process_key(ev);
    }

    /// Algum combo contém todas as teclas de `mask` (`bigger`: e tem mais teclas)?
    fn combo_candidate(&self, mask: u64, layer: u8, bigger: bool) -> bool {
        self.combos.iter().any(|c| c.applies(layer) && c.keys & mask == mask && (!bigger || c.keys != mask))
    }

    fn combo_exact(&self, mask: u64, layer: u8) -> Option<Combo> {
        self.combos.iter().find(|c| c.applies(layer) && c.keys == mask).copied()
    }

    /// Fecha a espera: dispara o combo se as teclas formam um, senão entrega as
    /// teclas normalmente, na ordem.
    fn combo_resolve(&mut self) {
        let mask = core::mem::replace(&mut self.combo_mask, 0);
        let buf = core::mem::take(&mut self.combo_buf);
        if let Some(c) = self.combo_exact(mask, self.combo_layer)
            && !self.held_combos.is_full()
        {
            let active = self.press_combo_action(Action::decode(c.action));
            let _ = self.held_combos.push(HeldCombo { remaining: mask, active: Some(active) });
            self.emit_report();
            return;
        }
        for ev in buf {
            self.process_key(ev);
        }
    }

    // ---------------------------------------------------------------- tap-hold

    fn process_key(&mut self, ev: KeyEvent) {
        // Um evento que chega depois do tapping term resolve o pendente como "segurar".
        if let Some(p) = self.pending
            && ev.time.wrapping_sub(p.time) >= self.config.tapping_term_ms as u32
        {
            self.resolve_hold(true);
        }
        // Outra tecla apertada: o LT segurado não envia mais o toque ao soltar.
        if ev.pressed && self.retro.is_some_and(|(pos, _)| pos != ev.pos) {
            self.retro = None;
        }

        let Some(p) = self.pending else {
            self.handle(ev);
            return;
        };

        if ev.pos == p.pos && !ev.pressed {
            self.resolve_tap(ev.time);
            return;
        }

        // Soltou uma tecla apertada ANTES do pendente: não muda a decisão, sai já.
        if !ev.pressed && !self.buffer.iter().any(|b| b.pos == ev.pos) {
            self.handle(ev);
            return;
        }

        if self.buffer.push_back(ev).is_err() {
            self.resolve_hold(false);
            self.handle(ev);
            return;
        }

        let mode = match p.action {
            Action::ModTap(..) => self.config.mod_tap_mode,
            _ => self.config.layer_tap_mode,
        };
        let decide_hold = match mode {
            TapHoldMode::TimeoutOnly => false,
            TapHoldMode::HoldOnOtherKeyPress => ev.pressed,
            // Soltou uma tecla que foi apertada depois do pendente.
            TapHoldMode::PermissiveHold => !ev.pressed && self.buffer.iter().any(|b| b.pressed && b.pos == ev.pos),
        };
        if decide_hold {
            self.resolve_hold(false);
        }
    }

    /// Avança o tempo (chamar a cada ~1–5 ms). Resolve combos e tap-hold por tempo.
    pub fn tick(&mut self, now: u32) {
        self.now = now;
        self.run_macro();
        if self.combo_mask != 0 && now.wrapping_sub(self.combo_start) >= self.combo_term as u32 {
            self.combo_resolve();
        }
        if let Some(p) = self.pending
            && now.wrapping_sub(p.time) >= self.config.tapping_term_ms as u32
        {
            self.resolve_hold(true);
        }
    }

    /// Solta tudo (ex.: ao perder a conexão com a outra metade).
    pub fn release_all(&mut self) {
        self.player = None;
        self.macro_next.clear();
        self.macro_queue.clear();
        self.macro_mods = 0;
        self.macro_keys.clear();
        self.macro_tap = None;
        self.pending = None;
        self.retro = None;
        self.buffer.clear();
        self.combo_mask = 0;
        self.combo_buf.clear();
        for h in core::mem::take(&mut self.held_combos) {
            if let Some(Active::Layer(l)) = h.active {
                self.layers.off(l);
            }
        }
        for r in 0..ROWS {
            for c in 0..COLS {
                if let Some(Active::Layer(l)) = self.active[r][c] {
                    self.layers.off(l);
                }
                self.active[r][c] = None;
            }
        }
        self.emit_report();
    }

    fn set_active(&mut self, pos: KeyPos, a: Option<Active>) {
        self.active[pos.row as usize][pos.col as usize] = a;
    }

    fn take_active(&mut self, pos: KeyPos) -> Option<Active> {
        self.active[pos.row as usize][pos.col as usize].take()
    }

    fn command(&mut self, c: Command) {
        let _ = self.outputs.push_back(Output::Command(c));
    }

    fn handle(&mut self, ev: KeyEvent) {
        if ev.pressed {
            let action = self.keymap.resolve(&self.layers, ev.pos);
            let active = match action {
                // Logo depois de uma letra: é toque, sem esperar (flow tap).
                Action::ModTap(..) | Action::LayerTap(..)
                    if recent(self.last_typed, ev.time, self.config.flow_tap_ms) =>
                {
                    tap_of(action)
                }
                Action::ModTap(..) | Action::LayerTap(..) => {
                    self.pending = Some(Pending { pos: ev.pos, action, time: ev.time });
                    return;
                }
                _ => self.press_action(action),
            };
            self.note_typed(active, ev.time);
            self.set_active(ev.pos, Some(active));
        } else if let Some(a) = self.take_active(ev.pos) {
            self.release_active(a);
            // LT segurado sem usar a camada: envia o toque agora.
            if let Some((pos, tap)) = self.retro
                && pos == ev.pos
            {
                self.retro = None;
                self.set_active(pos, Some(tap));
                self.emit_report();
                self.set_active(pos, None);
            }
        }
        self.emit_report();
    }

    /// Lembra o instante de uma letra/número/pontuação enviada.
    fn note_typed(&mut self, a: Active, time: u32) {
        if let Active::Key(k) | Active::ModKey(_, k) = a
            && k.is_typing()
        {
            self.last_typed = Some(time);
        }
    }

    /// Ação de um combo. Tap-hold num combo vale como toque (a tecla).
    fn press_combo_action(&mut self, action: Action) -> Active {
        match action {
            Action::ModTap(..) | Action::LayerTap(..) => tap_of(action),
            a => self.press_action(a),
        }
    }

    /// Começa uma ação (exceto tap-hold) e devolve o que fica ativo até soltar.
    fn press_action(&mut self, action: Action) -> Active {
        match action {
            Action::No | Action::Trans | Action::ModTap(..) | Action::LayerTap(..) => Active::Consumed,
            Action::Key(k) => Active::Key(k),
            Action::ModKey(m, k) => Active::ModKey(m, k),
            Action::Momentary(l) => {
                self.layers.on(l);
                Active::Layer(l)
            }
            Action::Toggle(l) => {
                self.layers.toggle(l);
                Active::Consumed
            }
            Action::To(l) => {
                self.layers.to(l);
                Active::Consumed
            }
            Action::DefaultLayer(l) => {
                self.layers.set_default(l);
                Active::Consumed
            }
            Action::Bootloader => {
                self.command(Command::Bootloader);
                Active::Consumed
            }
            Action::Reboot => {
                self.command(Command::Reboot);
                Active::Consumed
            }
            Action::Rgb(c) => {
                self.command(Command::Rgb(c));
                Active::Consumed
            }
            Action::Oled(c) => {
                self.command(Command::Oled(c));
                Active::Consumed
            }
            Action::User(n) => {
                self.command(Command::User(n, true));
                Active::User(n)
            }
            Action::Macro(n) => {
                self.start_macro(n);
                Active::Consumed
            }
        }
    }

    // ------------------------------------------------------------------ macros

    fn start_macro(&mut self, n: u8) {
        if self.player.is_some() {
            let _ = self.macro_next.push_back(n);
            return;
        }
        let Some(steps) = self.macros.get(n) else { return };
        self.player = Some(Player { steps, pos: 0, wait_until: None });
        self.run_macro();
    }

    /// Avança a macro enquanto houver espaço na fila de saídas.
    fn run_macro(&mut self) {
        while let Some(mut p) = self.player {
            if let Some(t) = p.wait_until {
                if (self.now.wrapping_sub(t) as i32) < 0 {
                    return;
                }
                p.wait_until = None;
            }
            if p.pos >= p.steps.len() {
                self.finish_macro();
                continue;
            }
            if self.outputs.len() + MACRO_ROOM > OUTPUTS {
                self.player = Some(p);
                return;
            }
            let step = Step::decode(p.steps[p.pos]);
            p.pos += 1;
            match step {
                Some(Step::Tap(m, k)) => {
                    self.macro_tap = Some((m.0, k as u8));
                    self.emit_report();
                    self.macro_tap = None;
                    self.emit_report();
                    if self.macros.delay_ms > 0 {
                        p.wait_until = Some(self.now.wrapping_add(self.macros.delay_ms as u32));
                    }
                }
                Some(Step::Down(k)) => {
                    match k.as_modifier() {
                        Some(m) => self.macro_mods |= m.0,
                        None => {
                            if !self.macro_keys.contains(&(k as u8)) {
                                let _ = self.macro_keys.push(k as u8);
                            }
                        }
                    }
                    self.emit_report();
                }
                Some(Step::Up(k)) => {
                    match k.as_modifier() {
                        Some(m) => self.macro_mods &= !m.0,
                        None => self.macro_keys.retain(|x| *x != k as u8),
                    }
                    self.emit_report();
                }
                Some(Step::Wait(ms)) => p.wait_until = Some(self.now.wrapping_add(ms as u32)),
                None => {}
            }
            self.player = Some(p);
        }
    }

    /// Fim da macro: solta o que ela segurava, toca a próxima ou devolve as
    /// teclas guardadas, na ordem.
    fn finish_macro(&mut self) {
        self.player = None;
        self.macro_mods = 0;
        self.macro_keys.clear();
        if let Some(n) = self.macro_next.pop_front()
            && let Some(steps) = self.macros.get(n)
        {
            self.player = Some(Player { steps, pos: 0, wait_until: None });
            return;
        }
        self.emit_report();
        let queued = core::mem::take(&mut self.macro_queue);
        for ev in queued {
            self.process(ev);
        }
    }

    /// Termina uma ação que estava ativa.
    fn release_active(&mut self, a: Active) {
        match a {
            Active::Layer(l) => {
                // Só desliga se nenhuma outra tecla (ou combo) segura a mesma camada.
                let held = Some(Active::Layer(l));
                let still_held = self.active.iter().flatten().any(|a| *a == held)
                    || self.held_combos.iter().any(|h| h.active == held);
                if !still_held {
                    self.layers.off(l);
                }
            }
            Active::User(n) => self.command(Command::User(n, false)),
            _ => {}
        }
    }

    /// `by_timeout`: virou "segurar" só pelo tempo, sem outra tecla (retro tap).
    fn resolve_hold(&mut self, by_timeout: bool) {
        let Some(p) = self.pending.take() else { return };
        let a = match p.action {
            Action::ModTap(m, _) => Active::HoldMods(m),
            Action::LayerTap(l, ..) => {
                self.layers.on(l);
                if by_timeout && self.config.retro_tap {
                    self.retro = Some((p.pos, tap_of(p.action)));
                }
                Active::Layer(l)
            }
            _ => Active::Consumed,
        };
        self.set_active(p.pos, Some(a));
        self.emit_report();
        self.replay();
    }

    fn resolve_tap(&mut self, _time: u32) {
        let Some(p) = self.pending.take() else { return };
        let tap = match p.action {
            Action::ModTap(..) | Action::LayerTap(..) => Some(tap_of(p.action)),
            _ => None,
        };
        if let Some(a) = tap {
            self.note_typed(a, p.time);
            self.set_active(p.pos, Some(a));
            self.emit_report();
            self.set_active(p.pos, None);
            self.emit_report();
        }
        self.replay();
    }

    /// Reprocessa os eventos guardados enquanto havia uma decisão pendente.
    /// Se um novo tap-hold surgir no meio, `process` volta a guardar os eventos
    /// seguintes no buffer, mantendo a ordem.
    fn replay(&mut self) {
        let mut buf = core::mem::take(&mut self.buffer);
        while let Some(ev) = buf.pop_front() {
            self.process_key(ev);
        }
    }

    fn build_report(&self) -> KeyboardReport {
        let mut r = KeyboardReport::default();
        // Durante uma macro, só ela manda no relatório (Shift segurado não muda o texto).
        if self.player.is_some() {
            r.mods = self.macro_mods;
            for k in &self.macro_keys {
                r.push_key(*k);
            }
            if let Some((m, k)) = self.macro_tap {
                r.mods |= m;
                r.push_key(k);
            }
            return r;
        }
        let combos = self.held_combos.iter().filter_map(|h| h.active.as_ref());
        for a in self.active.iter().flatten().flatten().chain(combos) {
            match *a {
                Active::Key(k) => match k.as_modifier() {
                    Some(m) => r.mods |= m.0,
                    None => r.push_key(k as u8),
                },
                Active::ModKey(m, k) => {
                    r.mods |= m.0;
                    r.push_key(k as u8);
                }
                Active::HoldMods(m) => r.mods |= m.0,
                _ => {}
            }
        }
        r
    }

    fn emit_report(&mut self) {
        let r = self.build_report();
        if r != self.last_report {
            self.last_report = r;
            if self.outputs.push_back(Output::Report(r)).is_err() {
                // Fila cheia: descarta o relatório mais antigo e garante o mais novo.
                self.outputs.pop_front();
                let _ = self.outputs.push_back(Output::Report(r));
            }
        }
    }
}
