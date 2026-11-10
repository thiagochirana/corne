use corne_core::layout::{KeyPos, Side};
use corne_core::state::KeyboardState;
use corne_rgb::color::Rgb;
use corne_rgb::effects::EFFECTS;
use corne_rgb::leds::{LEDS, build};
use corne_rgb::{Frame, POWER_LIMIT, RgbEngine, apply_cmd, theme};

#[test]
fn every_effect_theme_and_side_renders_within_power_budget() {
    for side in [Side::Left, Side::Right] {
        for t in 0..theme::THEMES.len() as u8 {
            for e in 0..EFFECTS.len() as u8 {
                let mut eng = RgbEngine::new(side);
                let mut st = KeyboardState::default();
                st.settings.rgb_effect = e;
                st.settings.rgb_theme = t;
                st.settings.rgb_val = 255;
                st.settings.rgb_speed = 255;
                let mut out = [Rgb::default(); LEDS];
                // ~20 s de animação, com teclas, camada, mods e caps no meio
                for frame in 0..1200u32 {
                    let now = frame * 16 + 4_000_000_000; // perto do estouro do u32
                    if frame % 7 == 0 {
                        let k = KeyPos::from_index((frame as usize * 5) % 42);
                        eng.key_event(k, true, now);
                        eng.key_event(k, false, now + 30);
                    }
                    st.layers.active = if frame % 300 > 150 { 1 << 1 } else { 0 };
                    st.mods = if frame % 100 > 80 { 0x02 } else { 0 };
                    st.caps_lock = frame % 500 > 450;
                    eng.render(&Frame { state: &st, now, layer_keys: 0x0F0F_0F0F }, &mut out);
                    let total: u32 = out.iter().map(|c| c.r as u32 + c.g as u32 + c.b as u32).sum();
                    assert!(total <= LEDS as u32 * 3 * POWER_LIMIT as u32 + LEDS as u32 * 3, "efeito {e} tema {t}");
                }
            }
        }
    }
}

#[test]
fn rgb_off_is_black() {
    let mut eng = RgbEngine::new(Side::Left);
    let mut st = KeyboardState::default();
    st.settings.rgb_on = false;
    let mut out = [Rgb::new(1, 1, 1); LEDS];
    eng.render(&Frame { state: &st, now: 0, layer_keys: 0 }, &mut out);
    assert!(out.iter().all(|c| *c == Rgb::default()));
}

#[test]
fn led_map_covers_each_key_of_the_half_once() {
    for side in [Side::Left, Side::Right] {
        let leds = build(side);
        let mut keys: Vec<usize> = leds.iter().filter_map(|l| l.key).map(|k| k.index()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 21, "{side:?}");
        assert!(leds.iter().filter_map(|l| l.key).all(|k| k.side() == side));
    }
}

#[test]
fn commands_wrap_around() {
    let mut s = KeyboardState::default().settings;
    s.rgb_effect = 0;
    apply_cmd(&mut s, corne_core::action::RgbCmd::PrevEffect);
    assert_eq!(s.rgb_effect as usize, EFFECTS.len() - 1);
    apply_cmd(&mut s, corne_core::action::RgbCmd::NextEffect);
    assert_eq!(s.rgb_effect, 0);
    s.rgb_val = 250;
    apply_cmd(&mut s, corne_core::action::RgbCmd::ValUp);
    assert_eq!(s.rgb_val, 255);
}
