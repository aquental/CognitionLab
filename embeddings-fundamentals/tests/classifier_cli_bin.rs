//! The `classify` binary (spec/CLASSIFIER.md §10.3, §20 Phase 5). The stops before the
//! extraction run always; the runs over the synthetic fixtures need poppler and
//! tesseract and are opt-in: `RUN_INTEGRATION=1 cargo test --test classifier_cli_bin`.
//! Each run gets a clean environment and a temporary working directory, so no `.env`
//! and no real key is ever read.
#![expect(clippy::unwrap_used, reason = "test code")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const VARIAVEIS: &[&str] = &[
    "CLASSIFIER_PAGINAS",
    "CLASSIFIER_CONFIANCA_MIN",
    "CLASSIFIER_REMOTE_ALLOWED",
    "RLCD_MODEL",
    "RLCD_API_KEY",
    "TYPESAFE_API_KEY",
    "RLCD_BASE_URL",
    "RLCD_TIMEOUT_S",
    "RLCD_MAX_CONCURRENCY",
    "RUST_LOG",
];

fn fixture(nome: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/classifier/cli")
        .join(nome)
}

fn classify(args: &[&str], vars: &[(&str, &str)]) -> (Output, Value) {
    let dir = tempfile::tempdir().unwrap();
    let mut comando = Command::new(env!("CARGO_BIN_EXE_embeddings-fundamentals"));
    comando.arg("classify").args(args).current_dir(dir.path());
    for v in VARIAVEIS {
        comando.env_remove(v);
    }
    comando.envs(vars.iter().copied());
    let saida = comando.output().unwrap();
    let stdout = String::from_utf8(saida.stdout.clone()).unwrap();
    let linhas: Vec<&str> = stdout.lines().collect();
    let json = if args.contains(&"--json") {
        assert_eq!(linhas.len(), 1, "exactly one line: {stdout}");
        serde_json::from_str(linhas[0]).unwrap()
    } else {
        Value::Null
    };
    (saida, json)
}

fn integracao() -> bool {
    std::env::var("RUN_INTEGRATION").as_deref() == Ok("1")
}

#[test]
fn stops_before_the_extraction_print_one_line_and_no_path() {
    let dir = tempfile::tempdir().unwrap();
    let pasta = dir.path().join("Paciente Sintetico Zanoni Prado");
    std::fs::create_dir(&pasta).unwrap();
    let docx = pasta.join("Zanoni_Prado.docx");
    std::fs::write(&docx, b"synthetic").unwrap();
    let falta = pasta.join("Zanoni_Prado.pdf");
    let (docx, falta) = (docx.to_str().unwrap(), falta.to_str().unwrap());
    for (args, vars, codigo, motivo) in [
        (vec!["--json", falta], vec![], 2, "arquivo_inexistente"),
        (vec!["--json", docx], vec![], 2, "formato_nao_suportado"),
        (
            vec!["--json", "--paginas", "3", falta],
            vec![],
            5,
            "configuracao_invalida",
        ),
        (
            vec!["--json", falta],
            vec![("CLASSIFIER_CONFIANCA_MIN", "2")],
            5,
            "configuracao_invalida",
        ),
    ] {
        let (saida, json) = classify(&args, &vars);
        assert_eq!(saida.status.code(), Some(codigo), "{args:?}");
        assert_eq!(json["codigo_saida"], codigo);
        assert_eq!(json["estado"], "erro");
        assert_eq!(json["motivo_codigo"], motivo);
        let stderr = String::from_utf8(saida.stderr).unwrap();
        assert!(stderr.starts_with("classify: "), "{stderr}");
        for texto in [String::from_utf8(saida.stdout).unwrap(), stderr] {
            for proibido in ["Zanoni", "Prado", "Paciente Sintetico", ".docx"] {
                assert!(!texto.contains(proibido), "{proibido} in {texto}");
            }
        }
    }
    // The readable mode carries the same fields and no path either.
    let (saida, _) = classify(&[falta], &[]);
    let stdout = String::from_utf8(saida.stdout).unwrap();
    assert!(
        stdout.contains("motivo_codigo: \"arquivo_inexistente\""),
        "{stdout}"
    );
    assert!(!stdout.contains("Zanoni"));
}

#[test]
fn synthetic_fixtures_are_prepared_without_sending() {
    if !integracao() {
        return;
    }
    for (nome, origem) in [
        ("holter_24h.pdf", "nativo"),
        ("eco_mapa.pdf", "nativo"),
        ("eco_strain_digitalizado.pdf", "ocr_local"),
        ("eco_strain.png", "ocr_local"),
    ] {
        let caminho = fixture(nome);
        let (saida, json) = classify(&["--json", caminho.to_str().unwrap()], &[]);
        assert_eq!(saida.status.code(), Some(0), "{nome}");
        assert_eq!(json["estado"], "preparado", "{nome}");
        assert_eq!(json["envio_realizado"], false);
        assert_eq!(json["origens_texto"], serde_json::json!([origem]), "{nome}");
        assert_eq!(json["paginas_enviadas"], 1);
        assert!(json["motivo_codigo"].is_null());
    }
    // `--enviar` with CLASSIFIER_REMOTE_ALLOWED=0: prepared, code 0, no key needed.
    let holter = fixture("holter_24h.pdf");
    let (saida, json) = classify(
        &["--json", "--enviar", holter.to_str().unwrap()],
        &[
            ("CLASSIFIER_REMOTE_ALLOWED", "0"),
            ("RLCD_MODEL", "jev-latest"),
        ],
    );
    assert_eq!(saida.status.code(), Some(0));
    assert_eq!(json["motivo_codigo"], "envio_nao_autorizado");
    // The masked state, for a synthetic input only: the labelled name is gone.
    let (_, json) = classify(
        &["--json", "--mostrar-estado", holter.to_str().unwrap()],
        &[],
    );
    let estado = json["estado_preparado"].to_string();
    assert!(
        estado.contains("[REMOVIDO]") && !estado.contains("Teixeira"),
        "{estado}"
    );
}

#[test]
fn a9_a_known_name_that_is_a_file_term_blocks_without_the_state() {
    if !integracao() {
        return;
    }
    let eco = fixture("eco_mapa.pdf");
    let (saida, json) = classify(
        &[
            "--json",
            "--mostrar-estado",
            "--nome-paciente",
            "Joana Mapa",
            eco.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(saida.status.code(), Some(6));
    assert_eq!(json["estado"], "bloqueado_privacidade");
    assert_eq!(json["motivo_codigo"], "conferencia_privacidade");
    assert!(json.get("estado_preparado").is_none());
}

#[cfg(feature = "classifier-jev")]
#[test]
fn with_the_local_server_the_binary_classifies() {
    use std::io::{Read, Write};
    if !integracao() {
        return;
    }
    let escuta = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", escuta.local_addr().unwrap().port());
    let servidor = std::thread::spawn(move || {
        let (mut s, _) = escuta.accept().unwrap();
        let mut lido = Vec::new();
        let mut buf = [0_u8; 8192];
        loop {
            let n = s.read(&mut buf).unwrap();
            lido.extend_from_slice(&buf[..n]);
            let texto = String::from_utf8_lossy(&lido).to_lowercase();
            if let Some(fim) = texto.find("\r\n\r\n") {
                let tamanho: usize = texto[..fim]
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .map_or(0, |v| v.trim().parse().unwrap());
                if lido.len() >= fim + 4 + tamanho || n == 0 {
                    break;
                }
            }
        }
        let mut p = serde_json::Map::new();
        for c in embeddings_fundamentals::classifier::Categoria::TODAS {
            let v = if c.valor() == "holter" { 0.92 } else { 0.01 };
            p.insert(c.valor().to_owned(), serde_json::json!(v));
        }
        let corpo = serde_json::json!({"model": "jev-1.13.0", "usage": {"input_tokens": 10, "output_tokens": 1},
            "answers": {"tipo_documento": {"type": "choice", "choice": "holter", "confidence": 0.9,
                                           "probabilities": p}}})
        .to_string();
        let resposta = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nx-typesafe-request-id: req-bin\r\nconnection: close\r\n\r\n{corpo}",
            corpo.len()
        );
        s.write_all(resposta.as_bytes()).unwrap();
    });
    let holter = fixture("holter_24h.pdf");
    let (saida, json) = classify(
        &["--json", "--enviar", holter.to_str().unwrap()],
        &[
            ("CLASSIFIER_REMOTE_ALLOWED", "1"),
            ("TYPESAFE_API_KEY", "sk-sintetica"),
            ("RLCD_BASE_URL", base.as_str()),
        ],
    );
    servidor.join().unwrap();
    assert_eq!(saida.status.code(), Some(0));
    assert_eq!(json["estado"], "ok");
    assert_eq!(json["tipo"], "holter");
    assert_eq!(json["envio_realizado"], true);
    assert_eq!(json["provider_request_id"], "req-bin");
    let stderr = String::from_utf8(saida.stderr).unwrap();
    assert!(!stderr.contains("sk-sintetica"));
}
