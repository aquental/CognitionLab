//! The masking's regexes: each one's Python text, built as `mascaramento.py` builds it,
//! and its translation, compiled once (spec/CLASSIFIER.md §6.4, "Inventário").
//!
//! The texts of the vectors' `regexes` section are compared with the file by
//! `tests/classifier_vectors.rs`; the semantics of the translation are compared with
//! Python by `tests/classifier_pyre.rs`.

use std::cmp::Reverse;
use std::sync::LazyLock;

use crate::classifier::listas::{PARTICULAS, ROTULOS, ROTULOS_COMPOSTOS};
use crate::classifier::pyre::{ErroPyre, Flags, PyRegex, re_escape};

/// `_NUMERO_ABREVIADO`: "nº", "n.", "no".
const NUMERO_ABREVIADO: &str = r"(?:n[º°o.]\.?[ \t]*)?";

/// `_NOME_PROPRIO`: a word of letters that is not itself a title.
const NOME_PROPRIO: &str =
    r"(?!(?i:dra|dr|sra|sr|profa|prof)(?![^\W\d_]))[^\W\d_]+(?:['’-][^\W\d_]+)*(?![\w'’-])";

/// `_UNIDADE_DEPOIS`: a unit after a number makes it a clinical range, not a phone.
const UNIDADE_DEPOIS: &str =
    r"(?![ \t]*(?:kcal|cal|ml|l/|mmhg|bpm|ms|w\b|watts|kg|mg|g\b|%|m/s|min|s\b))";

const MESES: &str =
    "janeiro|fevereiro|marco|abril|maio|junho|julho|agosto|setembro|outubro|novembro|dezembro";
const MESES_ABREVIADOS: &str = "jan|fev|mar|abr|mai|jun|jul|ago|set|out|nov|dez";

/// Python's `sorted(..., key=lambda r: (-len(r), r))`: longest first, then by code
/// point; repeats dropped (`set`).
fn ordenados<'a>(itens: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let mut v: Vec<&str> = itens.into_iter().collect();
    v.sort_by_key(|s| (Reverse(s.chars().count()), *s));
    v.dedup();
    v
}

/// `_ROTULO_RE`.
pub(crate) fn rotulo_re() -> String {
    let escapados: Vec<String> = ordenados(ROTULOS.iter().copied())
        .into_iter()
        .map(re_escape)
        .collect();
    format!(
        r"(?:^|(?<=[\t|;])|(?<=\s\s))[ \t]*(?P<rot>(?:{})(?:[ \t]+[a-z]+){{1,2}}|{})(?![a-z0-9])[ \t]*(?::|[ \t]-(?=[ \t]))[ \t]*",
        ROTULOS_COMPOSTOS.join("|"),
        escapados.join("|")
    )
}

/// `_TRATAMENTO_RE`.
pub(crate) fn tratamento_re() -> String {
    format!(
        r"\b(?i:dra|dr|sra|sr|profa|prof)(?:\((?i:a)\))?\.?[ \t]+(?P<nome>{NOME_PROPRIO}(?:[ \t]+(?:(?i:d[aeo]s?|e)[ \t]+)?{NOME_PROPRIO})*)"
    )
}

/// `_REGISTRO_RE`.
pub(crate) fn registro_re() -> String {
    format!(
        r"\b(?P<sigla>CRM|CRF|CREF|CRN|COREN|CRP|CRO|RQE)(?:[ \t]*[/-][ \t]*[A-Za-z]{{2}}\b)?[ \t]*[:/-]?[ \t]*{NUMERO_ABREVIADO}(?:\d{{1,3}}(?:\.\d{{3}})+|\d{{4,}})(?!\d|,\d)(?:-[A-Za-z]\b)?(?:[ \t]*[/-][ \t]*(?-i:[A-Z]{{2}})\b)?(?![ \t]*(?:m?l\b|%|mg\b|dl\b))"
    )
}

/// `_IDENTIFICADOR_ROTULADO`.
pub(crate) fn identificador_rotulado() -> String {
    format!(
        r"\b(?:cpf|rg|cns|cep|cnh|cartao[ \t]+sus)\b[ \t]*{NUMERO_ABREVIADO}[:#-]?[ \t]*(?P<num>\d(?:[. \t/-]?\d){{4,}}(?:-?[Xx]\b)?)"
    )
}

/// `_ANTES_DE_FAIXA`.
pub(crate) const ANTES_DE_FAIXA: &str = r"\b(?:vo2|vco2|carga|limiar|tmr|gasto|faixa|intervalo|entre|pico|previst[oa]|esperad[oa]|fc|frequencia|kcal|watts|referencia|valor(?:es)?)\b[^\n]{0,25}$";

/// `_ANTES_DE_CONTAGEM`.
pub(crate) const ANTES_DE_CONTAGEM: &str =
    r"\b(?:batimentos|qrs|complexos|extrassistoles|ectopi[ac]s?|eventos)\b[^\n\d]{0,20}$";

/// `_DEPOIS_DE_CONTAGEM`.
pub(crate) const DEPOIS_DE_CONTAGEM: &str = r"[ \t]*(?:batimentos|complexos|qrs)\b";

/// `_DATAS_NUMERICAS`.
pub(crate) const DATAS_NUMERICAS: &str =
    r"\b(?:\d{4}[-./]\d{1,2}[-./]\d{1,2}|\d{1,2}[/.-]\d{1,2}[/.-](?:\d{4}|\d{2})|\d{1,2}/\d{4})\b";

/// `_DATAS_EXTENSO`; it runs on the folded text.
pub(crate) fn datas_extenso() -> String {
    format!(
        r"\b(?:\d{{1,2}}(?:º|°|o)?[ \t]+de[ \t]+(?:{MESES})(?:[ \t]+de[ \t]+\d{{4}})?|(?:{MESES})[ \t]+(?:de[ \t]+)?\d{{4}}|\d{{1,2}}[/-](?:{MESES_ABREVIADOS})\.?[/-](?:\d{{4}}|\d{{2}})|\d{{1,2}}[ \t]+(?:de[ \t]+)?(?:{MESES_ABREVIADOS})\.?[ \t]+(?:de[ \t]+)?\d{{4}}|(?:{MESES_ABREVIADOS})[/-]\d{{4}})\b"
    )
}

/// `_SEQUENCIA_DE_MARCADORES`.
pub(crate) fn sequencia_de_marcadores() -> String {
    let mut particulas: Vec<&str> = PARTICULAS.to_vec();
    particulas.sort_unstable();
    format!(
        r"\[REMOVIDO\](?:[ \t]+(?:(?:{})[ \t]+)?\[REMOVIDO\])+",
        particulas.join("|")
    )
}

/// What a pattern of layer 4 needs besides the match (`_sempre`, `_nao_e_faixa`,
/// `_nao_e_contagem`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Filtro {
    Sempre,
    NaoEFaixa,
    NaoEContagem,
}

/// `_PADROES`, in order, with their flags and filters.
pub(crate) fn padroes() -> Vec<(String, Flags, Filtro)> {
    use Filtro::{NaoEContagem, NaoEFaixa, Sempre};
    vec![
        (
            r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+".to_owned(),
            Flags::NENHUMA,
            Sempre,
        ),
        (
            r"(?:https?://|www\.)\S+".to_owned(),
            Flags::IGNORECASE,
            Sempre,
        ),
        (
            r"\b\d{2}\.?\d{3}\.?\d{3}/?\d{4}-?\d{2}\b".to_owned(),
            Flags::NENHUMA,
            Sempre,
        ),
        (
            r"\b\d{3}\.?\d{3}\.?\d{3}[-. \t]?\d{2}\b".to_owned(),
            Flags::NENHUMA,
            Sempre,
        ),
        (
            r"\b\d{3}[ \t]\d{3}[ \t]\d{3}[ \t]?-[ \t]?\d{2}\b".to_owned(),
            Flags::NENHUMA,
            Sempre,
        ),
        (
            r"\b\d{3}[. \t]?\d{4}[. \t]?\d{4}[. \t]?\d{4}\b".to_owned(),
            Flags::NENHUMA,
            Sempre,
        ),
        (
            r"\b\d{1,2}\.\d{3}\.\d{3}(?:-?[\dXx])?\b".to_owned(),
            Flags::NENHUMA,
            Sempre,
        ),
        (
            r"\b\d{2}\.?\d{3}-\d{3}\b".to_owned(),
            Flags::NENHUMA,
            Sempre,
        ),
        (
            r"\(\d{2}\)[ \t]*(?:9[ \t.]?)?\d{4}[- \t.]?\d{4}\b".to_owned(),
            Flags::NENHUMA,
            Sempre,
        ),
        (
            format!(r"\b\d{{2}}[ \t]+(?:9[ \t.]?)?\d{{4}}-\d{{4}}\b{UNIDADE_DEPOIS}"),
            Flags::IGNORECASE,
            Sempre,
        ),
        (
            format!(r"\b(?:9[ \t]?\d{{4}}|[2-5]\d{{3}})-\d{{4}}\b{UNIDADE_DEPOIS}"),
            Flags::IGNORECASE,
            NaoEFaixa,
        ),
        (r"\b\d{6,}\b".to_owned(), Flags::NENHUMA, NaoEContagem),
    ]
}

/// A Python text of the vectors' `regexes` section: one pattern, or `_PADROES`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextoPython {
    /// One pattern.
    Um(String),
    /// `_PADROES`, in order.
    Lista(Vec<String>),
}

/// The Python texts of the vectors' `regexes` section, in the file's order.
#[must_use]
pub fn textos_python() -> Vec<(&'static str, TextoPython)> {
    use TextoPython::{Lista, Um};
    vec![
        ("_ROTULO_RE", Um(rotulo_re())),
        ("_TRATAMENTO_RE", Um(tratamento_re())),
        ("_REGISTRO_RE", Um(registro_re())),
        ("_IDENTIFICADOR_ROTULADO", Um(identificador_rotulado())),
        ("_ANTES_DE_FAIXA", Um(ANTES_DE_FAIXA.to_owned())),
        ("_ANTES_DE_CONTAGEM", Um(ANTES_DE_CONTAGEM.to_owned())),
        ("_DEPOIS_DE_CONTAGEM", Um(DEPOIS_DE_CONTAGEM.to_owned())),
        ("_DATAS_NUMERICAS", Um(DATAS_NUMERICAS.to_owned())),
        ("_DATAS_EXTENSO", Um(datas_extenso())),
        ("_SEQUENCIA_DE_MARCADORES", Um(sequencia_de_marcadores())),
        (
            "_PADROES",
            Lista(padroes().into_iter().map(|(texto, _, _)| texto).collect()),
        ),
    ]
}

/// `_FIM_DE_CELULA`.
const FIM_DE_CELULA: &str = r"[\t|]";
/// The parentheses that `_rotulo_de_linha` drops.
const PARENTESES: &str = r"[ \t]*\([^)]*\)";
/// `_TRATAMENTO_NO_INICIO`.
const TRATAMENTO_NO_INICIO: &str = r"^(?i:dra|dr|sra|sr|profa|prof)(?:\((?i:a)\))?\.?[ \t]+";
/// `_PALAVRA_DE_NOME`, used with `fullmatch`.
const PALAVRA_DE_NOME: &str = r"[^\W\d_]+(?:['’-][^\W\d_]+)*\.?";
/// `_PARTICULA_DE_NOME`, used with `fullmatch`.
const PARTICULA_DE_NOME: &str = r"(?i:d[aeo]s?|e)";

/// The fixed helper regexes that are not in the vectors, as they appear in
/// `mascaramento.py`; an opt-in test finds each one in the pinned TestoDoc file.
#[must_use]
pub fn textos_auxiliares() -> [&'static str; 5] {
    [
        FIM_DE_CELULA,
        PARENTESES,
        TRATAMENTO_NO_INICIO,
        PALAVRA_DE_NOME,
        PARTICULA_DE_NOME,
    ]
}

/// Every compiled regex of the masking.
#[derive(Debug)]
pub(crate) struct Regexes {
    pub rotulo: PyRegex,
    pub fim_de_celula: PyRegex,
    pub parenteses: PyRegex,
    pub tratamento_no_inicio: PyRegex,
    pub palavra_de_nome: PyRegex,
    pub tratamento: PyRegex,
    pub particula_de_nome: PyRegex,
    pub registro: PyRegex,
    pub identificador: PyRegex,
    pub antes_de_faixa: PyRegex,
    pub antes_de_contagem: PyRegex,
    pub depois_de_contagem: PyRegex,
    pub padroes: Vec<(PyRegex, Filtro)>,
    pub datas_numericas: PyRegex,
    pub datas_extenso: PyRegex,
    pub sequencia_de_marcadores: PyRegex,
}

impl Regexes {
    fn compilar() -> Result<Self, ErroPyre> {
        let r = PyRegex::new;
        Ok(Self {
            rotulo: r(&rotulo_re(), Flags::MULTILINE)?,
            fim_de_celula: r(FIM_DE_CELULA, Flags::NENHUMA)?,
            parenteses: r(PARENTESES, Flags::NENHUMA)?,
            tratamento_no_inicio: r(TRATAMENTO_NO_INICIO, Flags::NENHUMA)?,
            palavra_de_nome: PyRegex::new_fullmatch(PALAVRA_DE_NOME, Flags::NENHUMA)?,
            tratamento: r(&tratamento_re(), Flags::NENHUMA)?,
            particula_de_nome: PyRegex::new_fullmatch(PARTICULA_DE_NOME, Flags::NENHUMA)?,
            registro: r(&registro_re(), Flags::IGNORECASE)?,
            identificador: r(&identificador_rotulado(), Flags::IGNORECASE)?,
            antes_de_faixa: r(ANTES_DE_FAIXA, Flags::NENHUMA)?,
            antes_de_contagem: r(ANTES_DE_CONTAGEM, Flags::NENHUMA)?,
            depois_de_contagem: r(DEPOIS_DE_CONTAGEM, Flags::NENHUMA)?,
            padroes: padroes()
                .into_iter()
                .map(|(texto, flags, filtro)| Ok((r(&texto, flags)?, filtro)))
                .collect::<Result<_, ErroPyre>>()?,
            datas_numericas: r(DATAS_NUMERICAS, Flags::NENHUMA)?,
            datas_extenso: r(&datas_extenso(), Flags::NENHUMA)?,
            sequencia_de_marcadores: r(&sequencia_de_marcadores(), Flags::IGNORECASE)?,
        })
    }
}

static REGEXES: LazyLock<Result<Regexes, ErroPyre>> = LazyLock::new(Regexes::compilar);

/// The compiled regexes, built on first use.
///
/// # Errors
///
/// The translation or compile error, if any; the tests show there is none.
pub(crate) fn regexes() -> Result<&'static Regexes, ErroPyre> {
    REGEXES.as_ref().map_err(Clone::clone)
}
