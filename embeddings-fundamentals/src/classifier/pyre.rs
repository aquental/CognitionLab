//! Regexes with the semantics of Python 3.14's `re` (spec/CLASSIFIER.md §6.3, §6.4).
//!
//! Each pattern is kept as its Python text and translated to `fancy-regex`. The
//! translation covers the subset of `re` syntax that TestoDoc's masking uses, and refuses
//! anything else:
//! - `\w`, `\d`, `\s` and their negations become explicit classes generated from Python
//!   (`pyclasses`); the regex engine's own classes are never used;
//! - `\b` becomes lookarounds over Python's `\w`;
//! - without MULTILINE, `$` matches at the end or before a final `\n`, as in Python;
//! - under IGNORECASE, `İ` (U+0130) and `ı` (U+0131) match `i`/`I`, in literals and in
//!   classes, which Rust's case folding does not do.
//!
//! Positions are byte offsets in the string searched. `pos` does not cut the string, so
//! lookbehind and `\b` see before it; `endpos` does, so nothing after it is seen.

use std::fmt::Write as _;

use fancy_regex::{Captures, RegexBuilder, RegexInput};

use super::pyclasses;

/// The flags of a Python pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    /// `re.IGNORECASE`.
    pub ignorecase: bool,
    /// `re.MULTILINE`.
    pub multiline: bool,
}

impl Flags {
    /// No flags.
    pub const NENHUMA: Self = Self {
        ignorecase: false,
        multiline: false,
    };
    /// `re.IGNORECASE`.
    pub const IGNORECASE: Self = Self {
        ignorecase: true,
        multiline: false,
    };
    /// `re.MULTILINE`.
    pub const MULTILINE: Self = Self {
        ignorecase: false,
        multiline: true,
    };
}

/// A failure to translate, compile or run a pattern.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ErroPyre {
    /// The Python text uses syntax the translation does not cover.
    #[error("unsupported regex syntax at code point {posicao}: {motivo}")]
    NaoSuportado {
        /// Code-point offset in the Python text.
        posicao: usize,
        /// What is not supported.
        motivo: &'static str,
    },
    /// The translated pattern did not compile, or a search failed (backtrack limit).
    #[error("regex engine error")]
    Motor(#[source] Box<fancy_regex::Error>),
}

impl From<fancy_regex::Error> for ErroPyre {
    fn from(erro: fancy_regex::Error) -> Self {
        Self::Motor(Box::new(erro))
    }
}

/// Upper bound on the compiled size of one delegated pattern: the generated classes
/// have hundreds of ranges.
const LIMITE_TAMANHO: usize = 64 << 20;

/// A compiled Python pattern.
#[derive(Debug)]
pub struct PyRegex {
    regex: fancy_regex::Regex,
}

impl PyRegex {
    /// Translates and compiles `python` with `flags`, for `search`, `match`, `finditer`
    /// and `sub`.
    ///
    /// # Errors
    ///
    /// [`ErroPyre::NaoSuportado`] for syntax outside the covered subset, and
    /// [`ErroPyre::Motor`] if the translation does not compile.
    pub fn new(python: &str, flags: Flags) -> Result<Self, ErroPyre> {
        Self::compilar(traduzir(python, flags)?)
    }

    /// Like [`PyRegex::new`], for Python's `fullmatch`: the whole string must match,
    /// trying every alternative, as `(?:python)\Z` does.
    ///
    /// # Errors
    ///
    /// As [`PyRegex::new`].
    pub fn new_fullmatch(python: &str, flags: Flags) -> Result<Self, ErroPyre> {
        Self::compilar(format!(r"\A(?:{})\z", traduzir(python, flags)?))
    }

    fn compilar(padrao: String) -> Result<Self, ErroPyre> {
        // Python's `re` has no step limit, and some TestoDoc patterns cost a quadratic
        // number of steps on long whitespace runs (`pdftotext -layout` pads columns with
        // them): with `fancy-regex`'s default of 1 000 000, a 3 000-space page already
        // failed. A search here takes as long as Python's would.
        let regex = RegexBuilder::new(&padrao)
            .delegate_size_limit(LIMITE_TAMANHO)
            .backtrack_limit(usize::MAX)
            .build()?;
        Ok(Self { regex })
    }

    /// `pattern.search(s, pos, endpos)`, in bytes: the first match that starts at or
    /// after `pos` and ends at or before `endpos`. Lookbehind sees before `pos`; nothing
    /// sees after `endpos`.
    ///
    /// # Errors
    ///
    /// [`ErroPyre::Motor`] if the search fails.
    pub fn search(
        &self,
        s: &str,
        pos: usize,
        endpos: usize,
    ) -> Result<Option<(usize, usize)>, ErroPyre> {
        let s = s.get(..endpos.min(s.len())).unwrap_or(s);
        let achado = self.regex.find_from_pos(s, pos.min(s.len()))?;
        Ok(achado.map(|m| (m.start(), m.end())))
    }

    /// `pattern.match(s, pos)`, in bytes: a match that starts exactly at `pos`.
    ///
    /// # Errors
    ///
    /// [`ErroPyre::Motor`] if the search fails.
    pub fn match_at(&self, s: &str, pos: usize) -> Result<Option<(usize, usize)>, ErroPyre> {
        let entrada = RegexInput::new(s).from_pos(pos.min(s.len())).anchored(true);
        let achado = self.regex.find_input(entrada)?;
        Ok(achado.map(|m| (m.start(), m.end())))
    }

    /// `pattern.fullmatch(s)` for a pattern built with [`PyRegex::new_fullmatch`].
    ///
    /// # Errors
    ///
    /// [`ErroPyre::Motor`] if the search fails.
    pub fn fullmatch(&self, s: &str) -> Result<bool, ErroPyre> {
        Ok(self.regex.is_match(s)?)
    }

    /// `pattern.finditer(s)`: the matches with their groups, left to right.
    pub fn finditer<'r, 't>(
        &'r self,
        s: &'t str,
    ) -> impl Iterator<Item = Result<Captures<'t, str>, ErroPyre>> + use<'r, 't> {
        self.regex
            .captures_iter(s)
            .map(|c| c.map_err(ErroPyre::from))
    }

    /// `pattern.sub(novo, s)` with a literal replacement (no group references).
    ///
    /// # Errors
    ///
    /// [`ErroPyre::Motor`] if a search fails.
    pub fn sub(&self, s: &str, novo: &str) -> Result<String, ErroPyre> {
        let mut saida = String::with_capacity(s.len());
        let mut pos = 0;
        for achado in self.regex.find_iter(s) {
            let achado = achado?;
            saida.push_str(s.get(pos..achado.start()).unwrap_or_default());
            saida.push_str(novo);
            pos = achado.end();
        }
        saida.push_str(s.get(pos..).unwrap_or_default());
        Ok(saida)
    }
}

/// `re.escape`: a backslash before each of `()[]{}?*+-|^$\.&~#`, space, `\t`, `\n`,
/// `\r`, `\v` and `\f` (Python 3.7 and later).
#[must_use]
pub fn re_escape(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for ch in texto.chars() {
        if "()[]{}?*+-|^$\\.&~# \t\n\r\u{b}\u{c}".contains(ch) {
            saida.push('\\');
        }
        saida.push(ch);
    }
    saida
}

/// The body of a class (without brackets) with the ranges of `tabela`.
fn corpo(tabela: &[(u32, u32)]) -> String {
    let mut saida = String::with_capacity(tabela.len() * 20);
    for &(inicio, fim) in tabela {
        if inicio == fim {
            let _ = write!(saida, r"\x{{{inicio:x}}}");
        } else {
            let _ = write!(saida, r"\x{{{inicio:x}}}-\x{{{fim:x}}}");
        }
    }
    saida
}

/// A class shorthand of Python's `re`.
#[derive(Debug, Clone, Copy)]
enum Atalho {
    Palavra,
    Digito,
    Espaco,
}

impl Atalho {
    const fn de(letra: char) -> Option<(Self, bool)> {
        match letra {
            'w' => Some((Self::Palavra, false)),
            'W' => Some((Self::Palavra, true)),
            'd' => Some((Self::Digito, false)),
            'D' => Some((Self::Digito, true)),
            's' => Some((Self::Espaco, false)),
            'S' => Some((Self::Espaco, true)),
            _ => None,
        }
    }

    fn classe(self, negada: bool) -> String {
        let tabela = match self {
            Self::Palavra => pyclasses::RE_WORD,
            Self::Digito => pyclasses::RE_DIGIT,
            Self::Espaco => pyclasses::RE_SPACE,
        };
        format!("[{}{}]", if negada { "^" } else { "" }, corpo(tabela))
    }
}

/// `\b` over Python's `\w`.
fn fronteira() -> String {
    let w = Atalho::Palavra.classe(false);
    format!("(?-i:(?:(?<={w})(?!{w})|(?<!{w})(?={w})))")
}

fn literal(ch: char) -> String {
    // `,` stays as it is, so the one in `{m,n}` keeps its meaning.
    if ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '_' | ',') {
        ch.to_string()
    } else {
        format!(r"\x{{{:x}}}", u32::from(ch))
    }
}

/// `i` and `I` also match `İ` and `ı` under IGNORECASE (Python's `sre`).
const I_EXTRAS: [char; 2] = ['\u{130}', '\u{131}'];

struct Tradutor {
    chars: Vec<char>,
    pos: usize,
    saida: String,
    /// IGNORECASE of each open group, outermost first.
    pilha: Vec<bool>,
    ignorecase: bool,
    multiline: bool,
}

/// Translates a Python pattern to `fancy-regex` syntax.
///
/// # Errors
///
/// [`ErroPyre::NaoSuportado`] for syntax outside the covered subset.
pub fn traduzir(python: &str, flags: Flags) -> Result<String, ErroPyre> {
    let mut t = Tradutor {
        chars: python.chars().collect(),
        pos: 0,
        saida: String::with_capacity(python.len() * 4),
        pilha: Vec::new(),
        ignorecase: flags.ignorecase,
        multiline: flags.multiline,
    };
    if flags.multiline {
        t.saida.push_str("(?m)");
    }
    if flags.ignorecase {
        t.saida.push_str("(?i)");
    }
    t.traduzir()?;
    Ok(t.saida)
}

impl Tradutor {
    const fn erro(&self, motivo: &'static str) -> ErroPyre {
        ErroPyre::NaoSuportado {
            posicao: self.pos,
            motivo,
        }
    }

    fn atual(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn segue(&self, texto: &str) -> bool {
        texto
            .chars()
            .enumerate()
            .all(|(i, ch)| self.chars.get(self.pos + i) == Some(&ch))
    }

    fn traduzir(&mut self) -> Result<(), ErroPyre> {
        while let Some(ch) = self.atual() {
            match ch {
                '\\' => self.escape()?,
                '[' => self.classe()?,
                '(' => self.abrir_grupo()?,
                ')' => {
                    self.ignorecase = self.pilha.pop().ok_or_else(|| self.erro("unbalanced )"))?;
                    self.saida.push(')');
                    self.pos += 1;
                }
                '$' => {
                    self.saida
                        .push_str(if self.multiline { "$" } else { r"(?=\n?\z)" });
                    self.pos += 1;
                }
                '^' | '.' | '|' | '*' | '+' | '?' | '{' | '}' => {
                    self.saida.push(ch);
                    self.pos += 1;
                }
                _ => {
                    self.literal(ch);
                    self.pos += 1;
                }
            }
        }
        if self.pilha.is_empty() {
            Ok(())
        } else {
            Err(self.erro("unbalanced ("))
        }
    }

    fn literal(&mut self, ch: char) {
        if self.ignorecase && (ch == 'i' || ch == 'I') {
            let _ = write!(self.saida, "[iI{}]", I_EXTRAS.map(literal).concat());
        } else {
            self.saida.push_str(&literal(ch));
        }
    }

    fn escape(&mut self) -> Result<(), ErroPyre> {
        let letra = self
            .chars
            .get(self.pos + 1)
            .copied()
            .ok_or_else(|| self.erro("trailing backslash"))?;
        if let Some((atalho, negada)) = Atalho::de(letra) {
            let classe = atalho.classe(negada);
            if self.ignorecase {
                let _ = write!(self.saida, "(?-i:{classe})");
            } else {
                self.saida.push_str(&classe);
            }
        } else {
            match letra {
                'b' => self.saida.push_str(&fronteira()),
                'A' => self.saida.push_str(r"\A"),
                'Z' => self.saida.push_str(r"\z"),
                't' => self.saida.push_str(r"\t"),
                'n' => self.saida.push_str(r"\n"),
                'r' => self.saida.push_str(r"\r"),
                'v' => self.saida.push_str(r"\x{b}"),
                'f' => self.saida.push_str(r"\x{c}"),
                c if c.is_ascii_alphanumeric() => return Err(self.erro("unsupported escape")),
                c => self.literal(c),
            }
        }
        self.pos += 2;
        Ok(())
    }

    fn abrir_grupo(&mut self) -> Result<(), ErroPyre> {
        self.pilha.push(self.ignorecase);
        self.pos += 1;
        if self.atual() != Some('?') {
            self.saida.push('(');
            return Ok(());
        }
        for (python, rust, ignorecase) in [
            ("?:", "(?:", None),
            ("?=", "(?=", None),
            ("?!", "(?!", None),
            ("?<=", "(?<=", None),
            ("?<!", "(?<!", None),
            ("?i:", "(?i:", Some(true)),
            ("?-i:", "(?-i:", Some(false)),
        ] {
            if self.segue(python) {
                self.saida.push_str(rust);
                self.pos += python.chars().count();
                if let Some(ignorecase) = ignorecase {
                    self.ignorecase = ignorecase;
                }
                return Ok(());
            }
        }
        if self.segue("?P<") {
            let inicio = self.pos + 3;
            let fim = self
                .chars
                .get(inicio..)
                .and_then(|resto| resto.iter().position(|&c| c == '>'))
                .ok_or_else(|| self.erro("unterminated group name"))?;
            let nome: String = self
                .chars
                .get(inicio..inicio + fim)
                .unwrap_or_default()
                .iter()
                .collect();
            if nome.is_empty() || !nome.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(self.erro("invalid group name"));
            }
            let _ = write!(self.saida, "(?P<{nome}>");
            self.pos = inicio + fim + 1;
            return Ok(());
        }
        Err(self.erro("unsupported group"))
    }

    /// One class `[...]`, from the `[` to its `]`.
    fn classe(&mut self) -> Result<(), ErroPyre> {
        self.pos += 1;
        let negada = self.atual() == Some('^');
        if negada {
            self.pos += 1;
        }
        let mut itens = String::new();
        let mut tem_i = false;
        let mut primeiro = true;
        loop {
            let ch = self
                .atual()
                .ok_or_else(|| self.erro("unterminated character class"))?;
            if ch == ']' && !primeiro {
                self.pos += 1;
                break;
            }
            primeiro = false;
            let inicio = if ch == '\\' {
                let letra = self
                    .chars
                    .get(self.pos + 1)
                    .copied()
                    .ok_or_else(|| self.erro("trailing backslash"))?;
                if let Some((atalho, negado)) = Atalho::de(letra) {
                    itens.push_str(&atalho.classe(negado));
                    self.pos += 2;
                    continue;
                }
                self.pos += 2;
                match letra {
                    't' => '\t',
                    'n' => '\n',
                    'r' => '\r',
                    'v' => '\u{b}',
                    'f' => '\u{c}',
                    c if c.is_ascii_alphanumeric() => {
                        return Err(self.erro("unsupported escape in class"));
                    }
                    c => c,
                }
            } else {
                self.pos += 1;
                ch
            };
            let e_faixa = self.atual() == Some('-')
                && self.chars.get(self.pos + 1).is_some_and(|&c| c != ']');
            let fim = if e_faixa {
                self.pos += 1;
                let ch = self
                    .atual()
                    .ok_or_else(|| self.erro("unterminated range"))?;
                self.pos += 1;
                if ch == '\\' {
                    let letra = self
                        .atual()
                        .ok_or_else(|| self.erro("trailing backslash"))?;
                    if letra.is_ascii_alphanumeric() {
                        return Err(self.erro("unsupported escape in range"));
                    }
                    self.pos += 1;
                    letra
                } else {
                    ch
                }
            } else {
                inicio
            };
            if fim < inicio {
                return Err(self.erro("bad range"));
            }
            tem_i |= (inicio..=fim).contains(&'i') || (inicio..=fim).contains(&'I');
            let _ = write!(
                itens,
                r"\x{{{:x}}}-\x{{{:x}}}",
                u32::from(inicio),
                u32::from(fim)
            );
        }
        if self.ignorecase && tem_i {
            for extra in I_EXTRAS {
                itens.push_str(&literal(extra));
            }
        }
        let _ = write!(self.saida, "[{}{itens}]", if negada { "^" } else { "" });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn re(python: &str, flags: Flags) -> PyRegex {
        PyRegex::new(python, flags).unwrap()
    }

    fn inteiro(python: &str, flags: Flags, s: &str) -> bool {
        PyRegex::new_fullmatch(python, flags)
            .unwrap()
            .fullmatch(s)
            .unwrap()
    }

    #[test]
    fn word_excludes_marks_and_connector_punctuation() {
        // Rust's `\w` matches both (spec §6.3).
        assert!(inteiro(r"\w+", Flags::NENHUMA, "aé_1"));
        assert!(!inteiro(r"\w+", Flags::NENHUMA, "a\u{301}"));
        assert!(!inteiro(r"\w+", Flags::NENHUMA, "a\u{203f}"));
        assert!(inteiro(r"[^\W\d_]+", Flags::NENHUMA, "Ana²½Ⅻ"));
        assert!(!inteiro(r"[^\W\d_]+", Flags::NENHUMA, "a1"));
        assert!(!inteiro(r"[^\W\d_]+", Flags::NENHUMA, "a_"));
    }

    #[test]
    fn space_and_digit_are_pythons() {
        assert!(inteiro(r"a\sb", Flags::NENHUMA, "a\u{1f}b"));
        assert!(!inteiro(r"a\Sb", Flags::NENHUMA, "a\u{1f}b"));
        assert!(inteiro(r"\d+", Flags::NENHUMA, "12٣"));
        assert!(!inteiro(r"\d", Flags::NENHUMA, "²"));
    }

    #[test]
    fn ignorecase_matches_dotted_and_dotless_i() {
        // Python 3.14: re.match('(?i)paciente', 'PACİENTE') and 'pacıente' are true.
        let paciente = re("paciente", Flags::IGNORECASE);
        for s in ["PACİENTE", "pacıente", "Paciente", "PACIENTE"] {
            assert_eq!(
                paciente.match_at(s, 0).unwrap().map(|m| m.0),
                Some(0),
                "{s}"
            );
        }
        assert!(inteiro("[A-Za-z]{2}", Flags::IGNORECASE, "İı"));
        assert!(inteiro("(?i:i)", Flags::NENHUMA, "İ"));
        assert!(!inteiro("i", Flags::NENHUMA, "İ"));
        assert!(!inteiro("(?-i:[A-Z]{2})", Flags::IGNORECASE, "İI"));
        assert!(
            !inteiro("[A-H]", Flags::IGNORECASE, "ı"),
            "the range has no i"
        );
        // Both engines already match these.
        assert!(inteiro("sk", Flags::IGNORECASE, "\u{17f}\u{212a}"));
    }

    #[test]
    fn dollar_without_multiline_also_matches_before_a_final_newline() {
        let r = re("c$", Flags::NENHUMA);
        assert_eq!(r.search("abc\n", 0, 4).unwrap(), Some((2, 3)));
        assert_eq!(r.search("abc\nX", 0, 5).unwrap(), None);
        assert_eq!(r.search("abc\nX", 0, 4).unwrap(), Some((2, 3)), "endpos");
        assert_eq!(r.search("abc\n\n", 0, 5).unwrap(), None);
    }

    #[test]
    fn pos_does_not_cut_the_string_but_endpos_does() {
        assert_eq!(
            re("(?<=b)a", Flags::NENHUMA).search("ba", 1, 2).unwrap(),
            Some((1, 2))
        );
        assert_eq!(re(r"\ba", Flags::NENHUMA).search("ba", 1, 2).unwrap(), None);
        assert_eq!(
            re(r"a\b", Flags::NENHUMA).search("ab", 0, 1).unwrap(),
            Some((0, 1))
        );
        assert_eq!(
            re("a(?=b)", Flags::NENHUMA).search("ab", 0, 1).unwrap(),
            None
        );
    }

    #[test]
    fn match_is_anchored_at_pos() {
        let r = re("a", Flags::NENHUMA);
        assert_eq!(r.match_at("xa", 0).unwrap(), None);
        assert_eq!(r.match_at("xa", 1).unwrap(), Some((1, 2)));
        let linha = re("^a", Flags::MULTILINE);
        assert_eq!(linha.match_at("ba", 1).unwrap(), None);
        assert_eq!(linha.match_at("b\na", 2).unwrap(), Some((2, 3)));
        assert_eq!(re("^a", Flags::NENHUMA).match_at("b\na", 2).unwrap(), None);
    }

    #[test]
    fn fullmatch_tries_every_alternative() {
        assert!(inteiro("a|ab", Flags::NENHUMA, "ab"));
    }

    #[test]
    fn escape_is_pythons_and_round_trips() {
        assert_eq!(re_escape("a.b c-d"), r"a\.b\ c\-d");
        for s in ["nome do", "a.b(c)[d]{e}?*+|^$&~#", "tab\there", "çã"] {
            assert!(inteiro(&re_escape(s), Flags::NENHUMA, s), "{s:?}");
        }
    }

    #[test]
    fn named_groups_and_sub() {
        let r = re(r"(?P<sigla>CRM)[ \t]*(?P<num>\d+)", Flags::NENHUMA);
        let c = r.finditer("x CRM 1234").next().unwrap().unwrap();
        assert_eq!(c.name("sigla").unwrap().as_str(), "CRM");
        assert_eq!(c.name("num").unwrap().range(), 6..10);
        assert_eq!(
            re(r"[ \t]*\([^)]*\)", Flags::NENHUMA)
                .sub("Altura (cm) x", "")
                .unwrap(),
            "Altura x"
        );
    }

    #[test]
    fn unsupported_syntax_is_refused() {
        for python in [r"\1", r"(?P=a)", r"\p{L}", "(a", "a)", "[a", r"\B"] {
            assert!(
                matches!(
                    traduzir(python, Flags::NENHUMA),
                    Err(ErroPyre::NaoSuportado { .. })
                ),
                "{python}"
            );
        }
    }
}
