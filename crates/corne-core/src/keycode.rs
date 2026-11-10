//! Keycodes HID (página "Keyboard/Keypad") e bits de modificadores.
//!
//! Cada keycode tem um nome curto usado no arquivo de layout (`keymap.kbd`),
//! no estilo do QMK, para facilitar a migração (`A`, `SPC`, `LCTL`, `F1`...).

/// Bits de modificadores, no mesmo formato do byte de modificadores do relatório HID.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Mods(pub u8);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const LCTRL: Mods = Mods(0x01);
    pub const LSHIFT: Mods = Mods(0x02);
    pub const LALT: Mods = Mods(0x04);
    pub const LGUI: Mods = Mods(0x08);
    pub const RCTRL: Mods = Mods(0x10);
    pub const RSHIFT: Mods = Mods(0x20);
    pub const RALT: Mods = Mods(0x40);
    pub const RGUI: Mods = Mods(0x80);

    pub const fn union(self, other: Mods) -> Mods {
        Mods(self.0 | other.0)
    }
    pub const fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub const fn ctrl(self) -> bool {
        self.0 & 0x11 != 0
    }
    pub const fn shift(self) -> bool {
        self.0 & 0x22 != 0
    }
    pub const fn alt(self) -> bool {
        self.0 & 0x44 != 0
    }
    pub const fn gui(self) -> bool {
        self.0 & 0x88 != 0
    }

    /// Nome de um único modificador (para parse e exibição).
    pub fn from_name(name: &str) -> Option<Mods> {
        Some(match name {
            "CTRL" | "LEFT_CTRL" | "LCTL" | "LCTRL" | "CTL" => Mods::LCTRL,
            "SHIFT" | "LEFT_SHIFT" | "LSFT" | "LSHIFT" | "SFT" => Mods::LSHIFT,
            "ALT" | "LEFT_ALT" | "LALT" | "LOPT" => Mods::LALT,
            "WIN" | "LEFT_WIN" | "GUI" | "CMD" | "SUPER" | "LGUI" | "LCMD" | "LWIN" => Mods::LGUI,
            "RIGHT_CTRL" | "RCTL" | "RCTRL" => Mods::RCTRL,
            "RIGHT_SHIFT" | "RSFT" | "RSHIFT" => Mods::RSHIFT,
            "ALTGR" | "RIGHT_ALT" | "RALT" | "ROPT" | "ALGR" => Mods::RALT,
            "RIGHT_WIN" | "RGUI" | "RCMD" | "RWIN" => Mods::RGUI,
            _ => return None,
        })
    }
}

macro_rules! keycodes {
    ($( $variant:ident = $code:literal => [$($name:literal),+] ),* $(,)?) => {
        /// Keycode HID (usage id da página 0x07).
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #[repr(u8)]
        pub enum Keycode {
            $( $variant = $code, )*
        }

        impl Keycode {
            /// Converte um usage id HID em `Keycode`, se for conhecido.
            pub fn from_u8(v: u8) -> Option<Keycode> {
                match v {
                    $( $code => Some(Keycode::$variant), )*
                    _ => None,
                }
            }

            /// Procura um keycode pelo nome (aceita aliases). Maiúsculas e
            /// minúsculas tanto faz: `a`, `esc` e `Spc` valem como `A`, `ESC`, `SPC`.
            pub fn from_name(name: &str) -> Option<Keycode> {
                let exact = |n: &str| match n {
                    $( $($name)|+ => Some(Keycode::$variant), )*
                    _ => None,
                };
                if let Some(k) = exact(name) {
                    return Some(k);
                }
                // Tenta em maiúsculas (nomes curtos, só ASCII).
                let mut buf = [0u8; 16];
                if name.len() > buf.len() || !name.is_ascii() {
                    return None;
                }
                buf[..name.len()].copy_from_slice(name.as_bytes());
                buf[..name.len()].make_ascii_uppercase();
                exact(core::str::from_utf8(&buf[..name.len()]).ok()?)
            }

            /// Nome canônico (primeiro alias).
            pub fn name(self) -> &'static str {
                match self {
                    $( Keycode::$variant => keycodes!(@first $($name),+), )*
                }
            }
        }
    };
    (@first $first:literal $(, $rest:literal)*) => { $first };
}

keycodes! {
    A = 0x04 => ["A"], B = 0x05 => ["B"], C = 0x06 => ["C"], D = 0x07 => ["D"],
    E = 0x08 => ["E"], F = 0x09 => ["F"], G = 0x0A => ["G"], H = 0x0B => ["H"],
    I = 0x0C => ["I"], J = 0x0D => ["J"], K = 0x0E => ["K"], L = 0x0F => ["L"],
    M = 0x10 => ["M"], N = 0x11 => ["N"], O = 0x12 => ["O"], P = 0x13 => ["P"],
    Q = 0x14 => ["Q"], R = 0x15 => ["R"], S = 0x16 => ["S"], T = 0x17 => ["T"],
    U = 0x18 => ["U"], V = 0x19 => ["V"], W = 0x1A => ["W"], X = 0x1B => ["X"],
    Y = 0x1C => ["Y"], Z = 0x1D => ["Z"],
    N1 = 0x1E => ["1"], N2 = 0x1F => ["2"], N3 = 0x20 => ["3"], N4 = 0x21 => ["4"],
    N5 = 0x22 => ["5"], N6 = 0x23 => ["6"], N7 = 0x24 => ["7"], N8 = 0x25 => ["8"],
    N9 = 0x26 => ["9"], N0 = 0x27 => ["0"],
    Enter = 0x28 => ["ENT", "ENTER", "RET"],
    Escape = 0x29 => ["ESC", "ESCAPE"],
    Backspace = 0x2A => ["BSPC", "BACKSPACE"],
    Tab = 0x2B => ["TAB"],
    Space = 0x2C => ["SPC", "SPACE"],
    Minus = 0x2D => ["-", "MINS", "MINUS"],
    Equal = 0x2E => ["=", "EQL", "EQUAL"],
    LeftBracket = 0x2F => ["[", "LBRC"],
    RightBracket = 0x30 => ["]", "RBRC"],
    Backslash = 0x31 => ["\\", "BSLS"],
    NonUsHash = 0x32 => ["NUHS"],
    Semicolon = 0x33 => [";", "SCLN"],
    Quote = 0x34 => ["'", "QUOT"],
    Grave = 0x35 => ["`", "GRV"],
    Comma = 0x36 => [",", "COMM"],
    Dot = 0x37 => [".", "DOT"],
    Slash = 0x38 => ["/", "SLSH"],
    CapsLock = 0x39 => ["CAPS"],
    F1 = 0x3A => ["F1"], F2 = 0x3B => ["F2"], F3 = 0x3C => ["F3"], F4 = 0x3D => ["F4"],
    F5 = 0x3E => ["F5"], F6 = 0x3F => ["F6"], F7 = 0x40 => ["F7"], F8 = 0x41 => ["F8"],
    F9 = 0x42 => ["F9"], F10 = 0x43 => ["F10"], F11 = 0x44 => ["F11"], F12 = 0x45 => ["F12"],
    PrintScreen = 0x46 => ["PSCR"],
    ScrollLock = 0x47 => ["SCRL"],
    Pause = 0x48 => ["PAUS"],
    Insert = 0x49 => ["INS"],
    Home = 0x4A => ["HOME"],
    PageUp = 0x4B => ["PGUP"],
    Delete = 0x4C => ["DEL"],
    End = 0x4D => ["END"],
    PageDown = 0x4E => ["PGDN"],
    Right = 0x4F => ["RGHT", "RIGHT"],
    Left = 0x50 => ["LEFT"],
    Down = 0x51 => ["DOWN"],
    Up = 0x52 => ["UP"],
    NumLock = 0x53 => ["NUM"],
    NonUsBackslash = 0x64 => ["NUBS"],
    Application = 0x65 => ["APP", "MENU"],
    // Teclas de mídia simples na página de teclado (funcionam na maioria dos sistemas).
    Mute = 0x7F => ["MUTE"],
    VolumeUp = 0x80 => ["VOLU"],
    VolumeDown = 0x81 => ["VOLD"],
    International1 = 0x87 => ["INT1", "RO"],
    // Modificadores: o primeiro nome é o principal; os outros (estilo QMK) também valem.
    LCtrl = 0xE0 => ["CTRL", "LEFT_CTRL", "LCTL", "LCTRL"],
    LShift = 0xE1 => ["SHIFT", "LEFT_SHIFT", "LSFT", "LSHIFT"],
    LAlt = 0xE2 => ["ALT", "LEFT_ALT", "LALT", "LOPT"],
    LGui = 0xE3 => ["WIN", "LEFT_WIN", "GUI", "CMD", "SUPER", "LGUI", "LCMD", "LWIN"],
    RCtrl = 0xE4 => ["RIGHT_CTRL", "RCTL", "RCTRL"],
    RShift = 0xE5 => ["RIGHT_SHIFT", "RSFT", "RSHIFT"],
    RAlt = 0xE6 => ["ALTGR", "RIGHT_ALT", "RALT", "ALGR", "ROPT"],
    RGui = 0xE7 => ["RIGHT_WIN", "RGUI", "RCMD", "RWIN"],
}

impl Keycode {
    /// Se for uma tecla modificadora, devolve o bit correspondente.
    pub fn as_modifier(self) -> Option<Mods> {
        let v = self as u8;
        if (0xE0..=0xE7).contains(&v) { Some(Mods(1 << (v - 0xE0))) } else { None }
    }

    /// Tecla de digitação: letras, números e pontuação (inclui ç e acentos do
    /// ABNT2). Espaço, Enter, Tab, Backspace, setas e modificadores não contam.
    /// Usado para saber se a pessoa está no meio de uma palavra.
    pub fn is_typing(self) -> bool {
        let v = self as u8;
        matches!(v, 0x04..=0x27 | 0x2D..=0x38 | 0x64 | 0x87)
    }
}
