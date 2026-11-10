//! Configurações pessoais. O layout das teclas fica em `../keymap.kbd`; aqui
//! ficam as escolhas de cada lado (cenas do OLED, rotação) e do motor.

use corne_core::debounce::DebounceMode;
use corne_core::engine::{EngineConfig, TapHoldMode};
use corne_core::layout::Side;
use corne_core::state::Settings;

#[cfg(feature = "left")]
pub const SIDE: Side = Side::Left;
#[cfg(feature = "right")]
pub const SIDE: Side = Side::Right;

/// Tap-hold (`LT`, `MT`): como decidir entre toque e segurar.
pub const ENGINE: EngineConfig = EngineConfig {
    // Segurar por mais que isso (sem outra tecla) = camada/modificador.
    tapping_term_ms: 200,
    // PermissiveHold: vira "segurar" só se outra tecla for apertada E solta
    // enquanto esta está segura. Teclas sobrepostas ao digitar rápido
    // (SPC↓ E↓ SPC↑ E↑) continuam sendo toque.
    mod_tap_mode: TapHoldMode::PermissiveHold,
    layer_tap_mode: TapHoldMode::PermissiveHold,
    // Flow tap: até 150 ms depois de uma letra/número/pontuação, o LT é toque
    // na hora (o espaço depois de uma palavra nunca vira camada e sai sem
    // atraso). Para usar a camada logo depois de digitar, espere um instante.
    // 0 desliga.
    flow_tap_ms: 150,
    // Segurou o LT e soltou sem apertar nada: envia o toque (espaço não some).
    retro_tap: true,
};

/// Debounce da matriz (ms).
pub const DEBOUNCE_MS: u32 = 5;

/// `Eager`: a tecla vale na hora e o repique é ignorado por `DEBOUNCE_MS`
/// (menor latência). `Defer`: espera `DEBOUNCE_MS` estável antes de valer; use
/// se aparecerem toques fantasmas por ruído.
pub const DEBOUNCE_MODE: DebounceMode = DebounceMode::Eager;

/// Intervalo de varredura da matriz (µs).
pub const SCAN_US: u64 = 250;

/// Velocidade do link entre as metades. Se houver erros com cabos longos,
/// reduza para 230_400.
pub const SPLIT_BAUD: u32 = 460_800;

/// Botão de reset da PCB: dois toques dentro deste intervalo (ms) reiniciam no
/// modo de gravação (aparece a unidade RPI-RP2). Um toque só apenas reinicia.
pub const DOUBLE_TAP_MS: u64 = 500;

/// Valores iniciais do RGB e do OLED (os comandos RGB_* e OLED_* mudam em uso).
pub const SETTINGS: Settings = Settings {
    rgb_on: true,
    rgb_effect: 4, // Aurora (veja corne_rgb::effects::EFFECTS)
    rgb_theme: 0,  // Synthwave (veja corne_rgb::theme::THEMES)
    rgb_hue: 0,
    rgb_sat: 255,
    rgb_val: 140,
    rgb_speed: 128,
    oled_on: true,
    oled_scene: 0,
};

#[cfg(feature = "oled")]
pub mod oled {
    use corne_oled::{Orientation, Scene};

    /// Cenas de cada lado, na ordem de OLED_NEXT. Os dois lados avançam juntos.
    #[cfg(feature = "left")]
    pub static SCENES: &[Scene] = &[Scene::Status, Scene::WpmGraph, Scene::Keys];
    #[cfg(feature = "right")]
    pub static SCENES: &[Scene] = &[Scene::Starfield, Scene::Rain, Scene::Life, Scene::Plasma];

    /// Avisos na tela (ex.: nome do tema de RGB por 2 s ao trocar com RGB_THEME).
    /// Por padrão só no lado direito, que mostra menos informação.
    #[cfg(feature = "left")]
    pub const POPUPS: bool = false;
    #[cfg(feature = "right")]
    pub const POPUPS: bool = true;

    /// Rotação da tela. Se aparecer de cabeça para baixo, use a versão `Flipped`.
    #[cfg(feature = "left")]
    pub const ORIENTATION: Orientation = Orientation::Portrait;
    #[cfg(feature = "right")]
    pub const ORIENTATION: Orientation = Orientation::Portrait;
}

/// Ganchos para ações `USER(n)` do keymap (macros etc.).
pub fn on_user_action(_n: u8, _pressed: bool) {}
