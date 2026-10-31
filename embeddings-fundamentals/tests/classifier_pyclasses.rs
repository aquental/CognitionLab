//! The generated Python character tables (`src/classifier/pyclasses.rs`) and the text
//! primitives built on them, against Python 3.14 itself (spec/CLASSIFIER.md §6.3, §14.4).
//!
//! The first test always runs. The others need Python 3.14 (Unicode 16.0.0) and are
//! opt-in: `RUN_PYTHON_DIFF=1 PYTHON314=/opt/homebrew/bin/python3.14 cargo test --test
//! classifier_pyclasses -- --ignored`.

use std::path::PathBuf;
use std::process::Command;

use embeddings_fundamentals::classifier::{canonico, pyclasses, texto};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, TestRunner};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[test]
fn generated_tables_come_from_the_current_generator() {
    let bytes = std::fs::read(repo("scripts/gen-pyclasses.py")).expect("read the generator");
    assert_eq!(
        hex::encode(Sha256::digest(&bytes)),
        pyclasses::GENERATOR_SHA256,
        "the generator changed: rerun `python3.14 scripts/gen-pyclasses.py > src/classifier/pyclasses.rs`"
    );
}

fn python() -> Option<String> {
    (std::env::var("RUN_PYTHON_DIFF").as_deref() == Ok("1"))
        .then(|| std::env::var("PYTHON314").unwrap_or_else(|_| "python3.14".to_owned()))
}

#[test]
#[ignore = "needs Python 3.14; RUN_PYTHON_DIFF=1"]
fn regenerating_gives_the_same_file() {
    let Some(python) = python() else { return };
    let out = Command::new(python)
        .arg(repo("scripts/gen-pyclasses.py"))
        .output()
        .expect("run the generator");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let atual = std::fs::read(repo("src/classifier/pyclasses.rs")).expect("read the tables");
    assert!(
        out.stdout == atual,
        "src/classifier/pyclasses.rs differs from the generator's output"
    );
}

/// Python applies each primitive to each input; returns one JSON object per input.
const PROGRAMA: &str = r#"
import json, re, sys, unicodedata
assert unicodedata.unidata_version == "16.0.0", unicodedata.unidata_version
QUEBRAS = re.compile("\r\n|[\r\x0b\x0c\x1c\x1d\x1e\x85  ]")
def dobrar(t):
    out = []
    for ch in t:
        base = unicodedata.normalize("NFD", ch)[0]
        baixa = base.lower()
        out.append(baixa[0] if len(baixa) == 1 else base)
    return "".join(out)
saida = []
for s in json.load(sys.stdin):
    saida.append({
        "dobrar": dobrar(s), "compor": QUEBRAS.sub("\n", unicodedata.normalize("NFC", s)),
        "lower": s.lower(), "upper": s.upper(), "title": s.title(), "isupper": s.isupper(),
        "strip": s.strip(), "split": s.split(),
    })
json.dump(saida, sys.stdout, ensure_ascii=True)
"#;

fn rust(s: &str) -> Value {
    json!({
        "dobrar": texto::dobrar(s), "compor": texto::compor(s), "lower": texto::lower(s),
        "upper": texto::upper(s), "title": texto::title(s), "isupper": texto::str_isupper(s),
        "strip": texto::strip(s), "split": texto::split(s).collect::<Vec<_>>(),
    })
}

#[test]
#[ignore = "needs Python 3.14; RUN_PYTHON_DIFF=1"]
fn primitives_match_python_on_random_strings() {
    let Some(python) = python() else { return };
    // Mostly the hard ranges: Latin with accents, combining marks, case oddities, the
    // line and space separators, and any other code point.
    let alfabeto = prop_oneof![
        3 => proptest::char::range('\u{0}', '\u{24f}'),
        2 => proptest::sample::select(vec![
            '\u{301}', '\u{303}', '\u{331}', 'İ', 'ı', 'ß', 'ǅ', 'ǆ', 'ﬁ', 'ª', 'º', 'Ω', 'Å', '²', '½', 'Ⅻ',
            '\u{1c}', '\u{1f}', '\u{85}', '\u{a0}', '\u{2028}', '\u{2029}', '\r', '\n', '\u{b}', '\u{c}', '‿',
            'Σ', 'Σ', 'σ', 'ς', 'Ο', 'Α', '\'', '.', ':', '\u{ad}', 'ʰ', '·',
        ]),
        1 => any::<char>(),
    ];
    let mut entradas: Vec<String> = Vec::new();
    let mut runner = TestRunner::new(Config {
        cases: 2000,
        ..Config::default()
    });
    let estrategia =
        proptest::collection::vec(alfabeto, 0..12).prop_map(|v| v.into_iter().collect::<String>());
    for _ in 0..2000 {
        entradas.push(
            estrategia
                .new_tree(&mut runner)
                .expect("generate")
                .current(),
        );
    }
    let mut filho = Command::new(python)
        .args(["-c", PROGRAMA])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run python");
    serde_json::to_writer(filho.stdin.take().expect("stdin"), &entradas).expect("write inputs");
    let out = filho.wait_with_output().expect("python output");
    assert!(out.status.success());
    let esperado: Vec<Value> = serde_json::from_slice(&out.stdout).expect("parse python output");
    for (entrada, esperado) in entradas.iter().zip(&esperado) {
        assert_eq!(&rust(entrada), esperado, "input {entrada:?}");
    }
}

const PROGRAMA_FLOATS: &str = r#"
import json, struct, sys
saida = []
for bits in json.load(sys.stdin):
    x = struct.unpack("<d", bytes.fromhex(bits))[0]
    saida.append(json.dumps({"x": [x]}, sort_keys=True, separators=(",", ":"), ensure_ascii=False))
json.dump(saida, sys.stdout)
"#;

#[test]
#[ignore = "needs Python 3.14; RUN_PYTHON_DIFF=1"]
fn canonical_floats_match_python() {
    let Some(python) = python() else { return };
    let mut runner = TestRunner::new(Config::default());
    // Every magnitude: random bit patterns, then the boundaries of the notation switch and
    // a value exactly between two shortest strings (Python takes the even digit).
    let mut floats: Vec<f64> = (0..20_000)
        .map(|_| {
            any::<u64>()
                .new_tree(&mut runner)
                .expect("generate")
                .current()
        })
        .map(f64::from_bits)
        .filter(|x| x.is_finite())
        .collect();
    floats.extend([
        0.0,
        -0.0,
        1.0,
        0.5,
        1e-4,
        1e-5,
        9.999e-5,
        1e15,
        1e16,
        9_999_999_999_999_998.0,
        1e17,
        123_456_789_012_345.6,
        5e-324,
        f64::MAX,
        f64::MIN_POSITIVE,
        0.1,
        0.2 + 0.1,
        -2.5e-7,
    ]);
    let bits: Vec<String> = floats
        .iter()
        .map(|x| hex::encode(x.to_le_bytes()))
        .collect();
    let mut filho = Command::new(python)
        .args(["-c", PROGRAMA_FLOATS])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run python");
    serde_json::to_writer(filho.stdin.take().expect("stdin"), &bits).expect("write inputs");
    let out = filho.wait_with_output().expect("python output");
    assert!(out.status.success());
    let esperado: Vec<String> = serde_json::from_slice(&out.stdout).expect("parse python output");
    for (x, esperado) in floats.iter().zip(&esperado) {
        assert_eq!(
            &canonico::json_canonico(&json!({"x": [x]})),
            esperado,
            "{x:e}"
        );
    }
}
