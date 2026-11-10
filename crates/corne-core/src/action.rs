//! Ações de tecla: o que uma posição do keymap faz quando pressionada.
//!
//! Toda `Action` tem uma codificação estável em `u16`. É ela que vai para a flash
//! e para o protocolo de configuração (estilo Vial), então **não mude os valores
//! existentes**; só acrescente novos.
//!
//! | Faixa              | Ação                                      |
//! |--------------------|-------------------------------------------|
//! | `0x0000`           | `No` (tecla desativada)                   |
//! | `0x0001`           | `Trans` (usa a camada de baixo)           |
//! | `0x0004..=0x00FF`  | `Key(kc)`                                 |
//! | `0x0100..=0x1FFF`  | `ModKey(mods5, kc)`  ex.: `C(S(TAB))`      |
//! | `0x2000..=0x3FFF`  | `ModTap(mods5, kc)`  ex.: `MT(LSFT, A)`    |
//! | `0x4000..=0x4FFF`  | `LayerTap(layer, kc)` ex.: `LT(1, SPC)`    |
//! | `0x6000..=0x6FFF`  | `LayerTap` com Shift no toque, ex.: `LT(1, ?)` |
//! | `0x5000 \| n`       | `Momentary(n)`  `MO(n)`                    |
//! | `0x5100 \| n`       | `Toggle(n)`     `TG(n)`                    |
//! | `0x5200 \| n`       | `To(n)`         `TO(n)`                    |
//! | `0x5300 \| n`       | `DefaultLayer(n)` `DF(n)`                  |
//! | `0x7000`/`0x7001`  | `Bootloader` / `Reboot`                   |
//! | `0x7100 \| c`       | `Rgb(cmd)`                                |
//! | `0x7200 \| c`       | `Oled(cmd)`                               |
//! | `0x7300 \| n`       | `Macro(n)`  `MACRO(nome)` ou `M(nome)`    |
//! | `0x7F00 \| n`       | `User(n)` (macros/ações próprias)         |

use core::fmt;

use crate::host::Host;
use crate::keycode::{Keycode, Mods};

/// Comandos para o complemento de RGB. O core só os transporta; quem
/// interpreta é o `corne-rgb` (via `Settings`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum RgbCmd {
    Toggle = 0,
    NextEffect,
    PrevEffect,
    HueUp,
    HueDown,
    SatUp,
    SatDown,
    ValUp,
    ValDown,
    SpeedUp,
    SpeedDown,
    NextTheme,
}

impl RgbCmd {
    const ALL: [(RgbCmd, &'static str); 12] = [
        (RgbCmd::Toggle, "RGB_TOG"),
        (RgbCmd::NextEffect, "RGB_MOD"),
        (RgbCmd::PrevEffect, "RGB_RMOD"),
        (RgbCmd::HueUp, "RGB_HUI"),
        (RgbCmd::HueDown, "RGB_HUD"),
        (RgbCmd::SatUp, "RGB_SAI"),
        (RgbCmd::SatDown, "RGB_SAD"),
        (RgbCmd::ValUp, "RGB_VAI"),
        (RgbCmd::ValDown, "RGB_VAD"),
        (RgbCmd::SpeedUp, "RGB_SPI"),
        (RgbCmd::SpeedDown, "RGB_SPD"),
        (RgbCmd::NextTheme, "RGB_THEME"),
    ];
    fn from_u8(v: u8) -> Option<Self> {
        Self::ALL.get(v as usize).map(|(c, _)| *c)
    }
    fn name(self) -> &'static str {
        Self::ALL[self as usize].1
    }
    fn from_name(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|(_, n)| *n == s).map(|(c, _)| *c)
    }
}

/// Comandos para o complemento de OLED.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum OledCmd {
    Toggle = 0,
    NextScene,
    PrevScene,
}

impl OledCmd {
    const ALL: [(OledCmd, &'static str); 3] =
        [(OledCmd::Toggle, "OLED_TOG"), (OledCmd::NextScene, "OLED_NEXT"), (OledCmd::PrevScene, "OLED_PREV")];
    fn from_u8(v: u8) -> Option<Self> {
        Self::ALL.get(v as usize).map(|(c, _)| *c)
    }
    fn name(self) -> &'static str {
        Self::ALL[self as usize].1
    }
    fn from_name(s: &str) -> Option<Self> {
        Self::ALL.iter().find(|(_, n)| *n == s).map(|(c, _)| *c)
    }
}

/// O que uma tecla faz.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Não faz nada (bloqueia camadas de baixo).
    No,
    /// Transparente: procura a ação na próxima camada ativa abaixo.
    Trans,
    /// Tecla simples.
    Key(Keycode),
    /// Tecla com modificadores segurados junto (atalho).
    ModKey(Mods, Keycode),
    /// Toque = tecla, segurar = modificadores (home row mods).
    ModTap(Mods, Keycode),
    /// Toque = tecla (sozinha ou com Shift, ex.: `?`), segurar = ativa a camada.
    LayerTap(u8, Mods, Keycode),
    /// Camada ativa enquanto segura.
    Momentary(u8),
    /// Liga/desliga a camada.
    Toggle(u8),
    /// Desliga todas as camadas e ativa só esta (além da padrão).
    To(u8),
    /// Troca a camada padrão (base).
    DefaultLayer(u8),
    /// Reinicia no bootloader UF2 (aparece a unidade RPI-RP2).
    Bootloader,
    /// Reinicia o teclado.
    Reboot,
    Rgb(RgbCmd),
    Oled(OledCmd),
    /// Toca a macro `n` (definida em `macros.kbd`).
    Macro(u8),
    /// Ação definida pelo usuário, tratada em `firmware/src/user_config.rs`.
    User(u8),
}

/// Erro de parse de uma ação escrita no arquivo de layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseError {
    UnknownKey,
    UnknownLayer,
    BadSyntax,
    LayerOutOfRange,
    /// Símbolo com acento num keymap sem `host = abnt2` ou `host = us-intl`.
    NeedsHost,
    /// Modificadores dos dois lados juntos (ex.: LSFT + RALT) não cabem na codificação.
    MixedMods,
    UnknownMacro,
    /// No toque do LT só cabe tecla simples ou com Shift; no MT, só tecla simples.
    TapKeyWithMods,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParseError::UnknownKey => "tecla desconhecida",
            ParseError::UnknownLayer => "camada desconhecida",
            ParseError::BadSyntax => "sintaxe inválida",
            ParseError::LayerOutOfRange => "número de camada fora do limite (0..15)",
            ParseError::NeedsHost => "acentos e ç precisam de `host = abnt2` ou `host = us-intl` no keymap.kbd",
            ParseError::MixedMods => "não dá para misturar modificadores esquerdos e direitos na mesma tecla",
            ParseError::UnknownMacro => "macro desconhecida (veja os nomes em macros.kbd)",
            ParseError::TapKeyWithMods => {
                "no toque do LT só vale tecla simples ou com SHIFT (ex.: ?); no MT, só tecla simples"
            }
        })
    }
}

// Modificadores compactados em 5 bits (como no QMK): bit 4 = lado direito.
fn mods_to_5(m: Mods) -> u16 {
    let right = m.0 & 0xF0 != 0 && m.0 & 0x0F == 0;
    let bits = if right { m.0 >> 4 } else { m.0 & 0x0F };
    (bits as u16 & 0x0F) | if right { 0x10 } else { 0 }
}

fn mods_from_5(v: u16) -> Mods {
    let bits = (v & 0x0F) as u8;
    if v & 0x10 != 0 { Mods(bits << 4) } else { Mods(bits) }
}

impl Action {
    /// Codifica para `u16` (formato persistido e usado pelo protocolo).
    pub fn encode(self) -> u16 {
        match self {
            Action::No => 0x0000,
            Action::Trans => 0x0001,
            Action::Key(k) => k as u16,
            Action::ModKey(m, k) => (mods_to_5(m) << 8) | k as u16,
            Action::ModTap(m, k) => 0x2000 | (mods_to_5(m) << 8) | k as u16,
            Action::LayerTap(l, m, k) => {
                let base = if m.shift() { 0x6000 } else { 0x4000 };
                base | ((l as u16 & 0x0F) << 8) | k as u16
            }
            Action::Momentary(l) => 0x5000 | l as u16,
            Action::Toggle(l) => 0x5100 | l as u16,
            Action::To(l) => 0x5200 | l as u16,
            Action::DefaultLayer(l) => 0x5300 | l as u16,
            Action::Bootloader => 0x7000,
            Action::Reboot => 0x7001,
            Action::Rgb(c) => 0x7100 | c as u16,
            Action::Oled(c) => 0x7200 | c as u16,
            Action::Macro(n) => 0x7300 | n as u16,
            Action::User(n) => 0x7F00 | n as u16,
        }
    }

    /// Decodifica um `u16`. Valores desconhecidos viram `No` (seguro).
    pub fn decode(v: u16) -> Action {
        let lo = (v & 0xFF) as u8;
        let kc = || Keycode::from_u8(lo);
        let r = match v {
            0x0000 => Some(Action::No),
            0x0001 => Some(Action::Trans),
            0x0004..=0x00FF => kc().map(Action::Key),
            0x0100..=0x1FFF => kc().map(|k| Action::ModKey(mods_from_5((v >> 8) & 0x1F), k)),
            0x2000..=0x3FFF => kc().map(|k| Action::ModTap(mods_from_5((v >> 8) & 0x1F), k)),
            0x4000..=0x4FFF => kc().map(|k| Action::LayerTap(((v >> 8) & 0x0F) as u8, Mods::NONE, k)),
            0x6000..=0x6FFF => kc().map(|k| Action::LayerTap(((v >> 8) & 0x0F) as u8, Mods::LSHIFT, k)),
            0x5000..=0x500F => Some(Action::Momentary(lo)),
            0x5100..=0x510F => Some(Action::Toggle(lo)),
            0x5200..=0x520F => Some(Action::To(lo)),
            0x5300..=0x530F => Some(Action::DefaultLayer(lo)),
            0x7000 => Some(Action::Bootloader),
            0x7001 => Some(Action::Reboot),
            0x7100..=0x71FF => RgbCmd::from_u8(lo).map(Action::Rgb),
            0x7200..=0x72FF => OledCmd::from_u8(lo).map(Action::Oled),
            0x7300..=0x73FF => Some(Action::Macro(lo)),
            0x7F00..=0x7FFF => Some(Action::User(lo)),
            _ => None,
        };
        r.unwrap_or(Action::No)
    }

    /// Lê uma ação escrita como texto, ex.: `A`, `LT(nav, SPC)`, `C(S(TAB))`, `!`.
    ///
    /// `layer_by_name` resolve nomes de camada (ex.: `nav`) para o índice.
    /// Símbolos seguem o layout US; para ABNT2/US internacional use [`Action::parse_with`].
    pub fn parse(s: &str, layer_by_name: &dyn Fn(&str) -> Option<u8>) -> Result<Action, ParseError> {
        Action::parse_with(s, Host::Us, layer_by_name)
    }

    /// Como [`Action::parse`], traduzindo símbolos (`ç`, `´`, `[`, `/`...) para a
    /// tecla que os produz no layout `host` do sistema.
    pub fn parse_with(s: &str, host: Host, layer_by_name: &dyn Fn(&str) -> Option<u8>) -> Result<Action, ParseError> {
        Action::parse_full(s, host, layer_by_name, &|_| None)
    }

    /// Como [`Action::parse_with`], resolvendo também nomes de macro em `M(nome)`.
    pub fn parse_full(
        s: &str,
        host: Host,
        layer_by_name: &dyn Fn(&str) -> Option<u8>,
        macro_by_name: &dyn Fn(&str) -> Option<u8>,
    ) -> Result<Action, ParseError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ParseError::BadSyntax);
        }
        if let Some((m, k)) = host.symbol(s) {
            return Ok(if m.is_empty() { Action::Key(k) } else { Action::ModKey(m, k) });
        }
        if host.lacks(s) {
            return Err(ParseError::NeedsHost);
        }
        // Atalhos de símbolos com shift (precisam vir antes do parse de funções).
        if let Some(a) = shifted_symbol(s) {
            return Ok(a);
        }
        match s {
            "_" | "__" | "___" | "____" | "_______" | "TRNS" | "TRANS" => return Ok(Action::Trans),
            "XXX" | "XXXXXXX" | "NO" | "NONE" => return Ok(Action::No),
            "BOOT" | "QK_BOOT" | "RESET" => return Ok(Action::Bootloader),
            "REBOOT" | "QK_RBT" => return Ok(Action::Reboot),
            _ => {}
        }
        if let Some(k) = Keycode::from_name(s) {
            return Ok(Action::Key(k));
        }
        if let Some(c) = RgbCmd::from_name(s) {
            return Ok(Action::Rgb(c));
        }
        if let Some(c) = OledCmd::from_name(s) {
            return Ok(Action::Oled(c));
        }

        // Forma de função: NOME(arg[, arg])
        let (func, args) = split_call(s).ok_or(ParseError::UnknownKey)?;
        let layer = |a: &str| -> Result<u8, ParseError> {
            let a = a.trim();
            let n = match a.parse::<u8>() {
                Ok(n) => n,
                Err(_) => layer_by_name(a).ok_or(ParseError::UnknownLayer)?,
            };
            if n > 15 { Err(ParseError::LayerOutOfRange) } else { Ok(n) }
        };
        // Tecla simples (sem modificador), para MT/LT/MK.
        let key = |a: &str| -> Result<Keycode, ParseError> {
            let a = a.trim();
            match host.symbol(a) {
                Some((m, k)) if m.is_empty() => Ok(k),
                Some(_) => Err(ParseError::TapKeyWithMods),
                None if host.lacks(a) => Err(ParseError::NeedsHost),
                None if shifted_symbol(a).is_some() => Err(ParseError::TapKeyWithMods),
                None => Keycode::from_name(a).ok_or(ParseError::UnknownKey),
            }
        };

        match func {
            "MO" => Ok(Action::Momentary(layer(args)?)),
            "TG" => Ok(Action::Toggle(layer(args)?)),
            "TO" => Ok(Action::To(layer(args)?)),
            "DF" => Ok(Action::DefaultLayer(layer(args)?)),
            "USER" | "U" => args.trim().parse::<u8>().map(Action::User).map_err(|_| ParseError::BadSyntax),
            "M" | "MACRO" => {
                let a = args.trim();
                match a.parse::<u8>() {
                    Ok(n) => Ok(Action::Macro(n)),
                    Err(_) => macro_by_name(a).map(Action::Macro).ok_or(ParseError::UnknownMacro),
                }
            }
            "LT" => {
                // O toque aceita tecla com Shift (ex.: `?`, `!`, `Ç` maiúsculo).
                let (l, k) = split_two(args)?;
                let (m, k) = match Action::parse_full(k, host, layer_by_name, macro_by_name)? {
                    Action::Key(k) => (Mods::NONE, k),
                    Action::ModKey(m, k) if m == Mods::LSHIFT || m == Mods::RSHIFT => (Mods::LSHIFT, k),
                    Action::ModKey(..) => return Err(ParseError::TapKeyWithMods),
                    _ => return Err(ParseError::UnknownKey),
                };
                Ok(Action::LayerTap(layer(l)?, m, k))
            }
            "MT" => {
                let (m, k) = split_two(args)?;
                Ok(Action::ModTap(parse_mods(m)?, key(k)?))
            }
            "MK" => {
                let (m, k) = split_two(args)?;
                Ok(Action::ModKey(same_side(parse_mods(m)?)?, key(k)?))
            }
            _ => {
                // XXX_T(KEY) = mod-tap; MOD(ação) = atalho com modificador.
                if let Some(mname) = func.strip_suffix("_T") {
                    let m = mod_alias(mname).ok_or(ParseError::UnknownKey)?;
                    return Ok(Action::ModTap(m, key(args)?));
                }
                let m = mod_alias(func).ok_or(ParseError::UnknownKey)?;
                match Action::parse_full(args, host, layer_by_name, macro_by_name)? {
                    Action::Key(k) => Ok(Action::ModKey(m, k)),
                    Action::ModKey(m2, k) => Ok(Action::ModKey(same_side(m.union(m2))?, k)),
                    _ => Err(ParseError::BadSyntax),
                }
            }
        }
    }
}

fn split_call(s: &str) -> Option<(&str, &str)> {
    let open = s.find('(')?;
    if !s.ends_with(')') || open == 0 {
        return None;
    }
    Some((&s[..open], &s[open + 1..s.len() - 1]))
}

fn split_two(args: &str) -> Result<(&str, &str), ParseError> {
    // Divide na primeira vírgula de nível zero (permite C(X) no segundo argumento).
    let mut depth = 0i32;
    for (i, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Ok((&args[..i], &args[i + 1..])),
            _ => {}
        }
    }
    Err(ParseError::BadSyntax)
}

fn mod_alias(name: &str) -> Option<Mods> {
    Some(match name {
        "C" => Mods::LCTRL,
        "S" => Mods::LSHIFT,
        "A" => Mods::LALT,
        "G" => Mods::LGUI,
        "RA" => Mods::RALT,
        "RC" => Mods::RCTRL,
        "RS" => Mods::RSHIFT,
        "RG" => Mods::RGUI,
        "CS" => Mods::LCTRL.union(Mods::LSHIFT),
        "CA" => Mods::LCTRL.union(Mods::LALT),
        "MEH" => Mods::LCTRL.union(Mods::LSHIFT).union(Mods::LALT),
        "HYPR" => Mods(0x0F),
        other => return Mods::from_name(other),
    })
}

fn same_side(m: Mods) -> Result<Mods, ParseError> {
    if m.0 & 0x0F != 0 && m.0 & 0xF0 != 0 { Err(ParseError::MixedMods) } else { Ok(m) }
}

/// `LCTL`, `LCTL|LSFT`, `C|S`...
fn parse_mods(s: &str) -> Result<Mods, ParseError> {
    let mut m = Mods::NONE;
    for part in s.split('|') {
        m = m.union(mod_alias(part.trim()).ok_or(ParseError::UnknownKey)?);
    }
    Ok(m)
}

/// Símbolos que são uma tecla com shift (layout US).
const SHIFTED: [(&str, Keycode); 21] = [
    ("!", Keycode::N1),
    ("@", Keycode::N2),
    ("#", Keycode::N3),
    ("$", Keycode::N4),
    ("%", Keycode::N5),
    ("^", Keycode::N6),
    ("&", Keycode::N7),
    ("*", Keycode::N8),
    ("(", Keycode::N9),
    (")", Keycode::N0),
    // "_" é transparente no layout; para underline use UNDS.
    ("UNDS", Keycode::Minus),
    ("+", Keycode::Equal),
    ("{", Keycode::LeftBracket),
    ("}", Keycode::RightBracket),
    ("|", Keycode::Backslash),
    (":", Keycode::Semicolon),
    ("\"", Keycode::Quote),
    ("~", Keycode::Grave),
    ("<", Keycode::Comma),
    (">", Keycode::Dot),
    ("?", Keycode::Slash),
];

fn shifted_symbol(s: &str) -> Option<Action> {
    SHIFTED.iter().find(|(n, _)| *n == s).map(|(_, k)| Action::ModKey(Mods::LSHIFT, *k))
}

/// Nomes dos modificadores, na ordem dos bits do relatório HID.
pub const MOD_NAMES: [&str; 8] = ["CTRL", "SHIFT", "ALT", "WIN", "RIGHT_CTRL", "RIGHT_SHIFT", "ALTGR", "RIGHT_WIN"];

fn write_mods(f: &mut fmt::Formatter<'_>, m: Mods) -> fmt::Result {
    const NAMES: [&str; 8] = MOD_NAMES;
    let mut first = true;
    for (i, n) in NAMES.iter().enumerate() {
        if m.0 & (1 << i) != 0 {
            if !first {
                f.write_str("|")?;
            }
            f.write_str(n)?;
            first = false;
        }
    }
    if first { f.write_str("NONE") } else { Ok(()) }
}

/// Formato textual canônico (o mesmo aceito por `Action::parse`).
impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(f, Host::Us)
    }
}

/// Texto da ação para um layout do sistema (ver [`Action::label`]).
pub struct Label {
    action: Action,
    host: Host,
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.action.write(f, self.host)
    }
}

impl Action {
    /// Texto da ação como o usuário escreveria no keymap com este `host`
    /// (ex.: no ABNT2 a tecla HID `;` aparece como `Ç`). Volta igual pelo
    /// [`Action::parse_with`].
    pub fn label(self, host: Host) -> Label {
        Label { action: self, host }
    }

    fn write(&self, f: &mut fmt::Formatter<'_>, host: Host) -> fmt::Result {
        let name = |k: Keycode| host.label(Mods::NONE, k).unwrap_or(k.name());
        match *self {
            Action::No => f.write_str("XXX"),
            Action::Trans => f.write_str("___"),
            Action::Key(k) => f.write_str(name(k)),
            Action::ModKey(m, k) => {
                if let Some(sym) = host.label(m, k) {
                    return f.write_str(sym);
                }
                // Símbolo com shift (ex.: "!") quando existir e o layout não mudar a tecla.
                if m == Mods::LSHIFT
                    && !host.remaps(k)
                    && let Some(sym) = SHIFTED.iter().find(|(_, kc)| *kc == k).map(|(s, _)| *s)
                {
                    return f.write_str(sym);
                }
                // Forma aninhada CTRL(SHIFT(X)).
                let mut n = 0;
                for (i, mod_name) in MOD_NAMES.iter().enumerate() {
                    if m.0 & (1 << i) != 0 {
                        write!(f, "{mod_name}(")?;
                        n += 1;
                    }
                }
                f.write_str(name(k))?;
                for _ in 0..n {
                    f.write_str(")")?;
                }
                Ok(())
            }
            Action::ModTap(m, k) => {
                f.write_str("MT(")?;
                write_mods(f, m)?;
                write!(f, ",{})", name(k))
            }
            Action::LayerTap(l, m, k) if m.is_empty() => write!(f, "LT({},{})", l, name(k)),
            Action::LayerTap(l, m, k) => write!(f, "LT({},{})", l, Action::ModKey(m, k).label(host)),
            Action::Momentary(l) => write!(f, "MO({})", l),
            Action::Toggle(l) => write!(f, "TG({})", l),
            Action::To(l) => write!(f, "TO({})", l),
            Action::DefaultLayer(l) => write!(f, "DF({})", l),
            Action::Bootloader => f.write_str("BOOT"),
            Action::Reboot => f.write_str("REBOOT"),
            Action::Rgb(c) => f.write_str(c.name()),
            Action::Oled(c) => f.write_str(c.name()),
            Action::Macro(n) => write!(f, "MACRO({})", n),
            Action::User(n) => write!(f, "USER({})", n),
        }
    }
}
