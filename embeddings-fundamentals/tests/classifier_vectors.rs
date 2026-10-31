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

/// Sections a test below checks.
const COVERED: &[&str] = &["meta", "constantes", "criterio"];

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
    for section in COVERED.iter().chain(PENDING).chain(EXCLUDED) {
        assert!(seen.insert(*section), "section `{section}` is listed twice");
    }
    assert_eq!(
        sections, seen,
        "the lists must partition the vectors' sections"
    );
}

#[test]
fn meta_matches_the_port_versions() {
    let v = vectors();
    let meta = &v["meta"];
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

#[test]
fn constants_match() {
    let v = vectors();
    let c = &v["constantes"];
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

#[test]
fn criterion_matches_question_and_options_in_order() {
    let v = vectors();
    let criterio = &v["criterio"];
    assert_eq!(text(criterio, "id"), classifier::CRITERIO_ID);
    assert_eq!(text(criterio, "pergunta"), classifier::PERGUNTA);
    let opcoes = criterio["opcoes"].as_array().expect("`opcoes` is a list");
    assert_eq!(opcoes.len(), Categoria::TODAS.len());
    for (opcao, categoria) in opcoes.iter().zip(Categoria::TODAS) {
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
