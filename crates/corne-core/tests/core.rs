use corne_core::action::{Action, ParseError, RgbCmd};
use corne_core::engine::{Command, Engine, EngineConfig, KeyEvent, Output};
use corne_core::hid::KeyboardReport;
use corne_core::keycode::{Keycode, Mods};
use corne_core::keymap::Keymap;
use corne_core::keymap_file;
use corne_core::layout::{KeyPos, Side};
use corne_core::split::{FrameDecoder, MAX_FRAME, Message};
use corne_core::state::KeyboardState;

/// Cópia fixa do layout para os testes (editar o seu keymap.kbd não quebra os testes).
const KEYMAP: &str = include_str!("fixtures/keymap.kbd");
/// O layout do usuário.
const USER_KEYMAP: &str = include_str!("../../../keymap.kbd");

/// As macros do usuário (o keymap precisa dos nomes delas).
const USER_MACROS: &str = include_str!("../../../macros.kbd");

fn user_keymap() -> keymap_file::ParsedKeymap<'static> {
    let host = keymap_file::read_host(USER_KEYMAP).unwrap_or_else(|e| panic!("erro no keymap.kbd, {e}"));
    // Os segredos (secrets.env) não vão para o git: aqui cada ENV(...) vale "x".
    let m = corne_core::macros::parse_with_env(USER_MACROS, host, &|_| Some("x"))
        .unwrap_or_else(|e| panic!("erro no macros.kbd, {e}"));
    let names: &'static [&'static str] = Box::leak(m.names.to_vec().into_boxed_slice());
    keymap_file::parse_with_macros(USER_KEYMAP, names).unwrap_or_else(|e| panic!("erro no keymap.kbd, {e}"))
}

#[test]
fn user_keymap_and_macros_are_valid() {
    user_keymap();
}

fn none(_: &str) -> Option<u8> {
    None
}

#[test]
fn action_roundtrip_encode_and_text() {
    let names = |s: &str| if s == "nav" { Some(1) } else { None };
    for txt in [
        "A",
        "SPC",
        "LT(nav,SPC)",
        "MT(LSFT,A)",
        "LCTL_T(F)",
        "C(S(TAB))",
        "MO(3)",
        "TG(2)",
        "TO(1)",
        "DF(0)",
        "!",
        "RGB_MOD",
        "OLED_NEXT",
        "BOOT",
        "___",
        "XXX",
        "RA(E)",
        "USER(7)",
    ] {
        let a = Action::parse(txt, &names).unwrap_or_else(|e| panic!("{txt}: {e}"));
        assert_eq!(Action::decode(a.encode()), a, "encode {txt}");
        let shown = a.to_string();
        assert_eq!(Action::parse(&shown, &names).unwrap(), a, "display {txt} -> {shown}");
    }
    assert_eq!(Action::parse("!", &none).unwrap(), Action::ModKey(Mods::LSHIFT, Keycode::N1));
    assert_eq!(
        Action::parse("C(S(TAB))", &none).unwrap(),
        Action::ModKey(Mods::LCTRL.union(Mods::LSHIFT), Keycode::Tab)
    );
    assert!(Action::parse("BANANA", &none).is_err());
    assert!(Action::parse("MO(nav)", &none).is_err());
}

#[test]
fn right_modifiers_survive_encoding() {
    let a = Action::ModKey(Mods::RALT, Keycode::E);
    assert_eq!(Action::decode(a.encode()), a);
}

#[test]
fn keymap_file_parses_default_layout() {
    let parsed = keymap_file::parse(KEYMAP).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(parsed.count, 4);
    assert_eq!(&parsed.names[..4], &["base", "nav", "sym", "adjust"]);
    let km = Keymap::from_encoded(&parsed.layers[..parsed.count]);
    assert_eq!(km.get(0, KeyPos::new(0, 0)), Action::Key(Keycode::Tab));
    assert_eq!(km.get(0, KeyPos::new(0, 11)), Action::Key(Keycode::Backspace));
    assert_eq!(km.get(0, KeyPos::new(3, 4)), Action::Momentary(1));
    assert_eq!(km.get(0, KeyPos::new(3, 7)), Action::Momentary(2));
    assert_eq!(km.get(3, KeyPos::new(1, 0)), Action::Rgb(RgbCmd::Toggle));
}

#[test]
fn keymap_file_reports_errors_with_line() {
    let err = keymap_file::parse("[a]\nQ W\n").err().unwrap();
    assert!(err.to_string().contains("esperava 42"), "{err}");
    let src = format!("[a]\n{}\nFOO\n", "A ".repeat(41));
    let err = keymap_file::parse(&src).err().unwrap();
    assert_eq!(err.line, 3);
    assert_eq!(err.token, "FOO");
}

fn engine() -> Engine {
    let p = keymap_file::parse(KEYMAP).unwrap();
    Engine::new(Keymap::from_encoded(&p.layers[..p.count]), EngineConfig::default())
}

fn press(e: &mut Engine, r: u8, c: u8, t: u32) {
    e.process(KeyEvent { pos: KeyPos::new(r, c), pressed: true, time: t });
}
fn release(e: &mut Engine, r: u8, c: u8, t: u32) {
    e.process(KeyEvent { pos: KeyPos::new(r, c), pressed: false, time: t });
}
fn reports(e: &mut Engine) -> Vec<KeyboardReport> {
    let mut v = vec![];
    while let Some(o) = e.poll_output() {
        if let Output::Report(r) = o {
            v.push(r);
        }
    }
    v
}

#[test]
fn simple_key_press_release() {
    let mut e = engine();
    press(&mut e, 0, 1, 0); // Q
    release(&mut e, 0, 1, 10);
    let r = reports(&mut e);
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].keys[0], Keycode::Q as u8);
    assert_eq!(r[1], KeyboardReport::default());
}

#[test]
fn momentary_layer_and_no_stuck_keys() {
    let mut e = engine();
    press(&mut e, 3, 4, 0); // MO(nav)
    press(&mut e, 1, 6, 5); // LEFT na nav
    release(&mut e, 3, 4, 10); // solta a camada antes da tecla
    release(&mut e, 1, 6, 15);
    let r = reports(&mut e);
    assert_eq!(r[0].keys[0], Keycode::Left as u8);
    assert_eq!(*r.last().unwrap(), KeyboardReport::default());
    assert_eq!(e.layers().highest(), 0);
}

#[test]
fn shifted_symbol_on_layer() {
    let mut e = engine();
    press(&mut e, 3, 7, 0); // MO(sym)
    press(&mut e, 0, 1, 5); // !
    let r = reports(&mut e);
    assert_eq!(r[0].mods, Mods::LSHIFT.0);
    assert_eq!(r[0].keys[0], Keycode::N1 as u8);
}

fn mt_engine(mode: corne_core::engine::TapHoldMode) -> Engine {
    let src = format!("[a]\nLSFT_T(A) B {}\n", "XXX ".repeat(40));
    let p = keymap_file::parse(&src).unwrap();
    let cfg = EngineConfig { mod_tap_mode: mode, ..Default::default() };
    Engine::new(Keymap::from_encoded(&p.layers[..1]), cfg)
}

#[test]
fn mod_tap_tap_and_hold() {
    use corne_core::engine::TapHoldMode::*;
    // toque rápido = A
    let mut e = mt_engine(PermissiveHold);
    press(&mut e, 0, 0, 0);
    release(&mut e, 0, 0, 50);
    let r = reports(&mut e);
    assert_eq!(r[0].keys[0], Keycode::A as u8);
    assert_eq!(r[0].mods, 0);

    // segurar além do tapping term = shift
    let mut e = mt_engine(PermissiveHold);
    press(&mut e, 0, 0, 0);
    e.tick(250);
    let r = reports(&mut e);
    assert_eq!(r[0].mods, Mods::LSHIFT.0);
}

#[test]
fn permissive_hold_rolling_vs_nested() {
    use corne_core::engine::TapHoldMode::*;
    // Rolagem (A↓ B↓ A↑ B↑): digitação rápida => "a" e "b", sem shift.
    let mut e = mt_engine(PermissiveHold);
    press(&mut e, 0, 0, 0);
    press(&mut e, 0, 1, 30);
    release(&mut e, 0, 0, 60);
    release(&mut e, 0, 1, 90);
    let r = reports(&mut e);
    assert!(r.iter().all(|r| r.mods == 0), "{r:?}");
    assert_eq!(r[0].keys[0], Keycode::A as u8);
    assert!(r.iter().any(|r| r.keys.contains(&(Keycode::B as u8))));

    // Aninhado (A↓ B↓ B↑ A↑): => Shift+B
    let mut e = mt_engine(PermissiveHold);
    press(&mut e, 0, 0, 0);
    press(&mut e, 0, 1, 30);
    release(&mut e, 0, 1, 60);
    release(&mut e, 0, 0, 90);
    let r = reports(&mut e);
    assert!(r.iter().any(|r| r.mods == Mods::LSHIFT.0 && r.keys[0] == Keycode::B as u8), "{r:?}");
    assert!(!r.iter().any(|r| r.keys.contains(&(Keycode::A as u8))));
}

#[test]
fn commands_are_emitted() {
    let mut e = engine();
    press(&mut e, 3, 4, 0); // MO(nav)
    press(&mut e, 3, 7, 1); // MO(adjust) na nav
    press(&mut e, 1, 0, 2); // RGB_TOG
    let mut cmds = vec![];
    while let Some(o) = e.poll_output() {
        if let Output::Command(c) = o {
            cmds.push(c);
        }
    }
    assert_eq!(cmds, vec![Command::Rgb(RgbCmd::Toggle)]);
}

#[test]
fn split_frames_roundtrip_and_reject_garbage() {
    let mut st = KeyboardState::default();
    st.layers.on(2);
    st.wpm = 87;
    st.settings.rgb_effect = 3;
    let msgs = [
        Message::Sync { time_ms: 123_456, state: st, ack: 200 },
        Message::Keys { seq: 7, count: 0, events: [(KeyPos::default(), false); 8] },
        Message::Keys {
            seq: 255,
            count: 2,
            events: {
                let mut ev = [(KeyPos::default(), false); 8];
                ev[0] = (KeyPos::from_matrix(Side::Right, 1, 0), true);
                ev[1] = (KeyPos::new(3, 8), false);
                ev
            },
        },
    ];
    let mut dec = FrameDecoder::new();
    for m in msgs {
        let mut buf = [0u8; MAX_FRAME];
        let n = m.encode(&mut buf);
        // lixo antes do quadro
        assert!(dec.push(0x00).is_none());
        let mut got = None;
        for &b in &buf[..n] {
            got = dec.push(b).or(got);
        }
        assert_eq!(got, Some(m));
        // CRC errado é rejeitado
        buf[n - 1] ^= 0xFF;
        let mut got = None;
        for &b in &buf[..n] {
            got = dec.push(b).or(got);
        }
        assert_eq!(got, None);
    }
}

#[test]
fn right_half_is_mirrored() {
    assert_eq!(KeyPos::from_matrix(Side::Right, 0, 0), KeyPos::new(0, 11));
    assert_eq!(KeyPos::from_matrix(Side::Right, 3, 3), KeyPos::new(3, 8));
    assert_eq!(KeyPos::from_matrix(Side::Left, 3, 5), KeyPos::new(3, 5));
    for i in 0..42 {
        assert_eq!(KeyPos::from_index(i).index(), i);
        assert!(KeyPos::from_index(i).exists());
    }
}

#[test]
fn host_layouts_translate_symbols() {
    use corne_core::host::Host;
    let p = |s: &str, h: Host| Action::parse_with(s, h, &none);
    // ABNT2: ç, acentos e símbolos que mudam de lugar.
    assert_eq!(p("Ç", Host::Abnt2).unwrap(), Action::Key(Keycode::Semicolon));
    assert_eq!(p("´", Host::Abnt2).unwrap(), Action::Key(Keycode::LeftBracket));
    assert_eq!(p("~", Host::Abnt2).unwrap(), Action::Key(Keycode::Quote));
    assert_eq!(p("[", Host::Abnt2).unwrap(), Action::Key(Keycode::RightBracket));
    assert_eq!(p("/", Host::Abnt2).unwrap(), Action::Key(Keycode::International1));
    assert_eq!(p("?", Host::Abnt2).unwrap(), Action::ModKey(Mods::LSHIFT, Keycode::International1));
    assert_eq!(p("!", Host::Abnt2).unwrap(), Action::ModKey(Mods::LSHIFT, Keycode::N1));
    assert_eq!(p("LSFT_T(Ç)", Host::Abnt2).unwrap(), Action::ModTap(Mods::LSHIFT, Keycode::Semicolon));
    // US internacional: ç = AltGr + vírgula.
    assert_eq!(p("ç", Host::UsIntl).unwrap(), Action::ModKey(Mods::RALT, Keycode::Comma));
    assert_eq!(p("[", Host::UsIntl).unwrap(), Action::Key(Keycode::LeftBracket));
    assert_eq!(p("S(ç)", Host::UsIntl), Err(ParseError::MixedMods));
    // US: sem acentos.
    assert_eq!(p("Ç", Host::Us), Err(ParseError::NeedsHost));
    assert_eq!(p("[", Host::Us).unwrap(), Action::Key(Keycode::LeftBracket));

    // O texto mostrado volta para a mesma ação.
    for h in Host::ALL {
        for txt in [
            "Ç", "´", "`", "~", "^", "¨", "[", "{", "]", "}", "/", "?", ";", ":", "\\", "|", "'", "\"", "!", "S(Ç)",
            "C(´)", "LT(1,Ç)", "A", "RA(E)",
        ] {
            let Ok(a) = p(txt, h) else { continue };
            let shown = a.label(h).to_string();
            assert_eq!(p(&shown, h).unwrap(), a, "{h:?}: {txt} -> {shown}");
        }
    }
    assert_eq!(p("Ç", Host::Abnt2).unwrap().label(Host::Abnt2).to_string(), "Ç");
    assert_eq!(Action::Key(Keycode::Slash).label(Host::Abnt2).to_string(), ";");
}

#[test]
fn keymap_host_directive() {
    let body = "Q ".repeat(41);
    let src = format!("host = abnt2\n[a]\n{body} Ç\n");
    let p = keymap_file::parse(&src).unwrap();
    assert_eq!(p.host, corne_core::host::Host::Abnt2);
    let err = keymap_file::parse("host = klingon\n[a]\n").err().unwrap();
    assert!(err.to_string().contains("klingon"), "{err}");
    let no_host = format!("[a]\n{body} Ç\n");
    let err = keymap_file::parse(&no_host).err().unwrap();
    assert!(err.to_string().contains("host"), "{err}");
}

#[test]
fn engine_replay_keeps_every_report() {
    use corne_core::engine::TapHoldMode::*;
    // Tap-hold pendente enquanto 8 teclas são tocadas (16 eventos no buffer).
    let src = format!("[a]\nLSFT_T(A) B C D E F G H I {}\n", "XXX ".repeat(33));
    let p = keymap_file::parse(&src).unwrap();
    let cfg = EngineConfig { mod_tap_mode: TimeoutOnly, ..Default::default() };
    let mut e = Engine::new(Keymap::from_encoded(&p.layers[..1]), cfg);
    press(&mut e, 0, 0, 0);
    for c in 1..=8u8 {
        press(&mut e, 0, c, c as u32 * 4);
        release(&mut e, 0, c, c as u32 * 4 + 2);
    }
    release(&mut e, 0, 0, 60);
    let r = reports(&mut e);
    // A (aperta/solta) + 8 teclas (aperta/solta), nenhum relatório perdido.
    assert_eq!(r.len(), 18, "{r:?}");
    for k in [Keycode::A, Keycode::B, Keycode::E, Keycode::I] {
        assert!(r.iter().any(|r| r.keys[0] == k as u8), "{k:?}");
    }
}

mod debounce {
    use corne_core::debounce::{DebounceMode, Debouncer};
    use corne_core::layout::{HALF_COLS, HALF_ROWS};

    /// Alimenta uma sequência (tempo, tecla 0,0 apertada?) e devolve as mudanças.
    fn run(mode: DebounceMode, seq: &[(u32, bool)]) -> Vec<(u32, bool)> {
        let mut d = Debouncer::new(mode, 5);
        let mut out = vec![];
        for &(t, down) in seq {
            let mut raw = [[false; HALF_COLS]; HALF_ROWS];
            raw[0][0] = down;
            d.update(&raw, t, |_, _, p| out.push((t, p)));
        }
        out
    }

    /// Leituras a cada 1 ms de `from` até `to` (exclusivo) com o mesmo valor.
    fn hold(from: u32, to: u32, down: bool) -> Vec<(u32, bool)> {
        (from..to).map(|t| (t, down)).collect()
    }

    #[test]
    fn eager_is_immediate_and_ignores_bounce() {
        // Repique no aperto e na soltura.
        let mut seq = vec![(0, true), (1, false), (2, true), (3, false), (4, true)];
        seq.extend(hold(5, 40, true));
        seq.extend([(40, false), (41, true), (42, false)]);
        seq.extend(hold(43, 60, false));
        let out = run(DebounceMode::Eager, &seq);
        assert_eq!(out, vec![(0, true), (40, false)]);
    }

    #[test]
    fn eager_never_loses_a_short_tap() {
        // Toque de 2 ms: o release sai no fim da trava.
        let mut seq = vec![(0, true), (1, true)];
        seq.extend(hold(2, 20, false));
        let out = run(DebounceMode::Eager, &seq);
        assert_eq!(out, vec![(0, true), (5, false)]);
    }

    #[test]
    fn defer_waits_for_stable_signal() {
        let mut seq = vec![(0, true), (1, false)];
        seq.extend(hold(2, 20, true));
        let out = run(DebounceMode::Defer, &seq);
        assert_eq!(out, vec![(7, true)]);
    }
}

#[test]
fn split_seq_never_collides_with_no_ack() {
    use corne_core::split::{NO_ACK, next_seq};
    assert_eq!(next_seq(NO_ACK), 0);
    let mut s = 0u8;
    for _ in 0..600 {
        s = next_seq(s);
        assert!(s != 0 && s != NO_ACK);
    }
}

#[test]
fn protocol_get_info_reports_side() {
    use corne_core::protocol::{self, ConfigTarget, PACKET, VERSION};
    use corne_core::state::Settings;
    struct T(Keymap, Side);
    impl ConfigTarget for T {
        fn keymap(&mut self) -> &mut Keymap {
            &mut self.0
        }
        fn reset_keymap(&mut self) {}
        fn get_settings(&mut self) -> Settings {
            Settings::DEFAULT
        }
        fn set_settings(&mut self, _: Settings) {}
        fn save(&mut self) -> bool {
            false
        }
        fn bootloader(&mut self) {}
        fn side(&self) -> Side {
            self.1
        }
    }
    let p = keymap_file::parse(KEYMAP).unwrap();
    let mut req = [0u8; PACKET];
    req[0] = 0x01;
    for (side, byte) in [(Side::Left, 0), (Side::Right, 1)] {
        let mut t = T(Keymap::from_encoded(&p.layers[..p.count]), side);
        let res = protocol::handle(&req, &mut t);
        assert_eq!(&res[..2], &[0x01, VERSION]);
        assert_eq!(res[6], byte);
    }
}

mod combos {
    use super::*;
    use corne_core::combo;

    const KM: &str = "
[base]
ESC  Q W E R T    Y U I O P BSPC
LSFT A S D F G    H J K L Ç ~
LCTL Z X C V B    N M , . / XXX
       LGUI MO(nav) ENT   SPC BSPC RALT
[nav]
___  1 2 3 4 5    6 7 8 9 0 ___
___  ___ ___ ___ ___ ___   LEFT DOWN UP RGHT ___ ___
___  ___ ___ ___ ___ ___   ___ ___ ___ ___ ___ ___
       ___ ___ ___   ___ ___ ___
";

    const COMBOS: &str = "
term = 40
W E   => ESC
W E R => TAB      // superconjunto de W E
A S   => USER(3)
K(0,0) K(0,11) => BOOT
[nav]
LEFT DOWN => C(Z)
";

    fn host_km() -> String {
        format!("host = abnt2\n{KM}")
    }

    fn setup() -> Engine {
        let src: &'static str = Box::leak(host_km().into_boxed_str());
        let km = keymap_file::parse(src).unwrap();
        let parsed = combo::parse(COMBOS, &km).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(parsed.term_ms, 40);
        let list: &'static [combo::Combo] = Box::leak(parsed.combos.to_vec().into_boxed_slice());
        Engine::new(Keymap::from_encoded(&km.layers[..km.count]), EngineConfig::default()).with_combos(
            list,
            parsed.term_ms,
            parsed.idle_ms,
        )
    }

    fn keys(r: &[KeyboardReport]) -> Vec<Vec<u8>> {
        // Teclas de cada relatório, em ordem (a posição no relatório HID não importa).
        r.iter()
            .map(|r| {
                let mut v: Vec<u8> = r.keys.iter().copied().filter(|k| *k != 0).collect();
                v.sort();
                v
            })
            .collect()
    }

    const W: u8 = Keycode::W as u8;
    const E: u8 = Keycode::E as u8;
    const Q: u8 = Keycode::Q as u8;

    #[test]
    fn three_key_combo_fires_immediately_and_releases_on_first_up() {
        let mut e = setup();
        press(&mut e, 0, 2, 0); // W
        press(&mut e, 0, 3, 5); // E (W E existe, mas W E R ainda pode vir)
        assert!(reports(&mut e).is_empty());
        press(&mut e, 0, 4, 10); // R -> TAB na hora
        release(&mut e, 0, 2, 60); // soltar W solta o TAB
        release(&mut e, 0, 3, 70);
        release(&mut e, 0, 4, 80);
        assert_eq!(keys(&reports(&mut e)), vec![vec![Keycode::Tab as u8], vec![]]);
    }

    #[test]
    fn two_key_combo_fires_after_term_when_bigger_is_possible() {
        let mut e = setup();
        press(&mut e, 0, 2, 0);
        press(&mut e, 0, 3, 5);
        e.tick(39);
        assert!(reports(&mut e).is_empty());
        e.tick(40);
        assert_eq!(keys(&reports(&mut e)), vec![vec![Keycode::Escape as u8]]);
    }

    #[test]
    fn lone_tap_and_other_keys_come_out_in_order() {
        // W tocado sozinho sai ao soltar.
        let mut e = setup();
        press(&mut e, 0, 2, 0);
        release(&mut e, 0, 2, 20);
        assert_eq!(keys(&reports(&mut e)), vec![vec![W], vec![]]);

        // W e depois Q (fora de combos): W sai antes de Q.
        let mut e = setup();
        press(&mut e, 0, 2, 0);
        press(&mut e, 0, 1, 10);
        assert_eq!(keys(&reports(&mut e)), vec![vec![W], vec![Q, W]]);

        // W segurado além da janela sai sozinho.
        let mut e = setup();
        press(&mut e, 0, 2, 0);
        e.tick(45);
        assert_eq!(keys(&reports(&mut e)), vec![vec![W]]);

        // Q não participa de combos: sai na hora.
        let mut e = setup();
        press(&mut e, 0, 1, 0);
        assert_eq!(keys(&reports(&mut e)), vec![vec![Q]]);

        // E depois W fora da janela: duas teclas normais.
        let mut e = setup();
        press(&mut e, 0, 3, 0);
        press(&mut e, 0, 2, 50);
        e.tick(100);
        assert_eq!(keys(&reports(&mut e)), vec![vec![E], vec![E, W]]);
    }

    #[test]
    fn user_action_and_explicit_positions() {
        let mut e = setup();
        press(&mut e, 1, 1, 0); // A
        press(&mut e, 1, 2, 3); // S
        release(&mut e, 1, 1, 50);
        release(&mut e, 1, 2, 55);
        let mut cmds = vec![];
        while let Some(o) = e.poll_output() {
            if let Output::Command(c) = o {
                cmds.push(c);
            }
        }
        assert_eq!(cmds, vec![Command::User(3, true), Command::User(3, false)]);

        let mut e = setup();
        press(&mut e, 0, 0, 0);
        press(&mut e, 0, 11, 2);
        let cmds: Vec<_> = std::iter::from_fn(|| e.poll_output()).collect();
        assert!(cmds.contains(&Output::Command(Command::Bootloader)), "{cmds:?}");
    }

    #[test]
    fn layer_combo_only_on_its_layer() {
        // Na base, J K (posições de LEFT DOWN na nav) são teclas normais.
        let mut e = setup();
        press(&mut e, 1, 6, 0);
        press(&mut e, 1, 7, 2);
        e.tick(50);
        let r = reports(&mut e);
        assert!(r.iter().all(|r| r.mods == 0), "{r:?}");

        // Com a nav segurada, viram Ctrl+Z.
        let mut e = setup();
        press(&mut e, 3, 4, 0); // MO(nav)
        press(&mut e, 1, 6, 10);
        press(&mut e, 1, 7, 12);
        let r = reports(&mut e);
        assert!(r.iter().any(|r| r.mods == Mods::LCTRL.0 && r.keys[0] == Keycode::Z as u8), "{r:?}");
    }

    #[test]
    fn combo_file_errors() {
        let src = host_km();
        let km = keymap_file::parse(&src).unwrap();
        let err = |s: &str| combo::parse(s, &km).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err("W => ESC").contains("pelo menos 2"));
        assert!(err("W E ESC").contains("=>"));
        assert!(err("W NUM => ESC").contains("NUM"));
        assert!(err("W W => ESC").contains("repetida"));
        assert!(err("BSPC W => ESC").contains("mais de uma posição"));
        assert!(err("K(3,0) W => ESC").contains("posição inválida"));
        assert!(err("[nada]\nW E => ESC").contains("nada"));
        assert!(err("W E => XXX").contains("não faz nada"));
        assert!(err("W E => ESC\nE W => TAB").contains("já existe"));
        assert!(err("term = 0").contains("term"));
        // acentos usam o host do keymap
        assert!(combo::parse("Ç ~ => ´", &km).is_ok());
    }
}

#[test]
fn user_combos_are_valid() {
    let km = user_keymap();
    if let Err(e) = corne_core::combo::parse(include_str!("../../../combos.kbd"), &km) {
        panic!("erro no combos.kbd, {e}");
    }
}

mod macros {
    use super::*;
    use corne_core::host::Host;
    use corne_core::macros::{self, MacroTable, Step, type_char};

    const N: Mods = Mods::NONE;
    const S: Mods = Mods::LSHIFT;

    fn chars(h: Host, c: char) -> Vec<(Mods, Keycode)> {
        type_char(h, c).unwrap_or_else(|e| panic!("{c}: {e}")).to_vec()
    }

    #[test]
    fn text_is_typed_with_the_host_layout() {
        use Keycode as K;
        let a = Host::Abnt2;
        assert_eq!(chars(a, 'a'), vec![(N, K::A)]);
        assert_eq!(chars(a, 'A'), vec![(S, K::A)]);
        assert_eq!(chars(a, 'ç'), vec![(N, K::Semicolon)]);
        assert_eq!(chars(a, 'Ç'), vec![(S, K::Semicolon)]);
        assert_eq!(chars(a, 'á'), vec![(N, K::LeftBracket), (N, K::A)]);
        assert_eq!(chars(a, 'Ã'), vec![(N, K::Quote), (S, K::A)]);
        assert_eq!(chars(a, 'ê'), vec![(S, K::Quote), (N, K::E)]);
        assert_eq!(chars(a, '~'), vec![(N, K::Quote), (N, K::Space)]); // acento sozinho
        assert_eq!(chars(a, '/'), vec![(N, K::International1)]);
        assert_eq!(chars(a, '@'), vec![(S, K::N2)]);
        assert_eq!(chars(a, '_'), vec![(S, K::Minus)]);
        assert_eq!(chars(a, '\n'), vec![(N, K::Enter)]);

        let i = Host::UsIntl;
        assert_eq!(chars(i, 'ç'), vec![(Mods::RALT, K::Comma)]);
        assert_eq!(chars(i, 'Ç'), vec![(Mods::RALT.union(Mods::RSHIFT), K::Comma)]);
        assert_eq!(chars(i, 'ã'), vec![(S, K::Grave), (N, K::A)]);
        assert_eq!(chars(i, '\''), vec![(N, K::Quote), (N, K::Space)]);

        assert!(type_char(Host::Us, 'é').is_err());
        assert_eq!(chars(Host::Us, '\''), vec![(N, K::Quote)]);
    }

    #[test]
    fn steps_roundtrip() {
        for s in [
            Step::Tap(N, Keycode::A),
            Step::Tap(Mods::LCTRL.union(S), Keycode::Tab),
            Step::Tap(Mods::RALT, Keycode::Comma),
            Step::Down(Keycode::LShift),
            Step::Up(Keycode::Z),
            Step::Wait(1234),
        ] {
            assert_eq!(Step::decode(s.encode()), Some(s));
        }
    }

    const MACROS: &str = r#"
delay = 0
[oi]
"Oi, ção!"   // comentário
[url]
"http://a.b" ENT
[atalho]
C(S(TAB)) WAIT(10)
DOWN(LSFT) X UP(LSFT)
"#;

    #[test]
    fn macro_file_parses() {
        let m = macros::parse(MACROS, Host::Abnt2).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&m.names[..], &["oi", "url", "atalho"]);
        let get = |n: usize| -> Vec<Step> {
            let (s, l) = m.spans[n];
            m.steps[s as usize..(s + l) as usize].iter().map(|v| Step::decode(*v).unwrap()).collect()
        };
        // "Oi, ção!" => O i , espaço ç ~ a o ! (o ~ de ã é tecla morta + a)
        let oi = get(0);
        assert_eq!(oi.len(), 9);
        assert_eq!(oi[0], Step::Tap(S, Keycode::O));
        assert_eq!(oi[4], Step::Tap(N, Keycode::Semicolon));
        assert_eq!(oi[5], Step::Tap(N, Keycode::Quote));
        // O // dentro das aspas não é comentário.
        assert_eq!(get(1).len(), "http://a.b".len() + 1);
        assert_eq!(
            get(2),
            vec![
                Step::Tap(Mods::LCTRL.union(S), Keycode::Tab),
                Step::Wait(10),
                Step::Down(Keycode::LShift),
                Step::Tap(N, Keycode::X),
                Step::Up(Keycode::LShift),
            ]
        );
    }

    #[test]
    fn apostrophe_in_and_around_text() {
        let steps = |src: &str| -> Vec<Step> {
            let m = macros::parse(src, Host::Abnt2).unwrap_or_else(|e| panic!("{src}: {e}"));
            m.steps.iter().map(|v| Step::decode(*v).unwrap()).collect()
        };
        let apos = Step::Tap(N, Keycode::Grave); // ' no ABNT2
        let quote = Step::Tap(S, Keycode::Grave); // " no ABNT2
        // Aspas simples delimitam texto, e o " dentro dele é caractere.
        assert_eq!(steps("[a]\n'a\"b'"), vec![Step::Tap(N, Keycode::A), quote, Step::Tap(N, Keycode::B)]);
        // Apóstrofo dentro de aspas duplas, e escapado dentro de aspas simples.
        assert_eq!(steps("[a]\n\"d'a\""), vec![Step::Tap(N, Keycode::D), apos, Step::Tap(N, Keycode::A)]);
        assert_eq!(steps("[a]\n'd\\'a'"), vec![Step::Tap(N, Keycode::D), apos, Step::Tap(N, Keycode::A)]);
        // Um ' sozinho é a tecla do apóstrofo.
        assert_eq!(steps("[a]\nX ' Y"), vec![Step::Tap(N, Keycode::X), apos, Step::Tap(N, Keycode::Y)]);
        assert!(macros::parse("[a]\n'sem fim", Host::Abnt2).err().unwrap().to_string().contains("aspas"));
    }

    #[test]
    fn macro_file_errors() {
        let err = |s: &str| macros::parse(s, Host::Abnt2).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err("[a]\n\"sem fim").contains("aspas"));
        assert!(err("[a]\n[b]\nX").contains("vazia"));
        assert!(err("X").contains("antes de qualquer"));
        assert!(err("[a]\nWAIT(99999)").contains("WAIT"));
        assert!(err("[a]\nMO(1)").contains("não é uma tecla"));
        assert!(err("[a]\nX\n[a]\nY").contains("duas vezes"));
        assert!(err("[a b]\nX").contains("inválido"));
        assert!(err("[a]\n\"\\q\"").contains("escape"));
        assert!(macros::parse("[a]\n\"é\"", Host::Us).err().unwrap().to_string().contains("host"));
    }

    fn setup(keymap_extra: &str, combos: &str) -> Engine {
        let msrc: &'static str = Box::leak(MACROS.to_string().into_boxed_str());
        let m = macros::parse(msrc, Host::Abnt2).unwrap();
        let ksrc: &'static str = Box::leak(
            format!("host = abnt2\n[base]\nM(oi) M(atalho) W E {keymap_extra} {}\n", "XXX ".repeat(38))
                .into_boxed_str(),
        );
        let km = keymap_file::parse_with_macros(ksrc, &m.names).unwrap_or_else(|e| panic!("{e}"));
        let c = combo::parse(combos, &km).unwrap_or_else(|e| panic!("{e}"));
        let table = MacroTable {
            steps: Box::leak(m.steps.to_vec().into_boxed_slice()),
            spans: Box::leak(m.spans.to_vec().into_boxed_slice()),
            delay_ms: m.delay_ms,
        };
        Engine::new(Keymap::from_encoded(&km.layers[..1]), EngineConfig::default())
            .with_combos(Box::leak(c.combos.to_vec().into_boxed_slice()), c.term_ms, c.idle_ms)
            .with_macros(table)
    }
    use corne_core::combo;

    /// Teclas apertadas, em ordem (só as transições de "apertou").
    fn typed(e: &mut Engine) -> Vec<(u8, u8)> {
        let mut prev = KeyboardReport::default();
        let mut out = vec![];
        for r in reports(e) {
            for k in r.keys.iter().filter(|k| **k != 0 && !prev.keys.contains(k)) {
                out.push((r.mods, *k));
            }
            prev = r;
        }
        out
    }

    #[test]
    fn key_plays_macro_text() {
        let mut e = setup("", "");
        press(&mut e, 0, 0, 0); // M(oi)
        release(&mut e, 0, 0, 30);
        e.tick(31);
        let t = typed(&mut e);
        assert_eq!(t.len(), 9, "{t:?}");
        assert_eq!(t[0], (S.0, Keycode::O as u8));
        assert_eq!(t[1], (0, Keycode::I as u8));
        assert!(!e.macro_playing());
        assert_eq!(e.mods(), Mods::NONE);
    }

    #[test]
    fn keys_pressed_during_macro_wait_come_after_it() {
        let mut e = setup("", "");
        press(&mut e, 0, 1, 0); // M(atalho): C(S(TAB)) WAIT(10) ...
        release(&mut e, 0, 1, 2);
        press(&mut e, 0, 2, 3); // W durante o WAIT
        release(&mut e, 0, 2, 4);
        assert!(e.macro_playing());
        e.tick(11);
        e.tick(12);
        let t = typed(&mut e);
        let tab = (Mods::LCTRL.union(S).0, Keycode::Tab as u8);
        let x = (S.0, Keycode::X as u8);
        assert_eq!(t, vec![tab, x, (0, Keycode::W as u8)]);
    }

    #[test]
    fn combo_plays_macro() {
        let mut e = setup("", "W E => M(oi)");
        press(&mut e, 0, 2, 0);
        press(&mut e, 0, 3, 5);
        release(&mut e, 0, 2, 40);
        release(&mut e, 0, 3, 41);
        e.tick(50);
        assert_eq!(typed(&mut e).len(), 9);
    }

    #[test]
    fn long_macro_loses_nothing() {
        // 200 letras: muito mais do que cabe na fila de saídas de uma vez.
        let text = "abcdefghij".repeat(20);
        let src: &'static str = Box::leak(format!("[long]\n\"{text}\"\n").into_boxed_str());
        let m = macros::parse(src, Host::Us).unwrap();
        let table = MacroTable {
            steps: Box::leak(m.steps.to_vec().into_boxed_slice()),
            spans: Box::leak(m.spans.to_vec().into_boxed_slice()),
            delay_ms: 0,
        };
        let ksrc: &'static str = Box::leak(format!("[a]\nM(0) {}\n", "XXX ".repeat(41)).into_boxed_str());
        let km = keymap_file::parse(ksrc).unwrap();
        let mut e = Engine::new(Keymap::from_encoded(&km.layers[..1]), EngineConfig::default()).with_macros(table);
        press(&mut e, 0, 0, 0);
        let mut n = 0;
        for t in 1..200 {
            // Esvazia a fila como o firmware faz a cada ms.
            n += reports(&mut e).len();
            e.tick(t);
        }
        n += reports(&mut e).len();
        assert_eq!(n, 400); // aperta + solta de cada letra
    }
}

#[test]
fn key_names_ignore_case_and_macro_reads_clearly() {
    assert_eq!(Action::parse("p", &none).unwrap(), Action::Key(Keycode::P));
    assert_eq!(Action::parse("esc", &none).unwrap(), Action::Key(Keycode::Escape));
    assert_eq!(Action::parse("c(z)", &none), Err(ParseError::UnknownKey)); // funções continuam em maiúsculas
    let macros = |s: &str| if s == "prediction1" { Some(4) } else { None };
    let a = Action::parse_full("MACRO(prediction1)", corne_core::Host::Us, &none, &macros).unwrap();
    assert_eq!(a, Action::Macro(4));
    assert_eq!(Action::parse_full("M(prediction1)", corne_core::Host::Us, &none, &macros).unwrap(), a);
    assert_eq!(a.to_string(), "MACRO(4)");
    assert_eq!(Action::decode(a.encode()), a);
}

#[test]
fn explicit_modifier_names() {
    let p = |s: &str| Action::parse(s, &none).unwrap_or_else(|e| panic!("{s}: {e}"));
    let cs_tab = Action::ModKey(Mods::LCTRL.union(Mods::LSHIFT), Keycode::Tab);
    assert_eq!(p("CTRL(SHIFT(TAB))"), cs_tab);
    assert_eq!(p("C(S(TAB))"), cs_tab); // nomes curtos continuam valendo
    assert_eq!(cs_tab.to_string(), "CTRL(SHIFT(TAB))");
    assert_eq!(p("SHIFT"), Action::Key(Keycode::LShift));
    assert_eq!(p("LSFT"), Action::Key(Keycode::LShift));
    assert_eq!(Action::Key(Keycode::LGui).to_string(), "WIN");
    assert_eq!(p("ALTGR(E)"), Action::ModKey(Mods::RALT, Keycode::E));
    assert_eq!(Action::ModKey(Mods::RALT, Keycode::E).to_string(), "ALTGR(E)");
    assert_eq!(p("SHIFT_T(A)"), Action::ModTap(Mods::LSHIFT, Keycode::A));
    assert_eq!(Action::ModTap(Mods::LSHIFT, Keycode::A).to_string(), "MT(SHIFT,A)");
    assert_eq!(p("MT(CTRL|SHIFT, A)"), Action::ModTap(Mods::LCTRL.union(Mods::LSHIFT), Keycode::A));
}

#[test]
fn layer_tap_with_shifted_tap_key() {
    use corne_core::host::Host;
    let names = |s: &str| if s == "setup" { Some(3) } else { None };
    // ? no ABNT2 = Shift + tecla extra ao lado do Shift direito.
    let a = Action::parse_with("LT(setup, ?)", Host::Abnt2, &names).unwrap();
    assert_eq!(a, Action::LayerTap(3, Mods::LSHIFT, Keycode::International1));
    assert_eq!(Action::decode(a.encode()), a);
    assert_eq!(a.label(Host::Abnt2).to_string(), "LT(3,?)");
    assert_eq!(Action::parse_with("LT(3,?)", Host::Abnt2, &names).unwrap(), a);
    // Toque simples continua igual.
    let b = Action::parse("LT(1, SPC)", &none).unwrap();
    assert_eq!(b, Action::LayerTap(1, Mods::NONE, Keycode::Space));
    assert_eq!(Action::decode(b.encode()), b);
    // Outros modificadores no toque não cabem.
    assert_eq!(Action::parse("LT(1, CTRL(A))", &none), Err(ParseError::TapKeyWithMods));
    assert_eq!(Action::parse("MT(CTRL, ?)", &none), Err(ParseError::TapKeyWithMods));

    // No motor: tocar envia Shift + tecla; segurar liga a camada.
    let src = "host = abnt2\n[a]\nLT(b, ?) ".to_string() + &"XXX ".repeat(41) + "\n[b]\n" + &"___ ".repeat(42);
    let km = keymap_file::parse(&src).unwrap();
    let mut e = Engine::new(Keymap::from_encoded(&km.layers[..km.count]), EngineConfig::default());
    press(&mut e, 0, 0, 0);
    release(&mut e, 0, 0, 50);
    let r = reports(&mut e);
    assert_eq!(r[0].mods, Mods::LSHIFT.0);
    assert_eq!(r[0].keys[0], Keycode::International1 as u8);
    assert_eq!(*r.last().unwrap(), KeyboardReport::default());
    // (depois de uma pausa: logo após o `?` seria toque pelo flow tap)
    press(&mut e, 0, 0, 500);
    e.tick(800);
    assert_eq!(e.layers().highest(), 1);
}

/// Digitação com `LT(sym, SPC)` no polegar: o espaço não pode virar camada.
mod typing {
    use super::*;
    use corne_core::engine::TapHoldMode;

    // [base]: A na (0,1), B na (0,2), LT(sym, SPC) na (3,6)
    // [sym]:  ! na (0,1)
    fn setup(cfg: EngineConfig) -> Engine {
        let base = "XXX A B ".to_string() + &"XXX ".repeat(33) + "XXX XXX XXX LT(sym, SPC) XXX XXX";
        let sym = "XXX ! ".to_string() + &"___ ".repeat(40);
        let src = format!("host = abnt2\n[base]\n{base}\n[sym]\n{sym}\n");
        let km = keymap_file::parse(&src).unwrap();
        Engine::new(Keymap::from_encoded(&km.layers[..km.count]), cfg)
    }
    const SPC: (u8, u8) = (3, 6);
    fn typed(e: &mut Engine) -> Vec<(u8, u8)> {
        // (mods, tecla) de cada tecla nova que aparece nos relatórios.
        let mut out = vec![];
        let mut prev: Vec<u8> = vec![];
        for r in reports(e) {
            for k in r.keys.iter().filter(|k| **k != 0) {
                if !prev.contains(k) {
                    out.push((r.mods, *k));
                }
            }
            prev = r.keys.to_vec();
        }
        out
    }
    const A: (u8, u8) = (0, Keycode::A as u8);
    const B: (u8, u8) = (0, Keycode::B as u8);
    const SP: (u8, u8) = (0, Keycode::Space as u8);
    const BANG: (u8, u8) = (Mods::LSHIFT.0, Keycode::N1 as u8);

    #[test]
    fn space_after_a_letter_is_always_a_tap() {
        // "a b" com o B descendo antes de o espaço subir e soltando antes dele
        // (o pior caso: seria camada no modo antigo e até no permissive hold).
        let mut e = setup(EngineConfig::default());
        press(&mut e, 0, 1, 0);
        release(&mut e, 0, 1, 40);
        press(&mut e, SPC.0, SPC.1, 90); // 90 ms depois do A: flow tap
        press(&mut e, 0, 2, 120);
        release(&mut e, 0, 2, 150);
        release(&mut e, SPC.0, SPC.1, 170);
        assert_eq!(typed(&mut e), vec![A, SP, B]);
        assert_eq!(e.layers().highest(), 0);
    }

    #[test]
    fn rolling_after_a_pause_is_a_tap() {
        // Começo de frase, sem letra antes: SPC↓ B↓ SPC↑ B↑ (rolagem).
        let mut e = setup(EngineConfig::default());
        press(&mut e, SPC.0, SPC.1, 0);
        press(&mut e, 0, 2, 30);
        release(&mut e, SPC.0, SPC.1, 60);
        release(&mut e, 0, 2, 90);
        assert_eq!(typed(&mut e), vec![SP, B]);

        // O modo antigo (HoldOnOtherKeyPress) engasgava aqui: saía o símbolo.
        let old = EngineConfig { layer_tap_mode: TapHoldMode::HoldOnOtherKeyPress, ..Default::default() };
        let mut e = setup(old);
        press(&mut e, SPC.0, SPC.1, 0);
        press(&mut e, 0, 1, 30);
        release(&mut e, SPC.0, SPC.1, 60);
        release(&mut e, 0, 1, 90);
        assert_eq!(typed(&mut e), vec![BANG]);
    }

    #[test]
    fn holding_space_still_gives_the_layer() {
        // Pausa, segura o espaço e toca A: !
        let mut e = setup(EngineConfig::default());
        press(&mut e, SPC.0, SPC.1, 0);
        press(&mut e, 0, 1, 60);
        release(&mut e, 0, 1, 100);
        release(&mut e, SPC.0, SPC.1, 140);
        assert_eq!(typed(&mut e), vec![BANG]);

        // Segura além do tapping term: camada direto.
        let mut e = setup(EngineConfig::default());
        press(&mut e, SPC.0, SPC.1, 0);
        e.tick(250);
        assert_eq!(e.layers().highest(), 1);
        press(&mut e, 0, 1, 300);
        release(&mut e, 0, 1, 320);
        release(&mut e, SPC.0, SPC.1, 400);
        assert_eq!(typed(&mut e), vec![BANG]); // sem espaço no fim (camada foi usada)
    }

    #[test]
    fn long_press_without_other_key_still_types_space() {
        let mut e = setup(EngineConfig::default());
        press(&mut e, SPC.0, SPC.1, 0);
        e.tick(300);
        release(&mut e, SPC.0, SPC.1, 350);
        assert_eq!(typed(&mut e), vec![SP]);
        assert_eq!(e.layers().highest(), 0);
        assert_eq!(e.mods(), Mods::NONE);

        // Desligado: segurar e soltar não envia nada.
        let mut e = setup(EngineConfig { retro_tap: false, ..Default::default() });
        press(&mut e, SPC.0, SPC.1, 0);
        e.tick(300);
        release(&mut e, SPC.0, SPC.1, 350);
        assert_eq!(typed(&mut e), vec![]);
    }

    #[test]
    fn letter_held_before_space_is_released_right_away() {
        // A↓ SPC↓ A↑: o A sobe na hora, mesmo com o espaço indeciso.
        let mut e = setup(EngineConfig { flow_tap_ms: 0, ..Default::default() });
        press(&mut e, 0, 1, 0);
        press(&mut e, SPC.0, SPC.1, 20);
        release(&mut e, 0, 1, 40);
        let r = reports(&mut e);
        assert_eq!(*r.last().unwrap(), KeyboardReport::default(), "{r:?}");
    }
}

/// Combos não disparam no meio de uma palavra.
#[test]
fn combo_waits_for_a_pause_in_typing() {
    use corne_core::combo;
    let base = "XXX A B C ".to_string() + &"XXX ".repeat(38);
    let src: &'static str = Box::leak(format!("host = abnt2\n[base]\n{base}\n").into_boxed_str());
    let km = keymap_file::parse(src).unwrap();
    let c = combo::parse("idle = 150\nB C => ESC\n", &km).unwrap();
    assert_eq!(c.idle_ms, 150);
    let list: &'static [combo::Combo] = Box::leak(c.combos.to_vec().into_boxed_slice());
    let make =
        || Engine::new(Keymap::from_encoded(&km.layers[..1]), EngineConfig::default()).with_combos(list, 40, 150);
    let has = |r: &[KeyboardReport], k: Keycode| r.iter().any(|r| r.keys.contains(&(k as u8)));

    // "abc" rápido, com B e C sobrepostos: texto, sem ESC.
    let mut e = make();
    press(&mut e, 0, 1, 0);
    release(&mut e, 0, 1, 30);
    press(&mut e, 0, 2, 80);
    press(&mut e, 0, 3, 100);
    release(&mut e, 0, 2, 120);
    release(&mut e, 0, 3, 140);
    let r = reports(&mut e);
    assert!(!has(&r, Keycode::Escape) && has(&r, Keycode::B) && has(&r, Keycode::C), "{r:?}");

    // Depois de uma pausa: combo.
    let mut e = make();
    press(&mut e, 0, 1, 0);
    release(&mut e, 0, 1, 30);
    press(&mut e, 0, 2, 400);
    press(&mut e, 0, 3, 410);
    let r = reports(&mut e);
    assert!(has(&r, Keycode::Escape) && !has(&r, Keycode::B), "{r:?}");

    assert!(combo::parse("idle = 5000\nB C => ESC\n", &km).is_err());
}

#[test]
fn secrets_env_file_and_env_steps() {
    use corne_core::host::Host;
    use corne_core::macros::{self, Step, env_value};
    let file = "# comentário\nSENHA=abc 123  \nFRASE=\"  dois  \"\nexport PIN=0204\nPIN=9999\nVAZIO=\n";
    assert_eq!(env_value(file, "SENHA"), Some("abc 123"));
    assert_eq!(env_value(file, "FRASE"), Some("  dois  "));
    assert_eq!(env_value(file, "PIN"), Some("9999")); // a última vale
    assert_eq!(env_value(file, "VAZIO"), Some(""));
    assert_eq!(env_value(file, "NADA"), None);

    let env = |n: &str| env_value(file, n);
    let src = "[pin]\nENV(PIN) ENT\n[normal]\n\"oi\"\n";
    let m = macros::parse_with_env(src, Host::Abnt2, &env).unwrap();
    assert_eq!(m.secret.to_vec(), vec![true, false]);
    assert_eq!(m.env_names.to_vec(), vec!["PIN"]);
    let steps: Vec<Step> = m.steps[..m.spans[0].1 as usize].iter().map(|v| Step::decode(*v).unwrap()).collect();
    assert_eq!(steps.len(), 5); // 9 9 9 9 ENT
    assert_eq!(steps[0], Step::Tap(Mods::NONE, Keycode::N9));

    // Erros não mostram o valor.
    let err = |src: &str| macros::parse_with_env(src, Host::Abnt2, &|_| Some("s3gr€do")).err().unwrap().to_string();
    let e = err("[a]\nENV(X)");
    assert!(e.contains("ENV(X)") && !e.contains("s3gr"), "{e}");
    let e = macros::parse_with_env("[a]\nENV(FALTA)", Host::Abnt2, &env).err().unwrap().to_string();
    assert!(e.contains("secrets.env"), "{e}");
    assert!(macros::parse("[a]\nENV(PIN)", Host::Abnt2).is_err());
    let e = macros::parse_with_env("[a]\nENV(a-b)", Host::Abnt2, &env).err().unwrap().to_string();
    assert!(e.contains("nome inválido"), "{e}");
}

mod persist {
    use corne_core::persist::{self, RECORD_LEN};
    use corne_core::state::Settings;

    const SLOTS: usize = 8;

    fn changed() -> Settings {
        Settings { rgb_effect: 7, rgb_val: 33, oled_scene_right: 5, oled_on: false, ..Settings::DEFAULT }
    }

    fn sector() -> Vec<[u8; RECORD_LEN]> {
        vec![[0xFF; RECORD_LEN]; SLOTS]
    }

    fn scan(sec: &[[u8; RECORD_LEN]]) -> persist::Scan {
        persist::scan(SLOTS, &Settings::DEFAULT, |i| sec[i])
    }

    #[test]
    fn blank_sector_has_nothing_saved() {
        let sec = sector();
        assert_eq!(scan(&sec), persist::Scan { saved: None, next_slot: Some(0) });
    }

    #[test]
    fn last_record_wins() {
        let mut sec = sector();
        sec[0] = persist::encode(&Settings::DEFAULT, &Settings::DEFAULT);
        sec[1] = persist::encode(&changed(), &Settings::DEFAULT);
        assert_eq!(scan(&sec), persist::Scan { saved: Some(changed()), next_slot: Some(2) });
    }

    #[test]
    fn torn_write_is_ignored() {
        let mut sec = sector();
        sec[0] = persist::encode(&changed(), &Settings::DEFAULT);
        // Faltou energia no meio do registro 1: só metade gravada.
        let r = persist::encode(&Settings::DEFAULT, &Settings::DEFAULT);
        sec[1][..RECORD_LEN / 2].copy_from_slice(&r[..RECORD_LEN / 2]);
        assert_eq!(scan(&sec), persist::Scan { saved: Some(changed()), next_slot: Some(2) });
    }

    #[test]
    fn full_sector_asks_for_erase() {
        let sec = vec![persist::encode(&changed(), &Settings::DEFAULT); SLOTS];
        assert_eq!(scan(&sec), persist::Scan { saved: Some(changed()), next_slot: None });
    }

    #[test]
    fn new_firmware_defaults_discard_saved() {
        let mut sec = sector();
        let old_defaults = Settings { rgb_effect: 1, ..Settings::DEFAULT };
        sec[0] = persist::encode(&changed(), &old_defaults);
        assert_eq!(scan(&sec).saved, None);
    }

    #[test]
    fn round_trip_every_field() {
        let s = Settings {
            rgb_on: false,
            rgb_effect: 1,
            rgb_theme: 2,
            rgb_hue: 3,
            rgb_sat: 4,
            rgb_val: 5,
            rgb_speed: 6,
            oled_on: false,
            oled_scene: 8,
            oled_scene_right: 9,
        };
        let r = persist::encode(&s, &Settings::DEFAULT);
        assert_eq!(persist::decode(&r), Some((persist::defaults_id(&Settings::DEFAULT), s)));
    }
}
