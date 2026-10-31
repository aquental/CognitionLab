//! Text primitives with Python 3.14's semantics on Unicode 16.0.0 (spec/CLASSIFIER.md §6).
//!
//! Positions and lengths in the classifier are in code points, as in Python. Character
//! classes and case mappings come from `pyclasses`, which is generated from Python
//! itself; Rust's own `char::is_*` and the regex engine's `\w`, `\d`, `\s` use other
//! definitions and are never used for these decisions.

use unicode_normalization::UnicodeNormalization;

use super::pyclasses;

fn em(tabela: &[(u32, u32)], ch: char) -> bool {
    let c = u32::from(ch);
    let i = tabela.partition_point(|&(_, fim)| fim < c);
    tabela.get(i).is_some_and(|&(inicio, _)| inicio <= c)
}

fn mapeado(tabela: &'static [(u32, &'static str)], ch: char) -> Option<&'static str> {
    let c = u32::from(ch);
    tabela
        .binary_search_by_key(&c, |&(ponto, _)| ponto)
        .ok()
        .and_then(|i| tabela.get(i))
        .map(|&(_, resultado)| resultado)
}

/// `\d` of Python's `re`.
#[must_use]
pub fn re_digito(ch: char) -> bool {
    em(pyclasses::RE_DIGIT, ch)
}

/// `\w` of Python's `re`.
#[must_use]
pub fn re_palavra(ch: char) -> bool {
    em(pyclasses::RE_WORD, ch)
}

/// `\s` of Python's `re`.
#[must_use]
pub fn re_espaco(ch: char) -> bool {
    em(pyclasses::RE_SPACE, ch)
}

/// `[^\W\d_]` of Python's `re`: letters, plus numeric characters that are not decimal
/// digits (`²`, `½`, `Ⅻ`).
#[must_use]
pub fn re_letra(ch: char) -> bool {
    em(pyclasses::RE_LETTER, ch)
}

/// `[^\W_]` of Python's `re`.
#[must_use]
pub fn re_alnum(ch: char) -> bool {
    em(pyclasses::RE_ALNUM, ch)
}

/// `str.isupper()` of one code point.
#[must_use]
pub fn is_upper(ch: char) -> bool {
    em(pyclasses::IS_UPPER, ch)
}

/// `str.islower()` of one code point.
#[must_use]
pub fn is_lower(ch: char) -> bool {
    em(pyclasses::IS_LOWER, ch)
}

/// A titlecase letter (category Lt), the flag `str.isupper()` of a string tests. Not
/// `str.istitle()` of one code point, which is also true for uppercase letters.
#[must_use]
pub fn is_title(ch: char) -> bool {
    em(pyclasses::IS_TITLE, ch)
}

/// The Unicode Cased property, as CPython's `str.title()` uses it.
#[must_use]
pub fn is_cased(ch: char) -> bool {
    em(pyclasses::IS_CASED, ch)
}

/// `str.isdigit()` of one code point (`'²'` is a digit, `'½'` is not).
#[must_use]
pub fn is_digit(ch: char) -> bool {
    em(pyclasses::IS_DIGIT, ch)
}

/// `str.isspace()` of one code point; includes U+001C to U+001F.
#[must_use]
pub fn is_space(ch: char) -> bool {
    em(pyclasses::IS_SPACE, ch)
}

fn push_mapeado(saida: &mut String, tabela: &'static [(u32, &'static str)], ch: char) {
    match mapeado(tabela, ch) {
        Some(m) => saida.push_str(m),
        None => saida.push(ch),
    }
}

/// `str.lower()`.
#[must_use]
pub fn lower(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    for ch in s.chars() {
        push_mapeado(&mut saida, pyclasses::LOWER, ch);
    }
    saida
}

/// `str.upper()`; the length can change (`ß` becomes `SS`).
#[must_use]
pub fn upper(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    for ch in s.chars() {
        push_mapeado(&mut saida, pyclasses::UPPER, ch);
    }
    saida
}

/// `str.title()`, as CPython's `do_title`: after a cased code point the next one is
/// lowered, otherwise it is title-cased.
#[must_use]
pub fn title(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    let mut anterior_tem_caixa = false;
    for ch in s.chars() {
        let tabela = if anterior_tem_caixa {
            pyclasses::LOWER
        } else {
            pyclasses::TITLE
        };
        push_mapeado(&mut saida, tabela, ch);
        anterior_tem_caixa = is_cased(ch);
    }
    saida
}

/// `str.isupper()` of a string, as CPython: no lowercase or titlecase code point and at
/// least one uppercase one.
#[must_use]
pub fn str_isupper(s: &str) -> bool {
    let mut tem_maiuscula = false;
    for ch in s.chars() {
        if is_lower(ch) || is_title(ch) {
            return false;
        }
        tem_maiuscula |= is_upper(ch);
    }
    tem_maiuscula
}

/// `str.strip()`: drops `isspace` code points at both ends.
#[must_use]
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `str.split()`: the runs of non-`isspace` code points.
pub fn split(s: &str) -> impl Iterator<Item = &str> {
    s.split(is_space).filter(|parte| !parte.is_empty())
}

/// Number of code points: Python's `len` of a `str`.
#[must_use]
pub fn len(s: &str) -> usize {
    s.chars().count()
}

/// The first `n` code points of `s` (Python's `s[:n]`).
#[must_use]
pub fn prefixo(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(byte, _)| &s[..byte])
}

/// NFC, then every line break that `str.splitlines()` knows becomes `\n`:
/// `\r\n`, `\r`, `\x0b`, `\x0c`, `\x1c`, `\x1d`, `\x1e`, `\x85`, U+2028 and U+2029.
/// `\x1f` is not a line break for `splitlines()` and stays (TD §6, step 0).
#[must_use]
pub fn compor(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut chars = texto.nfc().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                chars.next_if_eq(&'\n');
                saida.push('\n');
            }
            '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => saida.push('\n'),
            _ => saida.push(ch),
        }
    }
    saida
}

/// Folds one code point: the first code point of its canonical decomposition (NFD,
/// never NFKD), lowered when the lowercase form is a single code point.
#[must_use]
pub fn dobrar_char(ch: char) -> char {
    if ch.is_ascii() {
        return ch.to_ascii_lowercase();
    }
    let base = std::iter::once(ch).nfd().next().unwrap_or(ch);
    match mapeado(pyclasses::LOWER, base) {
        None => base,
        Some(baixa) => {
            let mut chars = baixa.chars();
            match (chars.next(), chars.next()) {
                (Some(unica), None) => unica,
                _ => base,
            }
        }
    }
}

/// Lowercase without accents, with the same number of code points, so positions found
/// in the folded text are valid in the original (TD §7.3, "Primitivas"). Does not
/// compose first: a loose combining mark stays.
#[must_use]
pub fn dobrar(texto: &str) -> String {
    texto.chars().map(dobrar_char).collect()
}

/// Maps byte offsets of a string to code-point offsets and back.
#[derive(Debug, Clone)]
pub struct MapaOffsets {
    /// Byte offset of each code point, plus the string's length at the end.
    inicios: Vec<usize>,
}

impl MapaOffsets {
    /// The map of `s`.
    #[must_use]
    pub fn new(s: &str) -> Self {
        let mut inicios: Vec<usize> = s.char_indices().map(|(byte, _)| byte).collect();
        inicios.push(s.len());
        Self { inicios }
    }

    /// Code-point offset of a byte offset at a code-point boundary, or `None`.
    #[must_use]
    pub fn char_de_byte(&self, byte: usize) -> Option<usize> {
        self.inicios.binary_search(&byte).ok()
    }

    /// Byte offset of a code-point offset, or `None` past the end.
    #[must_use]
    pub fn byte_de_char(&self, ch: usize) -> Option<usize> {
        self.inicios.get(ch).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn the_tables_are_python_314_on_unicode_16() {
        assert_eq!(pyclasses::UNIDATA_VERSION, super::super::UNICODE_VERSAO);
        assert!(pyclasses::PYTHON_VERSION.starts_with("3.14."));
    }

    #[test]
    fn python_word_class_excludes_marks_and_connector_punctuation() {
        // Rust's regex `\w` includes both (verified with regex 1.13.1, spec §6.3).
        assert!(re_palavra('a'));
        assert!(re_palavra('_'));
        assert!(!re_palavra('\u{301}'), "combining acute");
        assert!(!re_palavra('\u{203f}'), "undertie");
    }

    #[test]
    fn python_letter_class_includes_non_decimal_numbers() {
        for ch in ['²', '½', 'Ⅻ', 'é', 'ß'] {
            assert!(re_letra(ch), "{ch:?}");
        }
        for ch in ['1', '٣', '_', ' ', '\u{301}'] {
            assert!(!re_letra(ch), "{ch:?}");
        }
        assert!(is_digit('²') && !is_digit('½'));
    }

    #[test]
    fn python_space_includes_the_information_separators() {
        for ch in [
            '\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}', '\u{85}', '\u{a0}', '\u{2028}',
        ] {
            assert!(is_space(ch) && re_espaco(ch), "{ch:?}");
        }
        assert!(!is_space('\u{200b}'), "zero width space is not space");
    }

    #[test]
    fn case_follows_python() {
        // `ch.isupper()` of the initial only (spec §6.4): "João"[:1].isupper() is true.
        assert!(is_upper('J') && !is_upper('o'));
        assert!(!str_isupper("João"));
        assert!(str_isupper("JOÃO DA SILVA"));
        assert!(!str_isupper("123 -"), "no cased character");
        assert!(!str_isupper("ǅ"), "titlecase is not upper");
        assert!(
            is_title('ǅ') && !is_title('A'),
            "Lt only, unlike \"A\".istitle()"
        );
        assert_eq!(title("JOÃO DA SILVA"), "João Da Silva");
        assert_eq!(title("ǆ"), "ǅ");
        assert_eq!(
            title("ªb"),
            "ªb",
            "ª is cased (Other_Lowercase), so b is lowered"
        );
        assert_eq!(upper("ßa"), "SSA");
        assert_eq!(lower("İ"), "i\u{307}");
    }

    #[test]
    fn a_code_point_new_in_unicode_17_is_unassigned() {
        // U+A7CE is an uppercase letter from Unicode 17 on. Python 3.14 (UCD 16) sees an
        // unassigned code point: no case, not a letter, not `\w` (spec §4.2, §14.4).
        let ch = '\u{a7ce}';
        assert!(!is_upper(ch) && !is_lower(ch) && !is_cased(ch));
        assert!(!re_palavra(ch) && !re_letra(ch) && !re_alnum(ch));
        assert_eq!(lower("\u{a7ce}"), "\u{a7ce}");
        assert_eq!(dobrar("\u{a7ce}"), "\u{a7ce}");
        assert!(str_isupper("X\u{a7ce}"));
    }

    #[test]
    fn dobrar_keeps_the_length_and_uses_canonical_decomposition() {
        assert_eq!(dobrar("ÉçÃßİ"), "ecaßi");
        assert_eq!(dobrar("º½²"), "º½²", "NFKD would turn these into o, 1/2, 2");
        assert_eq!(dobrar("Joa\u{303}o"), "joa\u{303}o", "a loose mark stays");
        assert_eq!(dobrar("Ω Å"), "ω a");
    }

    #[test]
    fn compor_turns_every_splitlines_break_into_lf() {
        assert_eq!(
            compor("a\r\nb\rc\u{b}d\u{c}e\u{1c}f\u{1d}g\u{1e}h\u{85}i\u{2028}j\u{2029}k"),
            "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk"
        );
        assert_eq!(
            compor("a\u{1f}b"),
            "a\u{1f}b",
            "\\x1f is not a splitlines break"
        );
        assert_eq!(compor("\r\r\n"), "\n\n");
        assert_eq!(compor("Joa\u{303}o"), "João");
    }

    #[test]
    fn strip_and_split_use_python_whitespace() {
        assert_eq!(strip("\u{1f} a b\u{85}"), "a b");
        assert_eq!(split(" a\u{1c}b  c ").collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(prefixo("açúcar", 3), "açú");
        assert_eq!(prefixo("ab", 5), "ab");
        assert_eq!(len("Joa\u{303}o"), 5);
    }

    #[test]
    fn offsets_map_bytes_to_code_points() {
        let mapa = MapaOffsets::new("aÉb");
        assert_eq!(mapa.char_de_byte(0), Some(0));
        assert_eq!(mapa.char_de_byte(1), Some(1));
        assert_eq!(mapa.char_de_byte(2), None, "inside É");
        assert_eq!(mapa.char_de_byte(3), Some(2));
        assert_eq!(mapa.char_de_byte(4), Some(3));
        assert_eq!(mapa.byte_de_char(3), Some(4));
        assert_eq!(mapa.byte_de_char(4), None);
    }

    proptest! {
        #[test]
        fn dobrar_preserves_the_number_of_code_points(s in "\\PC*") {
            prop_assert_eq!(len(&dobrar(&s)), len(&s));
        }

        #[test]
        fn compor_is_idempotent(s in "\\PC*") {
            let uma = compor(&s);
            prop_assert_eq!(compor(&uma), uma);
        }
    }
}
