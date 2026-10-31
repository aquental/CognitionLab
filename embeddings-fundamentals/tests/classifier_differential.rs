//! Differential test of the masking and of `montar_estado` against TestoDoc's Python
//! (spec/CLASSIFIER.md §14.3). Opt-in and local:
//!
//! ```text
//! RUN_DIFFERENTIAL=1 TESTODOC_DIR=/path/to/testo-doc \
//!     cargo test --release --test classifier_differential -- --ignored
//! ```
//!
//! It builds random synthetic texts from labels, invented names, titles, numbers, dates,
//! clinical terms, separators and the Unicode traps, runs them (and the masked text a
//! second time, since TestoDoc's masking is not idempotent, spec §0.4 F9) through TestoDoc's
//! `mascaramento.py` and `classificador_documento.py` (Python 3.14 through `uv`, as
//! TestoDoc's vector generator does) and through the port, and compares. The port's one
//! deliberate masking change (spec §13, DV10) is applied to the Python side too, so
//! everything else is still compared with TestoDoc's own code. Nothing here comes from a
//! real exam.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use embeddings_fundamentals::classifier::{
    Camada, EntradaClassificacao, Pagina, PaginasModo, estado, indicios, listas, mascaramento,
};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, TestRunner};
use serde_json::{Value, json};

const PROGRAMA: &str = r#"
import json, sys, unicodedata
assert unicodedata.unidata_version == "16.0.0", unicodedata.unidata_version
from app.services import classificador_documento as cd, mascaramento as m
# DV10 (spec §13): the port bounds known-name parts by non-letters instead of `\b`. Apply the
# same change here, so every other behaviour is still compared with TestoDoc's own code.
import re
_original = m._padroes_conhecidos
def _padroes_conhecidos_dv9(conhecidos):
    saida = []
    for p in _original(conhecidos):
        t = p.pattern
        if t.startswith(r"\b(?:") and t.endswith(r")\b"):
            p = re.compile(r"(?<![^\W\d_])" + t[2:-2] + r"(?![^\W\d_])")
        saida.append(p)
    return saida
m._padroes_conhecidos = _padroes_conhecidos_dv9
def montar(doc, conhecidos):
    entrada = cd.EntradaClassificacao(
        nome_arquivo=doc["nome_arquivo"], titulo=doc["titulo"], extensao=doc["extensao"],
        total_paginas=len(doc["paginas"]), nomes_conhecidos=conhecidos,
        paginas=[cd.Pagina(n=i + 1, texto=t, origem=o) for i, (t, o) in enumerate(doc["paginas"])],
    )
    r = cd.montar_estado(entrada, doc["modo"])
    return {
        "estado": r.estado, "digest": r.digest.hex(), "paginas_enviadas": r.paginas_enviadas,
        "caracteres_enviados": r.caracteres_enviados, "truncado": r.truncado,
        "mascaramentos": r.mascaramentos, "origens_texto": r.origens_texto, "bloqueio": r.bloqueio,
        "caracteres_uteis": r.caracteres_uteis, "termo_do_tipo": r.termo_do_tipo,
    }
saida = []
for caso in json.load(sys.stdin):
    r = m.remover_dados_pessoais(caso["texto"], caso["conhecidos"], caso["preservar"])
    r2 = m.remover_dados_pessoais(r.texto, caso["conhecidos"], caso["preservar"])
    saida.append({
        "remover": {"texto": r.texto, "contagens": r.contagens},
        "segunda_passada": r2.texto,
        "conferir": m.conferir(caso["texto"], caso["conhecidos"]),
        "nomes_dos_rotulos": m.nomes_dos_rotulos(caso["texto"], caso["preservar"]),
        "nomes_dos_indicios": cd._nomes_dos_indicios(caso["texto"]),
        "montar_estado": montar(caso["documento"], caso["conhecidos"]),
    })
json.dump(saida, sys.stdout)
"#;

/// Pieces of exam-like text. Every name is invented.
const TOKENS: &[&str] = &[
    // labels
    "Paciente:",
    "PACIENTE:",
    "Nome:",
    "Nome do paciente:",
    "Nome:\n",
    "Médico responsável:",
    "Médico",
    "Responsável:\n",
    "Idade:",
    "Sexo:",
    "Sexo",
    "CPF:",
    "Telefone de contato:",
    "Data de nascimento:",
    "Dt. Nasc.:",
    "Nasc.",
    "Pedido:",
    "Convênio:",
    "Endereço:",
    "Altura (cm)",
    "Peso",
    "Data do exame",
    "Nome",
    "Estilo de vida:",
    "Prontuário -",
    "RG",
    "CNS",
    "Protocolo:\n",
    "Solicitante:",
    "nome do exame:",
    // invented names
    "Zanoni Prado",
    "ANA LUIZA COSTA",
    "Bruno de Souza",
    "Nilo Teixeira",
    "Carla dos Anjos",
    "Łukasz Nowak",
    "Joa\u{303}o Pires",
    "Maria e Silva",
    "TADEU VILAR",
    "Odete Ramos Lins",
    "não informado",
    "Ana",
    // titles
    "Dr.",
    "Dra.",
    "Dr(a).",
    "Prof.",
    "Sr",
    "DR.",
    "dra",
    // clinical text
    "FEVE 62 %",
    "Holter de 24 h",
    "SLG -19,5 %",
    "VO2 pico 2500-3500",
    "Total de batimentos QRS:",
    "112345",
    "TMR 1650 kcal/dia",
    "ECOCARDIOGRAMA COM STRAIN",
    "Masculino",
    "F",
    "34 anos",
    "valores dos segmentos",
    "MAPA",
    "Eco",
    "Análise da deformação miocárdica",
    // numbers and documents
    "123.456.789-09",
    "123 456 789 09",
    "(11) 98765-4321",
    "11 3456-7890",
    "9 8765-4321",
    "2500-3500",
    "1000-1200 ms",
    "1234567",
    "CRM/SP 123456",
    "CRM 1.234",
    "CRF 3,2 L",
    "RQE 4567",
    "CEP 01310 100",
    "01310-100",
    "12.345.678-9",
    "ana@ex.com",
    "www.ex.com/a",
    "nº 12345",
    // dates
    "12/03/1990",
    "1990-03-12",
    "12 de março de 1990",
    "mar/1980",
    "3/2026",
    "12 mar. 1980",
    "[REMOVIDO]",
    "[DATA]",
    // separators and traps
    " ",
    "  ",
    "\t",
    "\n",
    " | ",
    "; ",
    "\n\n",
    ": ",
    "\u{2028}",
    "_",
    "Ref 12345",
    "arquivo_",
    "2",
    "\r\n",
    "\u{301}",
    "İ",
    "ı",
];

const CONHECIDOS: &[&str] = &[
    "Zanoni Prado",
    "Carla dos Anjos",
    "123.456.789-09",
    "AB12",
    "  ",
    "Jo",
    "Nilo",
];

fn testodoc_backend() -> Option<PathBuf> {
    (std::env::var("RUN_DIFFERENTIAL").as_deref() == Ok("1"))
        .then(|| std::env::var("TESTODOC_DIR").ok())
        .flatten()
        .map(|dir| PathBuf::from(dir).join("backend"))
}

fn contagens(mapa: &std::collections::BTreeMap<Camada, u32>) -> Value {
    Value::Object(
        mapa.iter()
            .map(|(camada, n)| (camada.as_str().to_owned(), Value::from(*n)))
            .collect(),
    )
}

#[expect(clippy::expect_used, reason = "test helper")]
fn rust(caso: &Value) -> Value {
    let texto = caso["texto"].as_str().expect("text");
    let lista = |k: &str| -> Vec<String> {
        caso[k]
            .as_array()
            .expect("list")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let (conhecidos, preservar) = (lista("conhecidos"), lista("preservar"));
    let r = mascaramento::remover_dados_pessoais(texto, &conhecidos, &preservar).expect("mask");
    let r2 = mascaramento::remover_dados_pessoais(&r.texto, &conhecidos, &preservar).expect("mask");
    json!({
        "remover": {"texto": r.texto, "contagens": contagens(&r.contagens)},
        "segunda_passada": r2.texto,
        "conferir": contagens(&mascaramento::conferir(texto, &conhecidos).expect("check")),
        "nomes_dos_rotulos": mascaramento::nomes_dos_rotulos(texto, &preservar).expect("names"),
        "nomes_dos_indicios": indicios::nomes_dos_indicios(texto).expect("hints"),
        "montar_estado": montar(&caso["documento"], &conhecidos),
    })
}

#[expect(clippy::expect_used, reason = "test helper")]
fn montar(doc: &Value, conhecidos: &[String]) -> Value {
    let paginas: Vec<Pagina> = doc["paginas"]
        .as_array()
        .expect("pages")
        .iter()
        .zip(1..)
        .map(|(p, n)| Pagina {
            n,
            texto: p[0].as_str().expect("text").to_owned(),
            origem: serde_json::from_value(p[1].clone()).expect("origin"),
        })
        .collect();
    let entrada = EntradaClassificacao {
        nome_arquivo: doc["nome_arquivo"].as_str().expect("name").to_owned(),
        titulo: doc["titulo"].as_str().expect("title").to_owned(),
        extensao: doc["extensao"].as_str().expect("extension").to_owned(),
        total_paginas: u32::try_from(paginas.len()).expect("few pages"),
        paginas,
        nomes_conhecidos: conhecidos.to_vec(),
    };
    let modo: PaginasModo = doc["modo"].as_str().expect("mode").parse().expect("mode");
    let r = estado::montar_estado(&entrada, modo).expect("state");
    json!({
        "estado": r.estado.para_value(),
        "digest": hex::encode(r.digest),
        "paginas_enviadas": r.paginas_enviadas,
        "caracteres_enviados": r.caracteres_enviados,
        "truncado": r.truncado,
        "mascaramentos": contagens(&r.mascaramentos),
        "origens_texto": r.origens_texto.iter().map(|o| o.as_str()).collect::<Vec<_>>(),
        "bloqueio": contagens(&r.bloqueio),
        "caracteres_uteis": r.caracteres_uteis,
        "termo_do_tipo": r.termo_do_tipo,
    })
}

#[test]
#[ignore = "needs TestoDoc and uv; RUN_DIFFERENTIAL=1 TESTODOC_DIR=..."]
fn masking_matches_testodoc() {
    let Some(backend) = testodoc_backend() else {
        return;
    };
    let pagina = (
        proptest::collection::vec(proptest::sample::select(TOKENS.to_vec()), 0..20),
        proptest::sample::select(vec!["nativo", "ocr_local", "visao"]),
    )
        .prop_map(|(tokens, origem)| json!([tokens.concat(), origem]));
    let documento = (
        proptest::collection::vec(pagina, 1..5),
        proptest::sample::select(vec!["1", "2", "todas"]),
        proptest::sample::select(vec![
            "Zanoni_Prado_holter_24h.pdf",
            "eco strain 3D.pdf",
            "digitalizado_0042.PDF",
            "laudo.pdf",
        ]),
        proptest::sample::select(vec!["", "Holter 24 h", "ECO COM STRAIN", "Exame"]),
        proptest::sample::select(vec!["pdf", ".PDF", "Pdf.", "AB123", "jpeg2000x", ""]),
    )
        .prop_map(|(paginas, modo, nome_arquivo, titulo, extensao)| {
            json!({"paginas": paginas, "modo": modo, "nome_arquivo": nome_arquivo,
                   "titulo": titulo, "extensao": extensao})
        });
    let estrategia = (
        proptest::collection::vec(proptest::sample::select(TOKENS.to_vec()), 0..28),
        proptest::sample::subsequence(CONHECIDOS.to_vec(), 0..=3),
        any::<bool>(),
        documento,
    );
    let mut runner = TestRunner::new(Config::default());
    let vocabulario: Vec<&str> = listas::VOCABULARIO_TIPO.to_vec();
    let casos: Vec<Value> = (0..3000)
        .map(|_| {
            let (tokens, conhecidos, preservar, documento) = estrategia
                .new_tree(&mut runner)
                .expect("generate")
                .current();
            json!({
                "texto": tokens.concat(),
                "conhecidos": conhecidos,
                "preservar": if preservar { vocabulario.clone() } else { Vec::new() },
                "documento": documento,
            })
        })
        .collect();
    let mut filho = Command::new("uv")
        .args([
            "run",
            "--no-project",
            "--python",
            "3.14",
            "--with-requirements",
            "requirements.txt",
            "python",
            "-c",
            PROGRAMA,
        ])
        .current_dir(&backend)
        .env("PYTHONPATH", ".")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run TestoDoc's Python");
    serde_json::to_writer(filho.stdin.take().expect("stdin"), &casos).expect("write inputs");
    let out = filho.wait_with_output().expect("python output");
    assert!(out.status.success(), "TestoDoc's Python failed");
    let esperado: Vec<Value> = serde_json::from_slice(&out.stdout).expect("parse");
    assert_eq!(esperado.len(), casos.len());
    for (caso, esperado) in casos.iter().zip(&esperado) {
        assert_eq!(&rust(caso), esperado, "input {caso}");
    }
}

/// sha256 of the pinned TestoDoc files the port follows (spec/CLASSIFIER.md §0.3).
const TP_SHA256: &[(&str, &str)] = &[
    (
        "app/services/mascaramento.py",
        "9c8d9b0b0d1cb3170480e7ca8d8fdc242b49db383b522ab400d6d98e3133da05",
    ),
    (
        "app/services/classificador_documento.py",
        "72f893b3384134352357e4156dde0b8cd00387ae80d5f34ec9295ec7e996c8f4",
    ),
    (
        "app/services/patient_hints.py",
        "43917ab30c9e22b3245c2c73cef9833f69d24efbe5d42eed40125f0cffd94a54",
    ),
];

/// The regexes that are not in the vectors appear, as raw strings, in the pinned
/// TestoDoc files (spec/CLASSIFIER.md §6.4, "Texto das traduções").
#[test]
#[ignore = "needs TestoDoc; RUN_DIFFERENTIAL=1 TESTODOC_DIR=..."]
fn helper_regexes_are_testodocs() {
    use sha2::{Digest, Sha256};
    let Some(backend) = testodoc_backend() else {
        return;
    };
    let ler = |arquivo: &str| std::fs::read_to_string(backend.join(arquivo)).expect("read");
    for (arquivo, sha) in TP_SHA256 {
        assert_eq!(
            hex::encode(Sha256::digest(ler(arquivo))),
            *sha,
            "{arquivo} changed"
        );
    }
    let mascaramento_py = ler("app/services/mascaramento.py");
    for texto in mascaramento::textos_auxiliares() {
        assert!(
            mascaramento_py.contains(&format!("r\"{texto}\"")),
            "{texto}"
        );
    }
    let hints_py = ler("app/services/patient_hints.py");
    for texto in [indicios::NOME_REGEX_1, indicios::NOME_REGEX_2] {
        assert!(hints_py.contains(&format!("r\"{texto}\"")), "{texto}");
    }
    let classificador_py = ler("app/services/classificador_documento.py");
    assert!(
        classificador_py
            .contains(r#"re.search(r"\s+".join(map(re.escape, palavras)), texto, re.IGNORECASE)"#)
    );
}
