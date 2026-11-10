//! Layout de teclado do sistema operacional (o "idioma do teclado" no Windows/Linux).
//!
//! O teclado USB não manda caracteres, manda **posições de tecla** (códigos HID,
//! com nomes do layout americano). Quem transforma a posição em caractere é o
//! sistema. Ex.: a tecla HID `;` vira `ç` num Windows configurado em ABNT2.
//!
//! No `keymap.kbd`, a linha `host = abnt2` diz qual layout o sistema usa. Assim
//! você escreve o caractere que quer (`ç`, `´`, `[`, `/`) e o parser escolhe a
//! tecla HID certa para aquele layout. Nomes de tecla (`SCLN`, `LBRC`, `QUOT`)
//! continuam sendo posições fixas, sem tradução.
//!
//! Acentos (`´ ~ ^ \` ¨`) são **teclas mortas** no sistema: aperte o acento e
//! depois a letra (`´` + `a` = `á`).

use crate::keycode::{Keycode, Mods};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Host {
    /// Inglês (EUA). Sem acentos nem ç.
    #[default]
    Us,
    /// Inglês (EUA, internacional): acentos como teclas mortas, ç = AltGr + `,`.
    UsIntl,
    /// Português (Brasil ABNT2).
    Abnt2,
}

const N: Mods = Mods::NONE;
const S: Mods = Mods::LSHIFT;
const RA: Mods = Mods::RALT;

type Entry = (&'static str, Mods, Keycode);

/// ABNT2: só as teclas que mudam em relação ao layout US. O resto (letras,
/// números, `! @ # $ % & * ( ) - = + , . < >`) é igual.
/// Na busca reversa (mostrar o nome), vale a primeira entrada.
const ABNT2: [Entry; 27] = [
    ("'", N, Keycode::Grave),
    ("\"", S, Keycode::Grave),
    ("´", N, Keycode::LeftBracket),
    ("ACUT", N, Keycode::LeftBracket),
    ("`", S, Keycode::LeftBracket),
    ("GRAVE", S, Keycode::LeftBracket),
    ("[", N, Keycode::RightBracket),
    ("{", S, Keycode::RightBracket),
    // A tecla `] }` do ABNT2 fica onde o US tem `\`.
    ("]", N, Keycode::Backslash),
    ("}", S, Keycode::Backslash),
    ("Ç", N, Keycode::Semicolon),
    ("ç", N, Keycode::Semicolon),
    ("CCED", N, Keycode::Semicolon),
    ("~", N, Keycode::Quote),
    ("TILD", N, Keycode::Quote),
    ("^", S, Keycode::Quote),
    ("CIRC", S, Keycode::Quote),
    (";", N, Keycode::Slash),
    (":", S, Keycode::Slash),
    // Tecla extra ao lado do Shift esquerdo.
    ("\\", N, Keycode::NonUsBackslash),
    ("|", S, Keycode::NonUsBackslash),
    // Tecla extra ao lado do Shift direito.
    ("/", N, Keycode::International1),
    ("?", S, Keycode::International1),
    ("¨", S, Keycode::N6),
    ("DIAE", S, Keycode::N6),
    // Nomes para as teclas extras, se quiser usá-las direto.
    ("ABNT_BSLS", N, Keycode::NonUsBackslash),
    ("ABNT_SLSH", N, Keycode::International1),
];

/// US internacional: igual ao US, mais acentos mortos e ç.
const US_INTL: [Entry; 13] = [
    ("'", N, Keycode::Quote),
    ("´", N, Keycode::Quote),
    ("ACUT", N, Keycode::Quote),
    ("\"", S, Keycode::Quote),
    ("¨", S, Keycode::Quote),
    ("DIAE", S, Keycode::Quote),
    ("Ç", RA, Keycode::Comma),
    ("ç", RA, Keycode::Comma),
    ("CCED", RA, Keycode::Comma),
    ("~", S, Keycode::Grave),
    ("TILD", S, Keycode::Grave),
    ("^", S, Keycode::N6),
    ("CIRC", S, Keycode::N6),
];

const US: [Entry; 4] =
    [("~", S, Keycode::Grave), ("TILD", S, Keycode::Grave), ("^", S, Keycode::N6), ("CIRC", S, Keycode::N6)];

/// Símbolos que só existem em layouts com acentos.
const NEEDS_INTL: [&str; 9] = ["´", "ACUT", "¨", "DIAE", "Ç", "ç", "CCED", "GRAVE", "ABNT_BSLS"];

impl Host {
    pub const ALL: [Host; 3] = [Host::Us, Host::UsIntl, Host::Abnt2];

    pub fn from_name(s: &str) -> Option<Host> {
        Some(match s {
            "us" => Host::Us,
            "us-intl" | "us-international" | "intl" => Host::UsIntl,
            "abnt2" | "abnt" | "br" | "pt-br" => Host::Abnt2,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Host::Us => "us",
            Host::UsIntl => "us-intl",
            Host::Abnt2 => "abnt2",
        }
    }

    fn table(self) -> &'static [Entry] {
        match self {
            Host::Us => &US,
            Host::UsIntl => &US_INTL,
            Host::Abnt2 => &ABNT2,
        }
    }

    /// Tecla HID (com modificadores) que produz o símbolo `s` neste layout.
    /// `None` se o símbolo não muda em relação ao US.
    pub fn symbol(self, s: &str) -> Option<(Mods, Keycode)> {
        self.table().iter().find(|(n, _, _)| *n == s).map(|(_, m, k)| (*m, *k))
    }

    /// O símbolo só existe em layouts com acento (e este layout é o US).
    pub fn lacks(self, s: &str) -> bool {
        self.symbol(s).is_none() && NEEDS_INTL.contains(&s)
    }

    /// Nome para mostrar a tecla HID `k` com `m`, se este layout a redefine.
    pub fn label(self, m: Mods, k: Keycode) -> Option<&'static str> {
        self.table().iter().find(|(_, em, ek)| *em == m && *ek == k).map(|(n, _, _)| *n)
    }

    /// A tecla `k` produz outro caractere neste layout (com ou sem shift)?
    pub fn remaps(self, k: Keycode) -> bool {
        self.table().iter().any(|(_, _, ek)| *ek == k)
    }
}
