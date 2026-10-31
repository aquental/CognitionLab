//! Timing of `montar_estado` (spec/CLASSIFIER.md §15): the 21 vector cases and one
//! synthetic 200-page document. Recorded, never gated. Opt-in:
//! `cargo test --release --test classifier_medicao -- --ignored --nocapture`.

use std::path::PathBuf;
use std::time::Instant;

use embeddings_fundamentals::classifier::estado::montar_estado;
use embeddings_fundamentals::classifier::{EntradaClassificacao, OrigemTexto, Pagina, PaginasModo};
use serde_json::Value;

/// One invented page: labels, names, numbers, dates and clinical text.
fn pagina(n: u32) -> Pagina {
    let linha = format!(
        "Paciente: Zanoni Prado Lins  Idade: 41 anos  Sexo: M\n\
         Médico responsável: Dr. Nilo Teixeira CRM/SP 123456\n\
         Data do exame: 12/03/2026  Página {n}\n\
         Holter de 24 h: ritmo sinusal, FC média 72 bpm, 1234 extrassístoles.\n\
         Total de batimentos QRS: 112345. Pausas: nenhuma.\n"
    );
    Pagina {
        n,
        texto: linha.repeat(12),
        origem: OrigemTexto::Nativo,
    }
}

#[test]
#[ignore = "timing only; run with --release --ignored --nocapture"]
#[expect(clippy::print_stdout, reason = "opt-in measurement")]
fn measure_montar_estado() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/classifier/vetores.json");
    let v: Value = serde_json::from_slice(&std::fs::read(path).expect("read")).expect("parse");
    let casos = v["montar_estado"].as_array().expect("cases");
    let inicio = Instant::now();
    for caso in casos {
        let entrada: EntradaClassificacao =
            serde_json::from_value(caso["entrada"].clone()).expect("input");
        let modo: PaginasModo = caso["modo"].as_str().expect("mode").parse().expect("mode");
        montar_estado(&entrada, modo).expect("state");
    }
    println!("vectors: {} cases in {:?}", casos.len(), inicio.elapsed());

    let paginas: Vec<Pagina> = (1..=200).map(pagina).collect();
    let caracteres: usize = paginas.iter().map(|p| p.texto.chars().count()).sum();
    let entrada = EntradaClassificacao {
        nome_arquivo: "holter.pdf".to_owned(),
        titulo: "Holter 24 h".to_owned(),
        extensao: "pdf".to_owned(),
        total_paginas: 200,
        paginas,
        nomes_conhecidos: vec!["Zanoni Prado".to_owned()],
    };
    for modo in [PaginasModo::Uma, PaginasModo::Duas, PaginasModo::Todas] {
        let inicio = Instant::now();
        let prep = montar_estado(&entrada, modo).expect("state");
        println!(
            "200 pages ({caracteres} code points), mode {modo}: {:?}; {} pages, {} code points sent, truncado={}",
            inicio.elapsed(),
            prep.paginas_enviadas,
            prep.caracteres_enviados,
            prep.truncado
        );
    }
}
