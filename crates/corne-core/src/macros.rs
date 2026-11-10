//! Macros: uma sequência de teclas tocada por uma tecla (`M(nome)` no
//! `keymap.kbd`) ou por um combo (`combos.kbd`).
//!
//! Definidas em `macros.kbd`:
//!
//! ```text
//! delay = 0                      // ms entre uma tecla e a próxima (opcional)
//!
//! [assinatura]
//! "Atenciosamente," ENT "Fulano"
//!
//! [salvar_tudo]
//! CTRL(SHIFT(S)) WAIT(100) ESC
//!
//! [print]
//! DOWN(WIN) DOWN(SHIFT) S UP(SHIFT) UP(WIN)
//! ```
//!
//! Passos:
//! - `"texto"` ou `'texto'`: digita o texto, inclusive acentos e ç (usa o
//!   `host` do keymap.kbd). Escapes: `\"`, `\'`, `\\`, `\n` (Enter), `\t` (Tab).
//! - `ESC`, `CTRL(Z)`, `!`...: toca a tecla (aperta e solta), com os modificadores.
//! - `DOWN(tecla)` / `UP(tecla)`: segura / solta (o que ficar segurado é solto
//!   no fim da macro).
//! - `WAIT(ms)`: espera.
//! - `ENV(NOME)`: digita o valor de `NOME` do arquivo `secrets.env` (fora do
//!   git), para senhas e outros segredos não ficarem escritos aqui. Ver [`env_value`].
//!
//! Uma macro pode ocupar várias linhas; a quebra de linha do arquivo **não**
//! digita Enter (use `ENT` ou `\n`). Comentários `//` fora das aspas.

use core::fmt;

use crate::action::{Action, ParseError};
use crate::host::Host;
use crate::keycode::{Keycode, Mods};

/// Máximo de macros.
pub const MAX_MACROS: usize = 64;
/// Máximo de passos somando todas as macros (cada letra de um texto é 1 a 3 passos).
pub const MAX_STEPS: usize = 4096;
/// Maior espera num `WAIT` (ms); para mais, use vários.
pub const MAX_WAIT_MS: u16 = 0x1FFF;

/// Um passo de macro.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// Aperta e solta a tecla com os modificadores.
    Tap(Mods, Keycode),
    Down(Keycode),
    Up(Keycode),
    Wait(u16),
}

impl Step {
    /// Codificação em `u16`: `Tap` usa a mesma faixa de `Action::Key`/`ModKey`
    /// (`0x0004..=0x1FFF`), `Down` = `0x8000 | kc`, `Up` = `0x8100 | kc`,
    /// `Wait` = `0xA000 | ms`.
    pub fn encode(self) -> u16 {
        match self {
            Step::Tap(m, k) if m.is_empty() => Action::Key(k).encode(),
            Step::Tap(m, k) => Action::ModKey(m, k).encode(),
            Step::Down(k) => 0x8000 | k as u16,
            Step::Up(k) => 0x8100 | k as u16,
            Step::Wait(ms) => 0xA000 | ms.min(MAX_WAIT_MS),
        }
    }

    pub fn decode(v: u16) -> Option<Step> {
        let kc = || Keycode::from_u8((v & 0xFF) as u8);
        match v {
            0x0004..=0x1FFF => match Action::decode(v) {
                Action::Key(k) => Some(Step::Tap(Mods::NONE, k)),
                Action::ModKey(m, k) => Some(Step::Tap(m, k)),
                _ => None,
            },
            0x8000..=0x80FF => kc().map(Step::Down),
            0x8100..=0x81FF => kc().map(Step::Up),
            0xA000..=0xBFFF => Some(Step::Wait(v & MAX_WAIT_MS)),
            _ => None,
        }
    }
}

/// Macros prontas para o motor: os passos de todas, um trecho por macro.
#[derive(Clone, Copy, Debug)]
pub struct MacroTable {
    /// Passos codificados ([`Step::encode`]).
    pub steps: &'static [u16],
    /// `(início, quantidade)` em `steps`, uma entrada por macro.
    pub spans: &'static [(u16, u16)],
    /// Espera entre teclas (ms).
    pub delay_ms: u16,
}

impl MacroTable {
    pub const EMPTY: MacroTable = MacroTable { steps: &[], spans: &[], delay_ms: 0 };

    /// Passos da macro `n`.
    pub fn get(&self, n: u8) -> Option<&'static [u16]> {
        let (start, len) = *self.spans.get(n as usize)?;
        self.steps.get(start as usize..start as usize + len as usize)
    }
}

// ----------------------------------------------------------------- texto

/// Tecla de um símbolo neste layout (o mesmo que o keymap aceita).
fn key_for(host: Host, s: &str) -> Option<(Mods, Keycode)> {
    match Action::parse_with(s, host, &|_| None) {
        Ok(Action::Key(k)) => Some((Mods::NONE, k)),
        Ok(Action::ModKey(m, k)) => Some((m, k)),
        _ => None,
    }
}

/// Acrescenta Shift do mesmo lado dos modificadores (SHIFT+ALTGR não cabe na codificação).
fn shifted(m: Mods) -> Mods {
    if m.0 & 0xF0 != 0 { m.union(Mods::RSHIFT) } else { m.union(Mods::LSHIFT) }
}

/// Teclas mortas deste layout (o acento espera a próxima letra).
fn is_dead(host: Host, key: (Mods, Keycode)) -> bool {
    host != Host::Us && ["´", "`", "~", "^", "¨"].iter().any(|s| key_for(host, s) == Some(key))
}

/// Letra acentuada => (acento, letra base).
fn decompose(c: char) -> Option<(&'static str, char)> {
    Some(match c {
        'á' => ("´", 'a'),
        'é' => ("´", 'e'),
        'í' => ("´", 'i'),
        'ó' => ("´", 'o'),
        'ú' => ("´", 'u'),
        'ý' => ("´", 'y'),
        'à' => ("`", 'a'),
        'è' => ("`", 'e'),
        'ì' => ("`", 'i'),
        'ò' => ("`", 'o'),
        'ù' => ("`", 'u'),
        'â' => ("^", 'a'),
        'ê' => ("^", 'e'),
        'î' => ("^", 'i'),
        'ô' => ("^", 'o'),
        'û' => ("^", 'u'),
        'ã' => ("~", 'a'),
        'õ' => ("~", 'o'),
        'ñ' => ("~", 'n'),
        'ä' => ("¨", 'a'),
        'ë' => ("¨", 'e'),
        'ï' => ("¨", 'i'),
        'ö' => ("¨", 'o'),
        'ü' => ("¨", 'u'),
        'ÿ' => ("¨", 'y'),
        _ => return None,
    })
}

/// Teclas para digitar o caractere `c` com o layout `host` (1 a 3 toques).
pub fn type_char(host: Host, c: char) -> Result<heapless::Vec<(Mods, Keycode), 3>, ParseError> {
    let mut out = heapless::Vec::new();
    let letter = |l: char| -> Option<(Mods, Keycode)> {
        let mut b = [0u8; 4];
        let k = Keycode::from_name(l.to_ascii_uppercase().encode_utf8(&mut b))?;
        Some(if l.is_ascii_uppercase() { (Mods::LSHIFT, k) } else { (Mods::NONE, k) })
    };
    let lower = c.to_lowercase().next().unwrap_or(c);
    let upper = c != lower;

    let keys: (Mods, Keycode) = match c {
        'a'..='z' | 'A'..='Z' | '0'..='9' => letter(c).ok_or(ParseError::UnknownKey)?,
        ' ' => (Mods::NONE, Keycode::Space),
        '\n' => (Mods::NONE, Keycode::Enter),
        '\t' => (Mods::NONE, Keycode::Tab),
        '_' => key_for(host, "UNDS").ok_or(ParseError::UnknownKey)?,
        'ç' | 'Ç' => {
            if host == Host::Us {
                return Err(ParseError::NeedsHost);
            }
            let (m, k) = key_for(host, "ç").ok_or(ParseError::NeedsHost)?;
            (if upper { shifted(m) } else { m }, k)
        }
        _ => match decompose(lower) {
            Some((accent, base)) => {
                if host == Host::Us {
                    return Err(ParseError::NeedsHost);
                }
                let dead = key_for(host, accent).ok_or(ParseError::NeedsHost)?;
                let (m, k) = letter(base).ok_or(ParseError::UnknownKey)?;
                let _ = out.push(dead);
                let _ = out.push((if upper { Mods::LSHIFT } else { m }, k));
                return Ok(out);
            }
            None => {
                let mut b = [0u8; 4];
                key_for(host, c.encode_utf8(&mut b)).ok_or(ParseError::UnknownKey)?
            }
        },
    };
    let _ = out.push(keys);
    // Acento sozinho: tecla morta + espaço digita o próprio acento.
    if is_dead(host, keys) {
        let _ = out.push((Mods::NONE, Keycode::Space));
    }
    Ok(out)
}

// ----------------------------------------------------------------- arquivo

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroErrorKind {
    BadName,
    DuplicateName,
    StepOutsideMacro,
    EmptyMacro,
    UnterminatedString,
    BadEscape,
    BadWait,
    BadDelay,
    TooManyMacros,
    TooManySteps,
    /// Passo que não é tecla (ex.: MO(1), RGB_TOG, outra macro).
    NotAKey,
    /// Caractere que o layout não consegue digitar.
    BadChar(char),
    /// `ENV(NOME)` sem valor no `secrets.env` (nem no ambiente).
    MissingEnv,
    BadEnvName,
    /// O valor de um `ENV` tem um caractere que o layout não digita (o valor não é mostrado).
    EnvBadChar,
    Action(ParseError),
}

#[derive(Debug, Clone, Copy)]
pub struct MacroError<'a> {
    pub line: usize,
    pub token: &'a str,
    pub kind: MacroErrorKind,
}

impl fmt::Display for MacroError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MacroErrorKind::*;
        write!(f, "linha {}: ", self.line)?;
        let t = self.token;
        match self.kind {
            BadName => write!(f, "nome de macro inválido '{t}' (use letras, números, _ ou -)"),
            DuplicateName => write!(f, "macro '{t}' definida duas vezes"),
            StepOutsideMacro => write!(f, "'{t}' antes de qualquer [nome] de macro"),
            EmptyMacro => write!(f, "macro '{t}' está vazia"),
            UnterminatedString => write!(f, "texto sem as aspas de fechamento"),
            BadEscape => write!(f, "escape inválido em {t} (use \\\" \\' \\\\ \\n \\t)"),
            BadWait => write!(f, "WAIT inválido '{t}' (ms, até {MAX_WAIT_MS})"),
            BadDelay => write!(f, "delay inválido '{t}' (ms, ex.: delay = 5)"),
            TooManyMacros => write!(f, "máximo de {MAX_MACROS} macros"),
            TooManySteps => write!(f, "macros grandes demais (máximo de {MAX_STEPS} passos no total)"),
            NotAKey => write!(f, "'{t}' não é uma tecla (macros só tocam teclas, DOWN/UP e WAIT)"),
            BadChar(c) => write!(f, "o caractere '{c}' não pode ser digitado com este host (veja host no keymap.kbd)"),
            MissingEnv => write!(f, "{t}: a variável não está no secrets.env (nem no ambiente)"),
            BadEnvName => write!(f, "nome inválido em {t} (use letras, números e _)"),
            EnvBadChar => write!(f, "{t}: o valor tem um caractere que este host não digita (o valor não é mostrado)"),
            Action(e) => write!(f, "'{t}': {e}"),
        }
    }
}

pub struct ParsedMacros<'a> {
    pub names: heapless::Vec<&'a str, MAX_MACROS>,
    pub steps: heapless::Vec<u16, MAX_STEPS>,
    pub spans: heapless::Vec<(u16, u16), MAX_MACROS>,
    pub delay_ms: u16,
    /// A macro usa `ENV(...)` (tem segredo): ferramentas não devem mostrar o conteúdo.
    pub secret: heapless::Vec<bool, MAX_MACROS>,
    /// Nomes usados em `ENV(...)`, sem repetição.
    pub env_names: heapless::Vec<&'a str, MAX_MACROS>,
}

impl ParsedMacros<'_> {
    pub fn index(&self, name: &str) -> Option<u8> {
        self.names.iter().position(|n| *n == name).map(|i| i as u8)
    }
}

/// Token de uma linha de macro: texto entre aspas ou palavra (respeitando parênteses).
enum Tok<'a> {
    Text(&'a str),
    Word(&'a str),
}

/// Quebra a linha em tokens; para no `//` fora das aspas.
fn line_tokens<'a>(
    line: &'a str,
    mut f: impl FnMut(Tok<'a>) -> Result<(), MacroErrorKind>,
) -> Result<(), (MacroErrorKind, &'a str)> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            c if c.is_ascii_whitespace() => i += 1,
            b'/' if b.get(i + 1) == Some(&b'/') => break,
            // Texto entre aspas duplas ou simples. Um ' sozinho é a tecla do apóstrofo.
            q @ (b'"' | b'\'') if !(q == b'\'' && b.get(i + 1).is_none_or(|c| c.is_ascii_whitespace())) => {
                let start = i + 1;
                let mut j = start;
                loop {
                    match b.get(j) {
                        None => return Err((MacroErrorKind::UnterminatedString, &line[i..])),
                        Some(b'\\') => j += 2,
                        Some(&c) if c == q => break,
                        Some(_) => j += 1,
                    }
                }
                let text = &line[start..j];
                f(Tok::Text(text)).map_err(|k| (k, &line[i..=j]))?;
                i = j + 1;
            }
            _ => {
                let start = i;
                let mut depth = 0i32;
                while i < b.len() {
                    match b[i] {
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        c if c.is_ascii_whitespace() && depth <= 0 => break,
                        _ => {}
                    }
                    i += 1;
                }
                let w = &line[start..i];
                f(Tok::Word(w)).map_err(|k| (k, w))?;
            }
        }
    }
    Ok(())
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Faz o parse do `macros.kbd`. `host` vem do `keymap.kbd` ([`crate::keymap_file::read_host`]).
/// `ENV(NOME)` sempre falha (sem segredos disponíveis).
pub fn parse(src: &str, host: Host) -> Result<ParsedMacros<'_>, MacroError<'_>> {
    parse_with_env(src, host, &|_| None)
}

/// Valor de `NOME` num arquivo no formato `.env` (`secrets.env`):
///
/// ```text
/// # comentário
/// SENHA_BANCO=valor sem aspas      (espaços nas pontas são removidos)
/// FRASE="  com espaços  "          (aspas mantêm tudo exatamente como está)
/// export PIN=1234                  (o "export " é ignorado)
/// ```
pub fn env_value<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let mut found = None;
    for line in src.lines() {
        let l = line.trim_start();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let l = l.strip_prefix("export ").unwrap_or(l);
        let Some((k, v)) = l.split_once('=') else { continue };
        if k.trim() != name {
            continue;
        }
        let v = v.trim();
        let quoted =
            v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')));
        // A última definição vale (como no shell).
        found = Some(if quoted { &v[1..v.len() - 1] } else { v });
    }
    found
}

fn valid_env_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Como [`parse`], com `env` respondendo o valor de cada `ENV(NOME)`.
pub fn parse_with_env<'a, 'e>(
    src: &'a str,
    host: Host,
    env: &dyn Fn(&str) -> Option<&'e str>,
) -> Result<ParsedMacros<'a>, MacroError<'a>> {
    let mut out = ParsedMacros {
        names: heapless::Vec::new(),
        steps: heapless::Vec::new(),
        spans: heapless::Vec::new(),
        delay_ms: 0,
        secret: heapless::Vec::new(),
        env_names: heapless::Vec::new(),
    };

    for (n, raw) in src.lines().enumerate() {
        let line_no = n + 1;
        let err = |token, kind| MacroError { line: line_no, token, kind };
        let t = raw.trim();
        let code = t.split("//").next().unwrap_or("").trim();

        // [nome]
        if code.starts_with('[') && code.ends_with(']') {
            let name = code[1..code.len() - 1].trim();
            if !valid_name(name) {
                return Err(err(name, MacroErrorKind::BadName));
            }
            if out.index(name).is_some() {
                return Err(err(name, MacroErrorKind::DuplicateName));
            }
            close_macro(&mut out).map_err(|k| err(name, k))?;
            out.names.push(name).map_err(|_| err(name, MacroErrorKind::TooManyMacros))?;
            out.spans.push((out.steps.len() as u16, 0)).map_err(|_| err(name, MacroErrorKind::TooManyMacros))?;
            out.secret.push(false).map_err(|_| err(name, MacroErrorKind::TooManyMacros))?;
            continue;
        }
        // delay = N (antes das macros)
        if out.names.is_empty()
            && let Some((k, v)) = code.split_once('=')
            && k.trim() == "delay"
        {
            let v = v.trim();
            out.delay_ms = v.parse().ok().filter(|d| *d <= 1000).ok_or(err(v, MacroErrorKind::BadDelay))?;
            continue;
        }

        let in_macro = !out.names.is_empty();
        let push =
            |s: Step, out: &mut ParsedMacros<'_>| out.steps.push(s.encode()).map_err(|_| MacroErrorKind::TooManySteps);
        line_tokens(raw, |tok| {
            if !in_macro {
                return Err(MacroErrorKind::StepOutsideMacro);
            }
            match tok {
                Tok::Text(text) => {
                    let mut chars = text.chars();
                    while let Some(c) = chars.next() {
                        let c = if c == '\\' {
                            match chars.next() {
                                Some('n') => '\n',
                                Some('t') => '\t',
                                Some('"') => '"',
                                Some('\'') => '\'',
                                Some('\\') => '\\',
                                _ => return Err(MacroErrorKind::BadEscape),
                            }
                        } else {
                            c
                        };
                        let keys = type_char(host, c).map_err(|_| MacroErrorKind::BadChar(c))?;
                        for (m, k) in keys {
                            push(Step::Tap(m, k), &mut out)?;
                        }
                    }
                }
                Tok::Word(w) => {
                    if let Some(name) = w.strip_prefix("ENV(").and_then(|r| r.strip_suffix(')')) {
                        // Segredo: digita o valor sem nunca mostrá-lo em mensagens.
                        let name = name.trim();
                        if !valid_env_name(name) {
                            return Err(MacroErrorKind::BadEnvName);
                        }
                        let value = env(name).ok_or(MacroErrorKind::MissingEnv)?;
                        for c in value.chars() {
                            let keys = type_char(host, c).map_err(|_| MacroErrorKind::EnvBadChar)?;
                            for (m, k) in keys {
                                push(Step::Tap(m, k), &mut out)?;
                            }
                        }
                        if let Some(s) = out.secret.last_mut() {
                            *s = true;
                        }
                        if !out.env_names.contains(&name) {
                            let _ = out.env_names.push(name);
                        }
                        return Ok(());
                    }
                    let step = word_step(w, host)?;
                    push(step, &mut out)?;
                }
            }
            Ok(())
        })
        .map_err(|(k, tok)| err(tok, k))?;
    }
    close_macro(&mut out).map_err(|k| MacroError {
        line: src.lines().count(),
        token: out.names.last().copied().unwrap_or(""),
        kind: k,
    })?;
    Ok(out)
}

/// Fecha a macro atual (grava o tamanho; vazia é erro).
fn close_macro(out: &mut ParsedMacros<'_>) -> Result<(), MacroErrorKind> {
    if let Some(span) = out.spans.last_mut() {
        span.1 = out.steps.len() as u16 - span.0;
        if span.1 == 0 {
            return Err(MacroErrorKind::EmptyMacro);
        }
    }
    Ok(())
}

/// Tecla sem modificador (para DOWN/UP).
fn plain_key(s: &str, host: Host) -> Result<Keycode, MacroErrorKind> {
    match Action::parse_with(s, host, &|_| None) {
        Ok(Action::Key(k)) => Ok(k),
        Ok(_) => Err(MacroErrorKind::NotAKey),
        Err(e) => Err(MacroErrorKind::Action(e)),
    }
}

fn word_step(w: &str, host: Host) -> Result<Step, MacroErrorKind> {
    let call = |name: &str| w.strip_prefix(name).and_then(|r| r.strip_prefix('(')).and_then(|r| r.strip_suffix(')'));
    if let Some(a) = call("WAIT") {
        let ms = a.trim().parse::<u16>().ok().filter(|ms| *ms <= MAX_WAIT_MS).ok_or(MacroErrorKind::BadWait)?;
        return Ok(Step::Wait(ms));
    }
    if let Some(a) = call("DOWN") {
        return plain_key(a.trim(), host).map(Step::Down);
    }
    if let Some(a) = call("UP") {
        return plain_key(a.trim(), host).map(Step::Up);
    }
    match Action::parse_with(w, host, &|_| None) {
        Ok(Action::Key(k)) => Ok(Step::Tap(Mods::NONE, k)),
        Ok(Action::ModKey(m, k)) => Ok(Step::Tap(m, k)),
        Ok(_) => Err(MacroErrorKind::NotAKey),
        Err(ParseError::UnknownMacro) => Err(MacroErrorKind::NotAKey),
        Err(e) => Err(MacroErrorKind::Action(e)),
    }
}
