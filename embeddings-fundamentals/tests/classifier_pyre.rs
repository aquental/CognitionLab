//! The Python regexes of the classifier's masking, translated by `pyre`
//! (spec/CLASSIFIER.md §6.4).
//!
//! Always: every regex of the vectors translates and compiles with the flags TestoDoc
//! gives it. Opt-in (`RUN_PYTHON_DIFF=1 PYTHON314=/opt/homebrew/bin/python3.14 cargo test
//! --test classifier_pyre -- --include-ignored`): `finditer` and `search(s, pos, endpos)`
//! agree with Python 3.14's `re` on random strings built from the regexes' own tokens and
//! the Unicode traps.

use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use embeddings_fundamentals::classifier::pyre::{Flags, PyRegex};
use embeddings_fundamentals::classifier::texto::MapaOffsets;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, TestRunner};
use serde_json::{Value, json};

#[expect(clippy::expect_used, reason = "test helper")]
fn vectors() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/classifier/vetores.json");
    serde_json::from_slice(&std::fs::read(path).expect("read the vectors")).expect("parse")
}

/// Every regex of the vectors' `regexes` section, with TestoDoc's flags
/// (`mascaramento.py`; spec §6.4, "Inventário").
#[expect(clippy::expect_used, reason = "test helper")]
fn tv_regexes() -> Vec<(String, String, Flags)> {
    let v = vectors();
    let r = &v["regexes"];
    let mut saida = Vec::new();
    for (nome, flags) in [
        ("_ROTULO_RE", Flags::MULTILINE),
        ("_TRATAMENTO_RE", Flags::NENHUMA),
        ("_REGISTRO_RE", Flags::IGNORECASE),
        ("_IDENTIFICADOR_ROTULADO", Flags::IGNORECASE),
        ("_ANTES_DE_FAIXA", Flags::NENHUMA),
        ("_ANTES_DE_CONTAGEM", Flags::NENHUMA),
        ("_DEPOIS_DE_CONTAGEM", Flags::NENHUMA),
        ("_DATAS_NUMERICAS", Flags::NENHUMA),
        ("_DATAS_EXTENSO", Flags::NENHUMA),
        ("_SEQUENCIA_DE_MARCADORES", Flags::IGNORECASE),
    ] {
        let texto = r[nome].as_str().expect("a regex is a string");
        saida.push((nome.to_owned(), texto.to_owned(), flags));
    }
    let padroes = r["_PADROES"].as_array().expect("_PADROES is a list");
    assert_eq!(padroes.len(), 12);
    for (i, padrao) in padroes.iter().enumerate() {
        // URL and the two phone patterns that end with `_UNIDADE_DEPOIS`.
        let flags = if [1, 9, 10].contains(&i) {
            Flags::IGNORECASE
        } else {
            Flags::NENHUMA
        };
        let texto = padrao.as_str().expect("a regex is a string");
        saida.push((format!("_PADROES[{i}]"), texto.to_owned(), flags));
    }
    saida
}

#[test]
fn every_vectors_regex_translates_and_compiles() {
    for (nome, texto, flags) in tv_regexes() {
        let inicio = Instant::now();
        let compilada = PyRegex::new(&texto, flags);
        assert!(compilada.is_ok(), "{nome}: {compilada:?}");
        let ms = inicio.elapsed().as_millis();
        assert!(ms < 2_000, "{nome} took {ms} ms to compile");
    }
}

fn python() -> Option<String> {
    (std::env::var("RUN_PYTHON_DIFF").as_deref() == Ok("1"))
        .then(|| std::env::var("PYTHON314").unwrap_or_else(|_| "python3.14".to_owned()))
}

const PROGRAMA: &str = r#"
import json, re, sys, unicodedata
assert unicodedata.unidata_version == "16.0.0", unicodedata.unidata_version
dados = json.load(sys.stdin)
saida = []
span = lambda m: [m.start(), m.end()] if m else None
for caso in dados:
    flags = (re.IGNORECASE if caso["i"] else 0) | (re.MULTILINE if caso["m"] else 0)
    p = re.compile(caso["padrao"], flags)
    linhas = []
    for s in caso["textos"]:
        n = len(s)
        linha = {
            "finditer": [span(m) for m in p.finditer(s)],
            "search": span(p.search(s, n // 3, n - n // 4)),
        }
        if caso["modo"] == "janelas":
            linha["janelas"] = [span(p.search(s, max(0, f - 40), f)) for f in range(n + 1)]
        if caso["modo"] == "match":
            linha["match"] = [span(p.match(s, i)) for i in range(n + 1)]
        linhas.append(linha)
    saida.append(linhas)
json.dump(saida, sys.stdout)
"#;

/// How TestoDoc uses a regex besides `finditer`: `_nao_e_faixa` and `_nao_e_contagem`
/// search the 40 code points before a match (`janelas`); `_valores_em_linha` and
/// `_nao_e_contagem` match at a position (`match`).
fn modo(nome: &str) -> &'static str {
    match nome {
        "_ANTES_DE_FAIXA" | "_ANTES_DE_CONTAGEM" => "janelas",
        "_ROTULO_RE" | "_DEPOIS_DE_CONTAGEM" => "match",
        _ => "",
    }
}

#[expect(clippy::expect_used, reason = "test helper")]
fn em_chars(mapa: &MapaOffsets, (a, b): (usize, usize)) -> [usize; 2] {
    [
        mapa.char_de_byte(a)
            .expect("a match starts at a code point"),
        mapa.char_de_byte(b).expect("a match ends at a code point"),
    ]
}

#[expect(clippy::expect_used, reason = "test helper")]
fn rust(regex: &PyRegex, modo: &str, s: &str) -> Value {
    let mapa = MapaOffsets::new(s);
    let n = s.chars().count();
    let byte = |c: usize| mapa.byte_de_char(c).expect("in range");
    let span = |m: Option<(usize, usize)>| m.map(|m| em_chars(&mapa, m));
    let achados: Vec<[usize; 2]> = regex
        .finditer(s)
        .map(|c| {
            let m = c.expect("search").get(0).expect("group 0");
            em_chars(&mapa, (m.start(), m.end()))
        })
        .collect();
    let busca = span(
        regex
            .search(s, byte(n / 3), byte(n - n / 4))
            .expect("search"),
    );
    let mut linha = json!({"finditer": achados, "search": busca});
    if modo == "janelas" {
        linha["janelas"] = (0..=n)
            .map(|f| {
                span(
                    regex
                        .search(s, byte(f.saturating_sub(40)), byte(f))
                        .expect("search"),
                )
            })
            .collect();
    }
    if modo == "match" {
        linha["match"] = (0..=n)
            .map(|i| span(regex.match_at(s, byte(i)).expect("match")))
            .collect();
    }
    linha
}

/// Pieces the regexes look for, the separators around them, and the Unicode traps.
const TOKENS: &[&str] = &[
    "Paciente",
    "PACİENTE",
    "pacıente",
    "nome",
    "Nome do paciente",
    "medico",
    "Médico responsável",
    "telefone",
    "idade",
    "sexo",
    "cpf",
    "CPF",
    "rg",
    "cns",
    "cep",
    "cartao sus",
    "crm",
    "CRM",
    "CRF",
    "RQE",
    "COREN",
    "SP",
    "sp",
    "Dr.",
    "dra",
    "Dra(a).",
    "prof",
    "Sr",
    "Ana",
    "Łukasz",
    "da",
    "de",
    "dos",
    "e",
    "di",
    "DI",
    "Dİ",
    "van",
    "[REMOVIDO]",
    "[removıdo]",
    "[DATA]",
    "nº",
    "n.",
    "no",
    "vo2",
    "pico",
    "entre",
    "faixa",
    "valores",
    "batimentos",
    "qrs",
    "QRS",
    "ectopias",
    "eventos",
    "kcal",
    "ml",
    "min",
    "MİN",
    "mmhg",
    "bpm",
    "ms",
    "%",
    "w",
    "janeiro",
    "marco",
    "mar",
    "set",
    "out",
    "dez",
    "de 1990",
    "º",
    "°",
    "12",
    "3",
    "1990",
    "2026",
    "123.456.789-09",
    "123 456 789 09",
    "12.345.678/0001-90",
    "01310-100",
    "12.345-678",
    "(11) 98765-4321",
    "11 3456-7890",
    "9 8765-4321",
    "2500-3500",
    "112345",
    "1234567",
    "12/03/1990",
    "1990-03-12",
    "3/2026",
    "12/mar/1980",
    "a@b.com",
    "x.y+z@dom-1.com.br",
    "https://ex.com/a",
    "www.ex.com",
    ":",
    " - ",
    "-",
    "/",
    ".",
    ",",
    ";",
    "|",
    "\t",
    "  ",
    " ",
    "\n",
    "\u{2028}",
    "\u{1f}",
    "\u{a0}",
    "\u{301}",
    "\u{203f}",
    "²",
    "½",
    "٣",
    "_",
    "ſ",
    "\u{212a}",
    "X",
    "x",
    "Ⅻ",
];

/// Hand-made texts for each trap, besides the random ones.
const FIXOS: &[&str] = &[
    "[REMOVIDO] [removıdo] e [REMOVİDO]",
    "[REMOVIDO] dİ [REMOVIDO]\t[REMOVIDO] dı [REMOVIDO]",
    "tel 11 3456-7890 MİN e 9 8765-4321 mın; 2500-3500 Min",
    "CRM/İı 123456 e CRM-ıİ 1.234-X/SP e CRM 1234/İı e crm-sp 12345",
    "vo2 pico 2500-3500\nentre 2000-2500\n",
    "Total de batimentos QRS: 112345\n123456 batimentos",
    "Paciente: Ana\n  Nome:\nAna Lima\nMédico responsável - Dr. Łukasz",
    "Dra(a). Ana e Dr. Bruno de Souza; DR. PAULO MOTA FEVE 62 %",
    "12 de março de 1990, 12/mar/1980, mar/1980, 3 de janeiro, 1990-03-12",
    "a\u{301}b\u{203f}c ² ½ ٣ ſ \u{212a}",
];

#[test]
#[ignore = "needs Python 3.14; RUN_PYTHON_DIFF=1"]
fn translated_regexes_match_python() {
    let Some(python) = python() else { return };
    let token = prop_oneof![
        6 => proptest::sample::select(TOKENS.to_vec()).prop_map(str::to_owned),
        1 => any::<char>().prop_map(String::from),
    ];
    // Tokens joined by the separators the regexes care about, so that sequences such as
    // "[REMOVIDO] [removıdo]" or "vo2 pico 2500-3500\n" come up.
    let separador = proptest::sample::select(vec!["", " ", " ", "\t", "\n", ": ", "  "]);
    let estrategia = proptest::collection::vec((token, separador), 0..16)
        .prop_map(|v| v.into_iter().map(|(t, sep)| t + sep).collect::<String>());
    let mut runner = TestRunner::new(Config::default());
    let textos: Vec<String> = FIXOS
        .iter()
        .map(|&s| s.to_owned())
        .chain((0..1500).map(|_| {
            estrategia
                .new_tree(&mut runner)
                .expect("generate")
                .current()
        }))
        .collect();
    let regexes = tv_regexes();
    let casos: Vec<Value> = regexes
        .iter()
        .map(|(nome, padrao, flags)| {
            json!({"padrao": padrao, "i": flags.ignorecase, "m": flags.multiline,
                   "modo": modo(nome), "textos": textos})
        })
        .collect();
    let mut filho = Command::new(python)
        .args(["-c", PROGRAMA])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run python");
    serde_json::to_writer(filho.stdin.take().expect("stdin"), &casos).expect("write inputs");
    let out = filho.wait_with_output().expect("python output");
    assert!(out.status.success());
    let esperado: Vec<Vec<Value>> = serde_json::from_slice(&out.stdout).expect("parse");
    for ((nome, padrao, flags), esperado) in regexes.iter().zip(&esperado) {
        let regex = PyRegex::new(padrao, *flags).expect("compile");
        for (texto, esperado) in textos.iter().zip(esperado) {
            assert_eq!(
                &rust(&regex, modo(nome), texto),
                esperado,
                "{nome} on {texto:?}"
            );
        }
    }
}
