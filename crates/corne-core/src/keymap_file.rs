//! Parser do arquivo de layout em texto (`keymap.kbd`).
//!
//! Formato:
//!
//! ```text
//! // comentário
//! [base]
//! TAB   Q  W  E  R  T      Y  U  I  O  P     BSPC
//! LCTL  A  S  D  F  G      H  J  K  L  ;     '
//! LSFT  Z  X  C  V  B      N  M  ,  .  /     ESC
//!         LGUI MO(nav) SPC     ENT MO(sym) RALT
//!
//! [nav]
//! ...
//! ```
//!
//! Antes das camadas, `host = abnt2` (ou `us-intl`, `us`) diz qual layout de
//! teclado o sistema operacional usa; isso permite escrever `ç`, `´`, `~` e ter
//! os símbolos (`[`, `/`, `;`...) na tecla certa. Ver [`crate::host`].
//!
//! Cada camada tem exatamente 42 teclas na ordem de leitura (3 linhas de 12 e
//! depois os 6 polegares). Quebras de linha e espaços extras não importam, mas
//! manter o desenho alinhado ajuda a ler. Camadas podem ser referenciadas pelo
//! nome (`MO(nav)`) ou pelo número (`MO(1)`), inclusive antes de serem definidas.
//!
//! É `no_std`: o mesmo parser roda no `build.rs` e poderá rodar num
//! configurador no PC.

use core::fmt;

use crate::action::{Action, ParseError};
use crate::host::Host;
use crate::keymap::{EncodedLayer, KEYMAP_LAYERS};
use crate::layout::{COLS, KEY_COUNT, KeyPos, ROWS};
use crate::macros::MAX_MACROS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileErrorKind {
    TooManyLayers,
    KeyOutsideLayer,
    WrongKeyCount { found: usize },
    BadHeader,
    BadHost,
    Action(ParseError),
}

#[derive(Debug, Clone, Copy)]
pub struct FileError<'a> {
    pub line: usize,
    pub token: &'a str,
    pub layer: Option<&'a str>,
    pub kind: FileErrorKind,
}

impl fmt::Display for FileError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "linha {}: ", self.line)?;
        if let Some(l) = self.layer {
            write!(f, "camada [{}]: ", l)?;
        }
        match self.kind {
            FileErrorKind::TooManyLayers => write!(f, "máximo de {} camadas", KEYMAP_LAYERS),
            FileErrorKind::KeyOutsideLayer => {
                write!(f, "tecla '{}' antes de qualquer cabeçalho [camada]", self.token)
            }
            FileErrorKind::WrongKeyCount { found } => {
                write!(f, "esperava {} teclas, encontrei {}", KEY_COUNT, found)
            }
            FileErrorKind::BadHeader => write!(f, "cabeçalho inválido '{}'", self.token),
            FileErrorKind::BadHost => {
                write!(f, "host '{}' desconhecido (use us, us-intl ou abnt2)", self.token)
            }
            FileErrorKind::Action(e) => write!(f, "'{}': {}", self.token, e),
        }
    }
}

/// Resultado do parse.
pub struct ParsedKeymap<'a> {
    pub layers: [EncodedLayer; KEYMAP_LAYERS],
    pub names: [&'a str; KEYMAP_LAYERS],
    pub count: usize,
    /// Layout do sistema declarado com `host = ...` (padrão: US).
    pub host: Host,
    /// Nomes das macros (de `macros.kbd`), na ordem do índice de `M(n)`.
    pub macro_names: heapless::Vec<&'a str, MAX_MACROS>,
}

impl ParsedKeymap<'_> {
    pub fn macro_index(&self, name: &str) -> Option<u8> {
        self.macro_names.iter().position(|n| *n == name).map(|i| i as u8)
    }
}

/// Só o `host = ...` do arquivo (necessário antes de ler as macros).
pub fn read_host(src: &str) -> Result<Host, FileError<'_>> {
    let mut host = Host::Us;
    for (n, raw) in src.lines().enumerate() {
        if let Some(v) = host_directive(strip_comment(raw)) {
            host = Host::from_name(v).ok_or(FileError {
                line: n + 1,
                token: v,
                layer: None,
                kind: FileErrorKind::BadHost,
            })?;
        }
    }
    Ok(host)
}

/// Linha `host = valor`: devolve o valor.
fn host_directive(line: &str) -> Option<&str> {
    let (k, v) = line.split_once('=')?;
    if k.trim() == "host" { Some(v.trim()) } else { None }
}

/// Quebra uma linha em tokens separados por espaço, respeitando parênteses
/// (`LT(nav, SPC)` é um token só).
pub(crate) fn tokens(line: &str) -> impl Iterator<Item = &str> {
    let mut rest = line;
    core::iter::from_fn(move || {
        rest = rest.trim_start();
        if rest.is_empty() {
            return None;
        }
        let mut depth = 0i32;
        let mut end = rest.len();
        for (i, ch) in rest.char_indices() {
            match ch {
                // Só conta parênteses que fazem parte de uma chamada (NOME(...)).
                '(' if i > 0 => depth += 1,
                ')' if depth > 0 => depth -= 1,
                c if c.is_whitespace() && depth == 0 => {
                    end = i;
                    break;
                }
                _ => {}
            }
        }
        let (tok, r) = rest.split_at(end);
        rest = r;
        Some(tok)
    })
}

/// Comentários começam com `//` (o `#` é a tecla "#").
pub(crate) fn strip_comment(line: &str) -> &str {
    match line.find("//") {
        Some(i) => &line[..i],
        None => line,
    }
}

pub(crate) fn header(line: &str) -> Option<&str> {
    let l = line.trim();
    if l.starts_with('[') && l.ends_with(']') && l.len() > 2 { Some(l[1..l.len() - 1].trim()) } else { None }
}

/// Faz o parse do arquivo inteiro (sem macros).
pub fn parse(src: &str) -> Result<ParsedKeymap<'_>, FileError<'_>> {
    parse_with_macros(src, &[])
}

/// Faz o parse do arquivo inteiro; `macros` são os nomes usados em `M(nome)`.
pub fn parse_with_macros<'a>(src: &'a str, macros: &[&'a str]) -> Result<ParsedKeymap<'a>, FileError<'a>> {
    let mut out = ParsedKeymap {
        macro_names: macros.iter().take(MAX_MACROS).copied().collect(),
        layers: [[[0u16; COLS]; ROWS]; KEYMAP_LAYERS],
        names: [""; KEYMAP_LAYERS],
        count: 0,
        host: Host::Us,
    };

    // 1ª passada: nomes das camadas (permite referências adiante).
    for (n, raw) in src.lines().enumerate() {
        let line = strip_comment(raw);
        if let Some(v) = host_directive(line) {
            out.host = Host::from_name(v).ok_or(FileError {
                line: n + 1,
                token: v,
                layer: None,
                kind: FileErrorKind::BadHost,
            })?;
            continue;
        }
        if let Some(name) = header(line) {
            if out.count >= KEYMAP_LAYERS {
                return Err(FileError { line: n + 1, token: name, layer: None, kind: FileErrorKind::TooManyLayers });
            }
            if name.is_empty() || name.contains(char::is_whitespace) {
                return Err(FileError { line: n + 1, token: name, layer: None, kind: FileErrorKind::BadHeader });
            }
            out.names[out.count] = name;
            out.count += 1;
        }
    }

    let names = out.names;
    let count = out.count;
    let host = out.host;
    let by_name = move |s: &str| -> Option<u8> { names[..count].iter().position(|n| *n == s).map(|i| i as u8) };

    // 2ª passada: teclas.
    let mut layer: Option<usize> = None;
    let mut key_idx = 0usize;
    let mut header_line = 0usize;

    let finish = |layer: Option<usize>, key_idx: usize, header_line: usize| -> Result<(), FileError<'_>> {
        if let Some(l) = layer
            && key_idx != KEY_COUNT
        {
            return Err(FileError {
                line: header_line,
                token: "",
                layer: Some(names[l]),
                kind: FileErrorKind::WrongKeyCount { found: key_idx },
            });
        }
        Ok(())
    };

    for (n, raw) in src.lines().enumerate() {
        let line = strip_comment(raw);
        if host_directive(line).is_some() {
            continue;
        }
        if header(line).is_some() {
            finish(layer, key_idx, header_line)?;
            layer = Some(layer.map_or(0, |l| l + 1));
            key_idx = 0;
            header_line = n + 1;
            continue;
        }
        for tok in tokens(line) {
            let Some(l) = layer else {
                return Err(FileError { line: n + 1, token: tok, layer: None, kind: FileErrorKind::KeyOutsideLayer });
            };
            if key_idx >= KEY_COUNT {
                return Err(FileError {
                    line: n + 1,
                    token: tok,
                    layer: Some(names[l]),
                    kind: FileErrorKind::WrongKeyCount { found: key_idx + 1 },
                });
            }
            let by_macro = |s: &str| macros.iter().position(|n| *n == s).map(|i| i as u8);
            let action = Action::parse_full(tok, host, &by_name, &by_macro).map_err(|e| FileError {
                line: n + 1,
                token: tok,
                layer: Some(names[l]),
                kind: FileErrorKind::Action(e),
            })?;
            let pos = KeyPos::from_index(key_idx);
            out.layers[l][pos.row as usize][pos.col as usize] = action.encode();
            key_idx += 1;
        }
    }
    finish(layer, key_idx, header_line)?;
    Ok(out)
}
