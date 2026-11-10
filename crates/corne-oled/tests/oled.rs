use corne_core::layout::{KeyPos, Side};
use corne_core::state::KeyboardState;
use corne_oled::{FrameBuffer, IDLE_OFF_MS, OledEngine, OledFrame, Orientation, Scene};

const SCENES: &[Scene] =
    &[Scene::Status, Scene::WpmGraph, Scene::Starfield, Scene::Rain, Scene::Life, Scene::Plasma, Scene::Keys];

#[test]
fn every_scene_renders_in_every_orientation() {
    let names = ["base", "nav", "sym", "adjust-long-name"];
    for side in [Side::Left, Side::Right] {
        for o in
            [Orientation::Portrait, Orientation::Landscape, Orientation::PortraitFlipped, Orientation::LandscapeFlipped]
        {
            for (i, _) in SCENES.iter().enumerate() {
                let mut eng = OledEngine::new(side, SCENES);
                let mut fb = FrameBuffer::new(o);
                let mut st = KeyboardState::default();
                st.settings.oled_scene = i as u8;
                for frame in 0..400u32 {
                    let now = frame * 33 + 4_294_000_000; // atravessa o estouro do u32
                    if frame % 5 == 0 {
                        eng.key_event(KeyPos::from_index((frame as usize) % 42), true, now);
                    }
                    st.wpm = (frame % 256) as u8;
                    st.mods = (frame % 16) as u8;
                    st.caps_lock = frame % 50 > 25;
                    st.layers.active = 1 << (frame % 4);
                    let f =
                        OledFrame { state: &st, now, layer_names: &names, rgb_effect: Some("Aurora"), rgb_theme: None };
                    assert!(eng.render(&f, &mut fb));
                }
                let lit = fb.raw().iter().map(|b| b.count_ones()).sum::<u32>();
                // cenas animadas devem desenhar algo
                assert!(lit > 0, "cena {:?} {o:?} vazia", SCENES[i]);
            }
        }
    }
}

#[test]
fn screen_turns_off_when_idle() {
    let mut eng = OledEngine::new(Side::Left, SCENES);
    let mut fb = FrameBuffer::new(Orientation::Portrait);
    let st = KeyboardState::default();
    let f = |now| OledFrame { state: &st, now, layer_names: &[], rgb_effect: None, rgb_theme: None };
    assert!(eng.render(&f(0), &mut fb));
    assert!(!eng.render(&f(IDLE_OFF_MS + 10), &mut fb));
    eng.key_event(KeyPos::new(0, 0), true, IDLE_OFF_MS + 20);
    assert!(eng.render(&f(IDLE_OFF_MS + 30), &mut fb));
}

#[test]
fn rotation_maps_corners() {
    let mut fb = FrameBuffer::new(Orientation::Portrait);
    assert_eq!(fb.dims(), (32, 128));
    fb.set(0, 0, true);
    assert!(fb.get_physical(127, 0));
    let mut fb = FrameBuffer::new(Orientation::LandscapeFlipped);
    fb.set(0, 0, true);
    assert!(fb.get_physical(127, 31));
}

#[test]
fn theme_popup_shows_for_two_seconds_only_where_enabled() {
    use corne_oled::{POPUP_MS, ThemeInfo};
    let names: [&str; 1] = ["base"];
    let theme = |index| Some(ThemeInfo { name: if index == 0 { "Synthwave" } else { "Ocean" }, index, count: 5 });
    for orientation in [Orientation::Portrait, Orientation::Landscape] {
        // Mesmo quadro com e sem aviso: compara o framebuffer.
        let draw = |popups: bool, frames: &[(u32, u8)]| {
            let mut e = OledEngine::new(Side::Right, &[Scene::Status]).with_popups(popups);
            let mut fb = FrameBuffer::new(orientation);
            let st = KeyboardState::default();
            for &(now, t) in frames {
                let f = OledFrame { state: &st, now, layer_names: &names, rgb_effect: None, rgb_theme: theme(t) };
                e.render(&f, &mut fb);
            }
            fb.raw().to_vec()
        };
        let plain = draw(false, &[(0, 0), (100, 1)]);
        // Ao ligar não há aviso.
        assert_eq!(draw(true, &[(0, 0)]), draw(false, &[(0, 0)]));
        // Trocou o tema: aviso aparece...
        assert_ne!(draw(true, &[(0, 0), (100, 1)]), plain);
        // ...e some depois de POPUP_MS.
        let after = 100 + POPUP_MS;
        assert_eq!(draw(true, &[(0, 0), (100, 1), (after, 1)]), draw(false, &[(0, 0), (100, 1), (after, 1)]));
    }
}
