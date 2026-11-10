//! Script de build:
//! 1. Lê `../keymap.kbd` e gera o keymap padrão em Rust (erros aparecem aqui,
//!    com número da linha, antes de qualquer coisa ir para o teclado).
//! 2. Lê `../macros.kbd` e `../combos.kbd` (opcionais) e gera as macros e os combos.
//! 3. Disponibiliza o `memory.x` para o linker.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

    // --- memory.x ---
    fs::copy(manifest.join("memory.x"), out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    // --- lado ---
    let left = env::var_os("CARGO_FEATURE_LEFT").is_some();
    let right = env::var_os("CARGO_FEATURE_RIGHT").is_some();
    if left == right {
        panic!(
            "\n\nEscolha o lado da metade: --features left  ou  --features right\n\
             (ou use scripts/build.sh, que gera os dois .uf2)\n\n"
        );
    }

    // --- keymap ---
    let path = manifest.join("../keymap.kbd");
    println!("cargo:rerun-if-changed={}", path.display());
    let src = fs::read_to_string(&path).unwrap_or_else(|e| panic!("não consegui ler {}: {e}", path.display()));
    // As macros vêm antes: o keymap precisa dos nomes delas para M(nome).
    let host = corne_core::keymap_file::read_host(&src).unwrap_or_else(|e| panic!("\n\nErro em keymap.kbd, {e}\n\n"));
    let macros_path = manifest.join("../macros.kbd");
    println!("cargo:rerun-if-changed={}", macros_path.display());
    let macros_src = fs::read_to_string(&macros_path).unwrap_or_default();
    // Segredos de ENV(NOME): primeiro o ../secrets.env (fora do git), depois o ambiente.
    let secrets_path = manifest.join("../secrets.env");
    println!("cargo:rerun-if-changed={}", secrets_path.display());
    let secrets = fs::read_to_string(&secrets_path).unwrap_or_default();
    let lookup = |name: &str| -> Option<&'static str> {
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        corne_core::macros::env_value(&secrets, name)
            .map(|v| leak(v.to_string()))
            .or_else(|| env::var(name).ok().map(leak))
    };
    let macros = match corne_core::macros::parse_with_env(&macros_src, host, &lookup) {
        Ok(m) => m,
        Err(e) => panic!("\n\nErro em macros.kbd, {e}\n\n"),
    };
    for name in &macros.env_names {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let parsed = match corne_core::keymap_file::parse_with_macros(&src, &macros.names) {
        Ok(p) => p,
        Err(e) => panic!("\n\nErro em keymap.kbd, {e}\n\n"),
    };

    let mut code = String::new();
    writeln!(code, "// Gerado por build.rs a partir de keymap.kbd. Não edite.").unwrap();
    writeln!(code, "#[allow(dead_code)]").unwrap();
    writeln!(code, "pub const LAYER_COUNT: usize = {};", parsed.count).unwrap();
    writeln!(code, "#[allow(dead_code)]").unwrap();
    write!(code, "pub static LAYER_NAMES: [&str; {}] = [", parsed.count).unwrap();
    for n in &parsed.names[..parsed.count] {
        write!(code, "{n:?}, ").unwrap();
    }
    writeln!(code, "];").unwrap();
    writeln!(code, "pub static DEFAULT_LAYERS: [corne_core::keymap::EncodedLayer; {}] = [", parsed.count).unwrap();
    for layer in &parsed.layers[..parsed.count] {
        writeln!(code, "    [").unwrap();
        for row in layer {
            write!(code, "        [").unwrap();
            for v in row {
                write!(code, "0x{v:04X}, ").unwrap();
            }
            writeln!(code, "],").unwrap();
        }
        writeln!(code, "    ],").unwrap();
    }
    writeln!(code, "];").unwrap();

    // --- combos (arquivo opcional) ---
    let combos_path = manifest.join("../combos.kbd");
    println!("cargo:rerun-if-changed={}", combos_path.display());
    let combos_src = fs::read_to_string(&combos_path).unwrap_or_default();
    let combos = match corne_core::combo::parse(&combos_src, &parsed) {
        Ok(c) => c,
        Err(e) => panic!("\n\nErro em combos.kbd, {e}\n\n"),
    };
    // --- macros ---
    write!(code, "pub static MACRO_STEPS: [u16; {}] = [", macros.steps.len()).unwrap();
    for v in &macros.steps {
        write!(code, "0x{v:04X}, ").unwrap();
    }
    writeln!(code, "];").unwrap();
    write!(code, "pub static MACRO_SPANS: [(u16, u16); {}] = [", macros.spans.len()).unwrap();
    for (s, l) in &macros.spans {
        write!(code, "({s}, {l}), ").unwrap();
    }
    writeln!(code, "];").unwrap();
    writeln!(
        code,
        "pub const MACROS: corne_core::macros::MacroTable = corne_core::macros::MacroTable {{ \
         steps: &MACRO_STEPS, spans: &MACRO_SPANS, delay_ms: {} }};",
        macros.delay_ms
    )
    .unwrap();

    writeln!(code, "pub const COMBO_TERM_MS: u16 = {};", combos.term_ms).unwrap();
    writeln!(code, "pub const COMBO_IDLE_MS: u16 = {};", combos.idle_ms).unwrap();
    writeln!(code, "pub static COMBOS: [corne_core::combo::Combo; {}] = [", combos.combos.len()).unwrap();
    for c in &combos.combos {
        writeln!(
            code,
            "    corne_core::combo::Combo {{ keys: 0x{:X}, action: 0x{:04X}, layer: {} }},",
            c.keys, c.action, c.layer
        )
        .unwrap();
    }
    writeln!(code, "];").unwrap();

    fs::write(out.join("keymap_gen.rs"), code).unwrap();
}
