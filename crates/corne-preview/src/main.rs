//! Pré-visualização no terminal dos efeitos de RGB e das cenas de OLED.
//!
//! Usa o mesmo código que roda no teclado (corne-rgb e corne-oled), então o
//! que você vê aqui é o que vai aparecer no Corne. Precisa de um terminal com
//! cores de 24 bits (Windows Terminal, o terminal do VS Code, etc.).
//!
//! ```text
//! cargo run -p corne-preview --release -- [opções]
//!
//!   --effect N|nome   efeito de RGB (padrão: aurora)
//!   --theme N         tema de cor (padrão: 0)
//!   --theme-cycle     troca de tema a cada 3 s (mostra o aviso no OLED direito)
//!   --scene-cycle     troca só a cena do OLED direito a cada 4 s (OLED_NEXT_RIGHT)
//!   --scene-left  nome   cena do OLED esquerdo  (padrão: status)
//!   --scene-right nome   cena do OLED direito   (padrão: stars)
//!   --landscape       OLED em paisagem (padrão: retrato, como no Corne)
//!   --layer N         segura a camada N
//!   --caps            liga o Caps Lock
//!   --no-typing       não simula digitação
//!   --frames N        desenha N quadros e sai (padrão: infinito, Ctrl+C sai)
//!   --list            lista efeitos, temas e cenas
//!   --keymap          valida o keymap.kbd e desenha as camadas
//! ```

use std::io::Write;
use std::time::{Duration, Instant};

use corne_core::action::Action;
use corne_core::combo;
use corne_core::host::Host;
use corne_core::keymap::Keymap;
use corne_core::keymap_file;
use corne_core::layout::{KeyPos, Side, all_keys};
use corne_core::macros;
use corne_core::state::KeyboardState;
use corne_core::wpm::Wpm;
use corne_oled::{FrameBuffer, OledEngine, OledFrame, Orientation, Scene, ThemeInfo};
use corne_rgb::color::Rgb;
use corne_rgb::effects::EFFECTS;
use corne_rgb::leds::LEDS;
use corne_rgb::{Frame, RgbEngine};

const KEYMAP: &str = include_str!("../../../keymap.kbd");
const COMBOS: &str = include_str!("../../../combos.kbd");
const MACROS: &str = include_str!("../../../macros.kbd");

const SCENES: [Scene; 15] = Scene::ALL;

fn scene_by_name(s: &str) -> Option<Scene> {
    Scene::from_name(s)
}

/// Cenas animadas (todas menos Status, WPM e Keys): as do lado direito.
const ANIMATED: &[Scene] = Scene::ALL.split_at(3).1;

struct Opts {
    effect: u8,
    theme: u8,
    left: Scene,
    right: Scene,
    orientation: Orientation,
    layer: Option<u8>,
    caps: bool,
    typing: bool,
    frames: Option<u32>,
    show_keymap: bool,
    theme_cycle: bool,
    scene_cycle: bool,
}

fn parse_args() -> Opts {
    let mut o = Opts {
        effect: 4,
        theme: 0,
        left: Scene::Status,
        right: Scene::Starfield,
        orientation: Orientation::Portrait,
        layer: None,
        caps: false,
        typing: true,
        frames: None,
        show_keymap: false,
        theme_cycle: false,
        scene_cycle: false,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let val = |i: &mut usize| -> String {
        *i += 1;
        args.get(*i).cloned().unwrap_or_else(|| {
            eprintln!("faltou o valor de {}", args[*i - 1]);
            std::process::exit(2)
        })
    };
    while i < args.len() {
        match args[i].as_str() {
            "--effect" => {
                let v = val(&mut i);
                o.effect = v
                    .parse()
                    .ok()
                    .or_else(|| EFFECTS.iter().position(|e| e.name().eq_ignore_ascii_case(&v)).map(|p| p as u8))
                    .unwrap_or_else(|| {
                        eprintln!("efeito desconhecido: {v} (use --list)");
                        std::process::exit(2)
                    });
            }
            "--theme" => o.theme = val(&mut i).parse().unwrap_or(0),
            "--theme-cycle" => o.theme_cycle = true,
            "--scene-cycle" => o.scene_cycle = true,
            "--scene-left" => o.left = scene_by_name(&val(&mut i)).unwrap_or(Scene::Status),
            "--scene-right" => o.right = scene_by_name(&val(&mut i)).unwrap_or(Scene::Starfield),
            "--landscape" => o.orientation = Orientation::Landscape,
            "--layer" => o.layer = val(&mut i).parse().ok(),
            "--caps" => o.caps = true,
            "--no-typing" => o.typing = false,
            "--frames" => o.frames = val(&mut i).parse().ok(),
            "--keymap" => o.show_keymap = true,
            "--list" => {
                println!("Efeitos:");
                for (n, e) in EFFECTS.iter().enumerate() {
                    println!("  {n:2}  {}", e.name());
                }
                println!("Temas:");
                for (n, t) in corne_rgb::theme::THEMES.iter().enumerate() {
                    println!("  {n:2}  {}", t.name);
                }
                println!("Cenas do OLED:");
                for s in SCENES {
                    println!("      {}", s.name());
                }
                std::process::exit(0);
            }
            "-h" | "--help" => {
                println!(
                    "{}",
                    include_str!("main.rs")
                        .lines()
                        .skip(7)
                        .take(16)
                        .map(|l| l.trim_start_matches("//! "))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("opção desconhecida: {other} (use --help)");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    o
}

fn bg(c: Rgb) -> String {
    format!("\x1b[48;2;{};{};{}m", c.r, c.g, c.b)
}

/// Desenha as duas metades: teclas como blocos coloridos, underglow embaixo.
fn draw_keyboard(out: &mut String, left: &[Rgb; LEDS], right: &[Rgb; LEDS], eng_l: &RgbEngine, eng_r: &RgbEngine) {
    // cor de cada tecla lógica
    let mut key_color = [Rgb::default(); 42];
    let mut under_l = vec![];
    let mut under_r = vec![];
    for (leds, colors, under) in [(eng_l.leds(), left, &mut under_l), (eng_r.leds(), right, &mut under_r)] {
        for (i, led) in leds.iter().enumerate() {
            match led.key {
                Some(k) => key_color[k.index()] = colors[i],
                None => under.push((led.x, colors[i])),
            }
        }
    }
    // Escala: os LEDs RGB reais parecem muito mais brilhantes; realça para a tela.
    let boost = |c: Rgb| Rgb::new(c.r.saturating_mul(2), c.g.saturating_mul(2), c.b.saturating_mul(2));
    for row in 0..4u8 {
        for col in 0..12u8 {
            if col == 6 {
                out.push_str("      ");
            }
            let pos = KeyPos::new(row, col);
            if !pos.exists() {
                out.push_str("      ");
                continue;
            }
            // escalonamento das colunas: desloca blocos meia linha com espaços
            let c = boost(key_color[pos.index()]);
            out.push_str(&format!("{} ▓▓▓ \x1b[0m ", bg(c)));
        }
        out.push('\n');
    }
    out.push('\n');
    for under in [&mut under_l, &mut under_r] {
        under.sort_by_key(|(x, _)| *x);
    }
    out.push_str("  underglow:  ");
    for (_, c) in under_l.iter() {
        out.push_str(&format!("{}    \x1b[0m ", bg(boost(*c))));
    }
    out.push_str("            ");
    for (_, c) in under_r.iter() {
        out.push_str(&format!("{}    \x1b[0m ", bg(boost(*c))));
    }
    out.push('\n');
}

/// Desenha os dois OLEDs lado a lado usando meio-blocos (2 pixels por caractere).
fn draw_oleds(out: &mut String, l: &FrameBuffer, r: &FrameBuffer) {
    let (w, h) = l.dims();
    let px = |fb: &FrameBuffer, x: i32, y: i32| fb.get(x, y);
    for y in (0..h).step_by(2) {
        for (n, fb) in [l, r].into_iter().enumerate() {
            if n == 1 {
                out.push_str("    ");
            }
            out.push_str("\x1b[38;2;140;220;255m\x1b[48;2;10;12;20m");
            for x in 0..w {
                let top = px(fb, x, y);
                let bot = px(fb, x, y + 1);
                out.push(match (top, bot) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    _ => ' ',
                });
            }
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
}

/// Texto da ação com camadas pelo nome, ex.: MO(nav), LT(sym,ENT).
fn with_layer_names(a: Action, host: Host, names: &[&str], macros: &[&str]) -> String {
    let name = |l: u8| names.get(l as usize).copied().unwrap_or("?");
    let s = a.label(host).to_string();
    match a {
        Action::Macro(n) => format!("MACRO({})", macros.get(n as usize).copied().unwrap_or("?")),
        Action::Momentary(l) | Action::Toggle(l) | Action::To(l) | Action::DefaultLayer(l) => {
            s.replace(&format!("({l})"), &format!("({})", name(l)))
        }
        Action::LayerTap(l, ..) => s.replacen(&format!("({l},"), &format!("({},", name(l)), 1),
        _ => s,
    }
}

/// Desenha cada camada como uma grade, com os nomes das ações.
fn print_keymap(km: &Keymap, names: &[&str], macros: &[&str], host: Host) {
    let label = |l: u8, pos: KeyPos| -> String {
        let s = with_layer_names(km.get(l, pos), host, names, macros);
        if s.chars().count() > 10 { s.chars().take(9).collect::<String>() + "…" } else { s }
    };
    for (l, name) in names.iter().enumerate() {
        println!("\x1b[1m[{name}]\x1b[0m  camada {l}, {} teclas ativas", km.layer_mask(l as u8).count_ones());
        for row in 0..4u8 {
            let mut line = String::new();
            for col in 0..12u8 {
                if col == 6 {
                    line.push_str("   ");
                }
                let pos = KeyPos::new(row, col);
                let cell = if pos.exists() { label(l as u8, pos) } else { String::new() };
                let shown = match cell.as_str() {
                    "___" => "\x1b[2m   ___    \x1b[0m".to_string(),
                    "XXX" => "\x1b[2m    ·     \x1b[0m".to_string(),
                    c => format!("{c:^10}"),
                };
                line.push_str(&shown);
                line.push(' ');
            }
            println!("{}", line.trim_end());
        }
        println!();
    }
    println!("keymap.kbd OK: {} camadas, host = {}", names.len(), host.name());
}

/// Lista as macros: tamanho e onde são usadas (teclas e combos).
fn print_macros(km: &Keymap, names: &[&str], m: &macros::ParsedMacros, combos: &combo::ParsedCombos) {
    println!();
    println!("\x1b[1mMacros\x1b[0m  delay de {} ms entre teclas", m.delay_ms);
    for (i, name) in m.names.iter().enumerate() {
        let mut used = vec![];
        for (l, lname) in names.iter().enumerate() {
            for pos in all_keys() {
                if km.get(l as u8, pos) == Action::Macro(i as u8) {
                    used.push(format!("[{lname}] K({},{})", pos.row, pos.col));
                }
            }
        }
        let n = combos.combos.iter().filter(|c| Action::decode(c.action) == Action::Macro(i as u8)).count();
        if n > 0 {
            used.push(format!("{n} combo(s)"));
        }
        let used = if used.is_empty() { "\x1b[2mnão usada\x1b[0m".to_string() } else { used.join(", ") };
        // Macros com ENV(...) não mostram o tamanho (daria pista do segredo).
        let size = if m.secret[i] { "  secreta  ".to_string() } else { format!("{:>4} passos", m.spans[i].1) };
        println!("  {:<14} {size}   {used}", name);
    }
    println!("macros.kbd OK: {} macros", m.names.len());
}

/// Lista os combos: teclas (como aparecem no keymap) => ação.
fn print_combos(km: &Keymap, names: &[&str], macros: &[&str], host: Host, parsed: &combo::ParsedCombos) {
    println!();
    println!("\x1b[1mCombos\x1b[0m  janela de {} ms, só após {} ms sem digitar", parsed.term_ms, parsed.idle_ms);
    for c in &parsed.combos {
        let layer = if c.layer == combo::ANY_LAYER { 0 } else { c.layer };
        let keys: Vec<String> =
            c.positions().map(|p| with_layer_names(km.get(layer, p), host, names, macros)).collect();
        let action = with_layer_names(Action::decode(c.action), host, names, macros);
        let scope = if c.layer == combo::ANY_LAYER {
            "todas as camadas".to_string()
        } else {
            format!("só em [{}]", names[layer as usize])
        };
        println!("  {:<24} => {:<12} \x1b[2m{scope}\x1b[0m", keys.join(" + "), action);
    }
    println!("combos.kbd OK: {} combos", parsed.combos.len());
}

fn main() {
    let o = parse_args();
    let fail = |file: &str, e: &dyn std::fmt::Display| -> ! {
        eprintln!("erro no {file}: {e}");
        std::process::exit(1)
    };
    let host = keymap_file::read_host(KEYMAP).unwrap_or_else(|e| fail("keymap.kbd", &e));
    // ENV(NOME) das macros: lido do secrets.env (fora do git) ou do ambiente, como no firmware.
    let secrets =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../secrets.env")).unwrap_or_default();
    let lookup = |name: &str| -> Option<&'static str> {
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        macros::env_value(&secrets, name).map(|v| leak(v.to_string())).or_else(|| std::env::var(name).ok().map(leak))
    };
    let macros = macros::parse_with_env(MACROS, host, &lookup).unwrap_or_else(|e| fail("macros.kbd", &e));
    let parsed = keymap_file::parse_with_macros(KEYMAP, &macros.names).unwrap_or_else(|e| fail("keymap.kbd", &e));
    let names: Vec<&str> = parsed.names[..parsed.count].to_vec();
    let keymap = Keymap::from_encoded(&parsed.layers[..parsed.count]);
    if o.show_keymap {
        print_keymap(&keymap, &names, &macros.names, parsed.host);
        let combos = combo::parse(COMBOS, &parsed).unwrap_or_else(|e| fail("combos.kbd", &e));
        print_combos(&keymap, &names, &macros.names, parsed.host, &combos);
        print_macros(&keymap, &names, &macros, &combos);
        return;
    }

    let mut state = KeyboardState::default();
    state.settings.rgb_effect = o.effect;
    state.settings.rgb_theme = o.theme;
    state.caps_lock = o.caps;
    if let Some(l) = o.layer {
        state.layers.on(l);
    }

    let mut rgb_l = RgbEngine::new(Side::Left);
    let mut rgb_r = RgbEngine::new(Side::Right);
    let left_scenes: &'static [Scene] = Box::leak(Box::new([o.left]));
    let right_scenes: &'static [Scene] = if o.scene_cycle { ANIMATED } else { Box::leak(Box::new([o.right])) };
    let mut oled_l = OledEngine::new(Side::Left, left_scenes);
    // Como no firmware: o lado direito mostra os avisos (ex.: troca de tema).
    let mut oled_r = OledEngine::new(Side::Right, right_scenes).with_popups(true);
    let mut fb_l = FrameBuffer::new(o.orientation);
    let mut fb_r = FrameBuffer::new(o.orientation);
    let mut wpm = Wpm::new();

    let keys: Vec<KeyPos> = all_keys().collect();
    let mut rng = 0x2545_F491u32;
    let mut rand = move || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng
    };
    let mut held: Option<(KeyPos, u32)> = None;
    let mut next_key = 0u32;

    let start = Instant::now();
    let mut frame = 0u32;
    print!("\x1b[2J\x1b[?25l");
    loop {
        let now = start.elapsed().as_millis() as u32;

        // Digitação simulada (~70–90 WPM), alternando as metades.
        if o.typing {
            if let Some((k, t)) = held
                && now >= t
            {
                let eng = if k.side() == Side::Left { &mut rgb_l } else { &mut rgb_r };
                eng.key_event(k, false, now);
                let ol = if k.side() == Side::Left { &mut oled_l } else { &mut oled_r };
                ol.key_event(k, false, now);
                held = None;
            }
            if held.is_none() && now >= next_key {
                let k = keys[(rand() % 36) as usize];
                let eng = if k.side() == Side::Left { &mut rgb_l } else { &mut rgb_r };
                eng.key_event(k, true, now);
                let ol = if k.side() == Side::Left { &mut oled_l } else { &mut oled_r };
                ol.key_event(k, true, now);
                wpm.key_pressed(now);
                held = Some((k, now + 60));
                // pausas ocasionais para o WPM variar
                next_key = now + 110 + (rand() % 90) + if rand() % 40 == 0 { 1500 } else { 0 };
            }
        }
        state.wpm = wpm.update(now);
        if o.theme_cycle {
            state.settings.rgb_theme = o.theme.wrapping_add((now / 3000) as u8);
        }
        if o.scene_cycle {
            // Só o lado direito troca; o esquerdo continua na mesma cena.
            state.settings.oled_scene_right = ((now / 4000) as usize % right_scenes.len()) as u8;
        }
        state.mods = if held.is_some_and(|(k, _)| k.col == 0) { 0x02 } else { 0 };

        let top = state.layers.highest();
        let f = Frame { state: &state, now, layer_keys: keymap.layer_mask(top) };
        let mut cl = [Rgb::default(); LEDS];
        let mut cr = [Rgb::default(); LEDS];
        rgb_l.render(&f, &mut cl);
        rgb_r.render(&f, &mut cr);

        let of = OledFrame {
            state: &state,
            now,
            layer_names: &names,
            rgb_effect: Some(corne_rgb::effect_name(&state.settings)),
            rgb_theme: {
                let (name, index, count) = corne_rgb::theme_info(&state.settings);
                Some(ThemeInfo { name, index, count })
            },
        };
        oled_l.render(&of, &mut fb_l);
        oled_r.render(&of, &mut fb_r);

        let mut out = String::from("\x1b[H");
        out.push_str(&format!(
            " Corne preview  |  RGB: {} / tema {}  |  OLED: {} + {}  |  WPM {:3}  |  camada: {}\x1b[K\n\n",
            corne_rgb::effect_name(&state.settings),
            corne_rgb::theme_name(&state.settings),
            o.left.name(),
            o.right.name(),
            state.wpm,
            names.get(top as usize).unwrap_or(&"?"),
        ));
        draw_keyboard(&mut out, &cl, &cr, &rgb_l, &rgb_r);
        out.push('\n');
        draw_oleds(&mut out, &fb_l, &fb_r);
        print!("{out}");
        std::io::stdout().flush().ok();

        frame += 1;
        if o.frames.is_some_and(|n| frame >= n) {
            break;
        }
        std::thread::sleep(Duration::from_millis(33));
    }
    print!("\x1b[?25h");
}
