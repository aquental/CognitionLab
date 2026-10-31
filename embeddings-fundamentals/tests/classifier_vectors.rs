//! Conformance of the exam-type classifier port with TestoDoc's vectors
//! (`tests/fixtures/classifier/vetores.json`, spec/CLASSIFIER.md §14.1).
//!
//! The vectors are read directly, never copied into code. Every top-level section is
//! either covered by a test here, still pending (a later phase covers it), or excluded
//! from the port's scope; the three sets must partition the file's sections.

use std::collections::BTreeSet;
use std::path::PathBuf;

use embeddings_fundamentals::classifier::{self, Categoria};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// sha256 of the vectors this port is pinned to (spec/CLASSIFIER.md §0.1).
const VECTORS_SHA256: &str = "f14676c78e9bf590e0cb1534c83ab456c82cb8bd67e696cc94bb239d891db22e";

/// What checks one section. It receives only that section's value, so it cannot read a
/// section that is still pending or excluded.
type Check = fn(&Value);

/// Sections a check below covers, each with its check. A section is covered only by
/// being listed here with a check, and `every_covered_section_passes_its_check` runs
/// every entry, so no covered section is skipped.
const COVERED: &[(&str, Check)] = &[
    ("meta", check_meta),
    ("constantes", check_constantes),
    ("criterio", check_criterio),
];

/// Sections a later phase covers (spec/CLASSIFIER.md §20). This list only shrinks: a
/// phase that covers a section moves it to `COVERED`.
const PENDING: &[&str] = &[
    "listas",
    "regexes",
    "dobrar",
    "compor",
    "partes_de_nome",
    "termos_permitidos",
    "nomes_dos_indicios",
    "nomes_dos_rotulos",
    "remover_dados_pessoais",
    "conferir",
    "montar_estado",
    "decidir_classificacao",
    "validar_respostas",
    "json_canonico",
];

/// Sections outside the port's scope (spec/CLASSIFIER.md §2.1).
const EXCLUDED: &[&str] = &["referencia_pseudonima", "bloco_tipo_detectado"];

fn vectors_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/classifier/vetores.json")
}

#[expect(clippy::expect_used, reason = "test helper")]
fn vectors() -> Value {
    let bytes = std::fs::read(vectors_path()).expect("read the vectors");
    serde_json::from_slice(&bytes).expect("parse the vectors")
}

#[expect(clippy::panic, reason = "test helper")]
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("`{key}` is not a string"))
}

/// The keys of an object, to compare a section's key set with the expected one.
#[expect(clippy::panic, reason = "test helper")]
fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect()
}

/// Runs each check on its section and returns the sections that failed, or that the
/// vectors lack.
fn run_checks(vectors: &Value, checks: &[(&str, Check)]) -> Vec<String> {
    checks
        .iter()
        .filter_map(|&(name, check)| {
            let Some(section) = vectors.get(name) else {
                return Some(format!("{name} (missing)"));
            };
            std::panic::catch_unwind(|| check(section))
                .is_err()
                .then(|| name.to_owned())
        })
        .collect()
}

#[test]
fn every_covered_section_passes_its_check() {
    let failed = run_checks(&vectors(), COVERED);
    assert!(failed.is_empty(), "sections that failed: {failed:?}");
}

#[test]
fn the_runner_reports_failing_and_missing_sections() {
    fn passes(_: &Value) {}
    fn fails(_: &Value) {
        assert_eq!(1, 2, "a check that fails on purpose");
    }
    let v = serde_json::json!({"a": 1, "b": 2});
    assert_eq!(
        run_checks(&v, &[("a", passes), ("b", fails), ("c", passes)]),
        ["b", "c (missing)"]
    );
}

#[test]
fn vectors_are_the_pinned_file() {
    let bytes = std::fs::read(vectors_path()).expect("read the vectors");
    assert_eq!(hex::encode(Sha256::digest(&bytes)), VECTORS_SHA256);
}

#[test]
fn every_section_is_covered_pending_or_excluded_exactly_once() {
    let v = vectors();
    let sections: BTreeSet<&str> = v
        .as_object()
        .expect("the vectors are an object")
        .keys()
        .map(String::as_str)
        .collect();
    let mut seen = BTreeSet::new();
    let covered = COVERED.iter().map(|&(name, _)| name);
    for section in covered
        .chain(PENDING.iter().copied())
        .chain(EXCLUDED.iter().copied())
    {
        assert!(seen.insert(section), "section `{section}` is listed twice");
    }
    assert_eq!(
        sections, seen,
        "the lists must partition the vectors' sections"
    );
}

fn check_meta(meta: &Value) {
    assert_eq!(
        keys(meta),
        BTreeSet::from([
            "descricao",
            "gerador",
            "mascaramento_versao",
            "perguntas_versao",
            "python",
            "unicode_versao",
        ])
    );
    assert_eq!(
        text(meta, "mascaramento_versao"),
        classifier::MASCARAMENTO_VERSAO
    );
    assert_eq!(text(meta, "perguntas_versao"), classifier::PERGUNTAS_VERSAO);
    assert_eq!(text(meta, "unicode_versao"), classifier::UNICODE_VERSAO);
}

#[test]
fn normalization_tables_are_the_vectors_unicode_version() {
    // A `cargo update` back to unicode-normalization 0.1.25+ (Unicode 17/18) fails here
    // (spec/CLASSIFIER.md §4.2).
    let (major, minor, patch) = unicode_normalization::UNICODE_VERSION;
    assert_eq!(
        format!("{major}.{minor}.{patch}"),
        classifier::UNICODE_VERSAO
    );
}

#[expect(clippy::panic, reason = "a check fails by panicking")]
fn check_constantes(c: &Value) {
    assert_eq!(
        keys(c),
        BTreeSet::from([
            "CRITERIO_ID",
            "DATA",
            "ETAPA",
            "LIMITE_PAGINA",
            "LIMITE_TOTAL",
            "REMOVIDO",
            "TEXTO_UTIL_MIN",
            "TOLERANCIA_LIMIAR",
            "TOLERANCIA_SOMA_PROBABILIDADES",
        ])
    );
    let int = |k: &str| {
        c[k].as_u64()
            .unwrap_or_else(|| panic!("`{k}` is not an integer"))
    };
    let float = |k: &str| {
        c[k].as_f64()
            .unwrap_or_else(|| panic!("`{k}` is not a number"))
    };
    assert_eq!(int("LIMITE_PAGINA"), u64::from(classifier::LIMITE_PAGINA));
    assert_eq!(int("LIMITE_TOTAL"), u64::from(classifier::LIMITE_TOTAL));
    assert_eq!(int("TEXTO_UTIL_MIN"), u64::from(classifier::TEXTO_UTIL_MIN));
    // Exact equality is intended: both sides are the same decimal literal.
    assert_eq!(
        float("TOLERANCIA_LIMIAR").to_bits(),
        classifier::TOLERANCIA_LIMIAR.to_bits()
    );
    assert_eq!(
        float("TOLERANCIA_SOMA_PROBABILIDADES").to_bits(),
        classifier::TOLERANCIA_SOMA_PROBABILIDADES.to_bits()
    );
    assert_eq!(text(c, "REMOVIDO"), classifier::REMOVIDO);
    assert_eq!(text(c, "DATA"), classifier::DATA);
    assert_eq!(text(c, "ETAPA"), classifier::ETAPA);
    assert_eq!(text(c, "CRITERIO_ID"), classifier::CRITERIO_ID);
}

#[expect(clippy::expect_used, reason = "a check fails by panicking")]
fn check_criterio(criterio: &Value) {
    assert_eq!(keys(criterio), BTreeSet::from(["id", "opcoes", "pergunta"]));
    assert_eq!(text(criterio, "id"), classifier::CRITERIO_ID);
    assert_eq!(text(criterio, "pergunta"), classifier::PERGUNTA);
    let opcoes = criterio["opcoes"].as_array().expect("`opcoes` is a list");
    assert_eq!(opcoes.len(), Categoria::TODAS.len());
    for (opcao, categoria) in opcoes.iter().zip(Categoria::TODAS) {
        assert_eq!(
            keys(opcao),
            BTreeSet::from(["descricao", "rotulo", "slug", "valor"])
        );
        assert_eq!(text(opcao, "valor"), categoria.valor());
        assert_eq!(text(opcao, "rotulo"), categoria.rotulo());
        assert_eq!(text(opcao, "descricao"), categoria.descricao());
        assert_eq!(
            opcao["slug"].as_str(),
            categoria.slug(),
            "slug of {}",
            categoria.valor()
        );
    }
}
