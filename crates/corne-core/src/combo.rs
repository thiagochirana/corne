//! Combos: duas ou mais teclas apertadas juntas disparam uma ação.
//!
//! Configurados em `combos.kbd` (ao lado do `keymap.kbd`):
//!
//! ```text
//! term = 40              // janela em ms para apertar todas as teclas (opcional)
//! idle = 150             // combo só vale após essa pausa na digitação (opcional, 0 desliga)
//!
//! J K      => ESC        // vale em qualquer camada
//! D F      => TAB
//! J K L    => USER(1)    // ação do usuário (futuras macros)
//!
//! [nav]                  // daqui para baixo: só quando a camada nav estiver no topo
//! H J      => CTRL(Z)
//! K(0,1) K(0,2) => BOOT  // posição explícita: K(linha, coluna)
//! ```
//!
//! As teclas são escritas como aparecem no keymap (na camada `base`, ou na
//! camada da seção), e o combo usa a **posição** delas. Qualquer número de
//! teclas (2 a [`MAX_COMBO_KEYS`]). A ação aceita tudo o que o keymap aceita.
//!
//! Funcionamento (no [`crate::engine`]): uma tecla que faz parte de algum combo
//! espera até `term` ms pelas outras. Se o combo se completar, a ação dispara
//! (e é solta quando a primeira tecla do combo for solta); se não, as teclas
//! saem normalmente, na ordem em que foram apertadas. Teclas que não fazem
//! parte de nenhum combo não sofrem atraso. No meio de uma palavra (última
//! letra há menos de `idle` ms), nenhum combo começa: "po" de "tempo" sai como
//! texto mesmo com `P O` sendo um combo.

use core::fmt;

use crate::action::{Action, ParseError};
use crate::keymap_file::{ParsedKeymap, header, strip_comment, tokens};
use crate::layout::{KEY_COUNT, KeyPos};

/// Máximo de combos.
pub const MAX_COMBOS: usize = 64;
/// Máximo de teclas num combo.
pub const MAX_COMBO_KEYS: usize = 16;
/// Janela padrão para apertar as teclas do combo (ms).
pub const DEFAULT_TERM_MS: u16 = 40;
/// Pausa padrão na digitação antes de um combo valer (ms).
pub const DEFAULT_IDLE_MS: u16 = 150;
/// `layer` de um combo que vale em qualquer camada.
pub const ANY_LAYER: u8 = 0xFF;

/// Um combo. Tudo codificado em números simples para poder ser gerado como
/// constante no firmware e, no futuro, gravado na flash.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Combo {
    /// Bit `i` = tecla `KeyPos::from_index(i)`.
    pub keys: u64,
    /// Ação codificada (`Action::encode`).
    pub action: u16,
    /// Camada em que vale (a mais alta ativa), ou [`ANY_LAYER`].
    pub layer: u8,
}

impl Combo {
    pub fn applies(&self, layer: u8) -> bool {
        self.layer == ANY_LAYER || self.layer == layer
    }

    /// Posições das teclas do combo.
    pub fn positions(&self) -> impl Iterator<Item = KeyPos> + '_ {
        (0..KEY_COUNT).filter(|i| self.keys & (1 << i) != 0).map(KeyPos::from_index)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboErrorKind {
    /// Linha sem `=>`.
    MissingArrow,
    BadTerm,
    BadIdle,
    TooFewKeys,
    TooManyKeys,
    TooManyCombos,
    DuplicateKey,
    /// A tecla não existe na camada de referência.
    KeyNotFound,
    /// A tecla aparece em mais de uma posição: use `K(linha, coluna)`.
    AmbiguousKey,
    BadPosition,
    UnknownLayer,
    /// Mesmo conjunto de teclas na mesma camada duas vezes.
    DuplicateCombo,
    EmptyAction,
    Action(ParseError),
}

#[derive(Debug, Clone, Copy)]
pub struct ComboError<'a> {
    pub line: usize,
    pub token: &'a str,
    pub kind: ComboErrorKind,
}

impl fmt::Display for ComboError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ComboErrorKind::*;
        write!(f, "linha {}: ", self.line)?;
        let t = self.token;
        match self.kind {
            MissingArrow => write!(f, "esperava 'teclas => ação', encontrei '{t}'"),
            BadTerm => write!(f, "term inválido '{t}' (use ms, ex.: term = 40)"),
            BadIdle => write!(f, "idle inválido '{t}' (use ms de 0 a 1000, ex.: idle = 150)"),
            TooFewKeys => write!(f, "um combo precisa de pelo menos 2 teclas"),
            TooManyKeys => write!(f, "máximo de {MAX_COMBO_KEYS} teclas por combo"),
            TooManyCombos => write!(f, "máximo de {MAX_COMBOS} combos"),
            DuplicateKey => write!(f, "tecla '{t}' repetida no combo"),
            KeyNotFound => write!(f, "tecla '{t}' não está na camada de referência (use K(linha, coluna))"),
            AmbiguousKey => write!(f, "tecla '{t}' aparece em mais de uma posição; use K(linha, coluna)"),
            BadPosition => write!(f, "posição inválida '{t}' (linhas 0..3, colunas 0..11)"),
            UnknownLayer => write!(f, "camada '{t}' não existe no keymap.kbd"),
            DuplicateCombo => write!(f, "já existe um combo com essas teclas nessa camada"),
            EmptyAction => write!(f, "ação '{t}' não faz nada num combo"),
            Action(e) => write!(f, "'{t}': {e}"),
        }
    }
}

pub struct ParsedCombos {
    pub combos: heapless::Vec<Combo, MAX_COMBOS>,
    pub term_ms: u16,
    pub idle_ms: u16,
}

/// Linha `nome = N` (`term`, `idle`).
fn directive<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let (k, v) = line.split_once('=')?;
    if k.trim() == name && !v.starts_with('>') { Some(v.trim()) } else { None }
}

/// `K(linha, coluna)`
fn explicit_pos(tok: &str) -> Option<Result<KeyPos, ()>> {
    let inner = tok.strip_prefix("K(")?.strip_suffix(')')?;
    let (r, c) = inner.split_once(',')?;
    let (Ok(r), Ok(c)) = (r.trim().parse::<u8>(), c.trim().parse::<u8>()) else { return Some(Err(())) };
    let pos = KeyPos::new(r, c);
    Some(if r < 4 && c < 12 && pos.exists() { Ok(pos) } else { Err(()) })
}

/// Faz o parse do `combos.kbd`. Usa o keymap já lido para achar as teclas pelo
/// nome, resolver nomes de camada e o `host` (acentos, ç).
pub fn parse<'a>(src: &'a str, km: &ParsedKeymap<'_>) -> Result<ParsedCombos, ComboError<'a>> {
    let mut out = ParsedCombos { combos: heapless::Vec::new(), term_ms: DEFAULT_TERM_MS, idle_ms: DEFAULT_IDLE_MS };
    let names = &km.names[..km.count];
    let by_name = |s: &str| -> Option<u8> { names.iter().position(|n| *n == s).map(|i| i as u8) };
    // Seção atual: camada do filtro (ANY_LAYER antes de qualquer seção).
    let mut layer = ANY_LAYER;

    for (n, raw) in src.lines().enumerate() {
        let line_no = n + 1;
        let line = strip_comment(raw);
        let err = |token: &'a str, kind| ComboError { line: line_no, token, kind };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(name) = header(line) {
            layer = by_name(name).ok_or(err(name, ComboErrorKind::UnknownLayer))?;
            continue;
        }
        if let Some(v) = directive(line, "term") {
            out.term_ms = v.parse().ok().filter(|t| (1..=1000).contains(t)).ok_or(err(v, ComboErrorKind::BadTerm))?;
            continue;
        }
        if let Some(v) = directive(line, "idle") {
            out.idle_ms = v.parse().ok().filter(|t| *t <= 1000).ok_or(err(v, ComboErrorKind::BadIdle))?;
            continue;
        }
        let (keys_src, action_src) = line.split_once("=>").ok_or(err(line.trim(), ComboErrorKind::MissingArrow))?;
        let action_src = action_src.trim();

        // Teclas: pelo nome (na camada da seção, ou na base) ou K(linha, coluna).
        let ref_layer = if layer == ANY_LAYER { 0 } else { layer as usize };
        let mut keys = 0u64;
        let mut count = 0usize;
        for tok in tokens(keys_src) {
            let pos = match explicit_pos(tok) {
                Some(Ok(p)) => p,
                Some(Err(())) => return Err(err(tok, ComboErrorKind::BadPosition)),
                None => {
                    let want = Action::parse_with(tok, km.host, &by_name)
                        .map_err(|e| err(tok, ComboErrorKind::Action(e)))?
                        .encode();
                    let mut found = None;
                    for i in 0..KEY_COUNT {
                        let p = KeyPos::from_index(i);
                        if km.layers[ref_layer][p.row as usize][p.col as usize] == want {
                            if found.is_some() {
                                return Err(err(tok, ComboErrorKind::AmbiguousKey));
                            }
                            found = Some(p);
                        }
                    }
                    found.ok_or(err(tok, ComboErrorKind::KeyNotFound))?
                }
            };
            let bit = 1u64 << pos.index();
            if keys & bit != 0 {
                return Err(err(tok, ComboErrorKind::DuplicateKey));
            }
            keys |= bit;
            count += 1;
        }
        if count < 2 {
            return Err(err(keys_src.trim(), ComboErrorKind::TooFewKeys));
        }
        if count > MAX_COMBO_KEYS {
            return Err(err(keys_src.trim(), ComboErrorKind::TooManyKeys));
        }

        let by_macro = |s: &str| km.macro_index(s);
        let action = Action::parse_full(action_src, km.host, &by_name, &by_macro)
            .map_err(|e| err(action_src, ComboErrorKind::Action(e)))?;
        if matches!(action, Action::No | Action::Trans) {
            return Err(err(action_src, ComboErrorKind::EmptyAction));
        }
        if out.combos.iter().any(|c| c.keys == keys && c.layer == layer) {
            return Err(err(keys_src.trim(), ComboErrorKind::DuplicateCombo));
        }
        out.combos
            .push(Combo { keys, action: action.encode(), layer })
            .map_err(|_| err(keys_src.trim(), ComboErrorKind::TooManyCombos))?;
    }
    Ok(out)
}
