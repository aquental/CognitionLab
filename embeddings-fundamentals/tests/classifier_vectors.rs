//! Conformance of the exam-type classifier port with TestoDoc's vectors
//! (`tests/fixtures/classifier/vetores.json`, spec/CLASSIFIER.md §14.1).
//!
//! The vectors are read directly, never copied into code. Every top-level section is
//! either covered by a test here, still pending (a later phase covers it), or excluded
//! from the port's scope; the three sets must partition the file's sections.

use std::collections::BTreeSet;
use std::path::PathBuf;

use embeddings_fundamentals::classifier::{
    self, Camada, Categoria, canonico, estado, indicios, listas, mascaramento, texto,
};
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
    ("listas", check_listas),
    ("dobrar", check_dobrar),
    ("compor", check_compor),
    ("partes_de_nome", check_partes_de_nome),
    ("termos_permitidos", check_termos_permitidos),
    ("json_canonico", check_json_canonico),
    ("regexes", check_regexes),
    ("remover_dados_pessoais", check_remover_dados_pessoais),
    ("conferir", check_conferir),
    ("nomes_dos_rotulos", check_nomes_dos_rotulos),
    ("nomes_dos_indicios", check_nomes_dos_indicios),
];

/// Sections a later phase covers (spec/CLASSIFIER.md §20). This list only shrinks: a
/// phase that covers a section moves it to `COVERED`.
const PENDING: &[&str] = &[
    "montar_estado",
    "decidir_classificacao",
    "validar_respostas",
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

/// The cases of a section; a section with no cases fails, so it cannot pass empty.
#[expect(clippy::panic, reason = "a check fails by panicking")]
fn cases(section: &Value) -> &[Value] {
    match section.as_array() {
        Some(cases) if !cases.is_empty() => cases,
        _ => panic!("the section is not a non-empty list of cases"),
    }
}

/// A list of strings; any other value fails.
#[expect(clippy::panic, reason = "a check fails by panicking")]
fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("not a list: {value}"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("not a string: {item}"))
        })
        .collect()
}

#[expect(clippy::expect_used, reason = "a check fails by panicking")]
fn check_listas(section: &Value) {
    let listas_tv = section.as_object().expect("`listas` is an object");
    let nomes: Vec<&str> = listas::TODAS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        nomes,
        listas_tv.keys().map(String::as_str).collect::<Vec<_>>()
    );
    for (nome, lista) in listas::TODAS {
        assert_eq!(strings(&listas_tv[*nome]), *lista, "list {nome}");
    }
}

fn check_dobrar(section: &Value) {
    for case in cases(section) {
        let entrada = text(case, "entrada");
        assert_eq!(
            texto::dobrar(entrada),
            text(case, "saida"),
            "dobrar({entrada:?})"
        );
    }
}

fn check_compor(section: &Value) {
    for case in cases(section) {
        let entrada = text(case, "entrada");
        assert_eq!(
            texto::compor(entrada),
            text(case, "saida"),
            "compor({entrada:?})"
        );
    }
}

fn check_partes_de_nome(section: &Value) {
    for case in cases(section) {
        let entrada = text(case, "entrada");
        assert_eq!(
            mascaramento::partes_de_nome(entrada),
            strings(&case["saida"]),
            "partes_de_nome({entrada:?})"
        );
    }
}

fn check_termos_permitidos(section: &Value) {
    for case in cases(section) {
        let entrada = text(case, "entrada");
        assert_eq!(
            estado::termos_permitidos(entrada),
            strings(&case["saida"]),
            "termos_permitidos({entrada:?})"
        );
    }
}

#[expect(clippy::expect_used, reason = "a check fails by panicking")]
fn check_json_canonico(section: &Value) {
    for case in cases(section) {
        let saida = canonico::json_canonico(&case["valor"]).expect("integers and strings only");
        assert_eq!(saida, text(case, "saida"));
        assert_eq!(
            hex::encode(canonico::digest(&case["valor"]).expect("digest")),
            text(case, "sha256_hex")
        );
    }
}

/// A list of strings, owned.
fn owned(value: &Value) -> Vec<String> {
    strings(value).into_iter().map(str::to_owned).collect()
}

/// Per-layer counts as the vectors write them: an object with only the layers above
/// zero, in layer order.
fn contagens(mapa: &std::collections::BTreeMap<Camada, u32>) -> Value {
    Value::Object(
        mapa.iter()
            .map(|(camada, n)| (camada.as_str().to_owned(), Value::from(*n)))
            .collect(),
    )
}

#[expect(clippy::expect_used, reason = "a check fails by panicking")]
fn check_regexes(section: &Value) {
    let textos = mascaramento::textos_python();
    let nomes: Vec<&str> = textos.iter().map(|(nome, _)| *nome).collect();
    assert_eq!(
        nomes,
        section
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    for (nome, texto) in &textos {
        match texto {
            mascaramento::TextoPython::Um(texto) => {
                assert_eq!(text(section, nome), texto, "{nome}");
            }
            mascaramento::TextoPython::Lista(lista) => {
                assert_eq!(strings(&section[*nome]), *lista, "{nome}");
            }
        }
    }
}

#[expect(clippy::expect_used, reason = "a check fails by panicking")]
fn check_remover_dados_pessoais(section: &Value) {
    for (i, case) in cases(section).iter().enumerate() {
        let saida = mascaramento::remover_dados_pessoais(
            text(case, "texto"),
            &owned(&case["conhecidos"]),
            &owned(&case["preservar"]),
        )
        .expect("masking runs");
        assert_eq!(saida.texto, text(&case["saida"], "texto"), "case {i}");
        assert_eq!(
            contagens(&saida.contagens),
            case["saida"]["contagens"],
            "case {i}"
        );
    }
}

#[expect(clippy::expect_used, reason = "a check fails by panicking")]
fn check_conferir(section: &Value) {
    for (i, case) in cases(section).iter().enumerate() {
        let achados = mascaramento::conferir(text(case, "texto"), &owned(&case["conhecidos"]))
            .expect("the check runs");
        assert_eq!(contagens(&achados), case["saida"], "case {i}");
    }
}

#[expect(clippy::expect_used, reason = "a check fails by panicking")]
fn check_nomes_dos_rotulos(section: &Value) {
    for (i, case) in cases(section).iter().enumerate() {
        let nomes =
            mascaramento::nomes_dos_rotulos(text(case, "texto"), &owned(&case["preservar"]))
                .expect("runs");
        assert_eq!(nomes, strings(&case["saida"]), "case {i}");
    }
}

#[expect(clippy::expect_used, reason = "a check fails by panicking")]
fn check_nomes_dos_indicios(section: &Value) {
    for (i, case) in cases(section).iter().enumerate() {
        let nomes = indicios::nomes_dos_indicios(text(case, "entrada")).expect("runs");
        assert_eq!(nomes, strings(&case["saida"]), "case {i}");
    }
}
