//! The real Jev (spec/CLASSIFIER.md §14.6): three **synthetic** documents sent to
//! TypeSafe, to check the shape of the answers and their validation. The exact type is
//! not asserted: the model is external. Opt-in, and it sends, so it needs both switches:
//!
//! ```text
//! RUN_JEV_LIVE=1 CLASSIFIER_REMOTE_ALLOWED=1 \
//!     cargo test --features classifier-jev --test classifier_live -- --ignored --nocapture
//! ```
//!
//! The key is `RLCD_API_KEY`, else `TYPESAFE_API_KEY`, from the process or from the
//! crate's `.env` (see `.env.example`). Every text and name below is invented; never put
//! a real exam here.
#![cfg(feature = "classifier-jev")]

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::time::Duration;

use embeddings_fundamentals::classifier::estado::montar_estado;
use embeddings_fundamentals::classifier::jev::{Chave, ClienteJev, modelo_fixo};
use embeddings_fundamentals::classifier::{
    Avaliador, EntradaClassificacao, Envio, EstadoClassificacao, MODELO_PADRAO, OrigemTexto,
    Pagina, PaginasModo, classificar,
};
use embeddings_fundamentals::classify::ambiente_em_camadas;

/// (file name, title, page text) of each synthetic document.
const DOCUMENTOS: [(&str, &str, &str); 3] = [
    (
        "eco_strain.pdf",
        "Ecocardiograma com strain",
        "ECOCARDIOGRAMA TRANSTORACICO COM STRAIN\nPaciente: Nilo Teixeira\n\
         Analise da deformacao miocardica por speckle tracking.\n\
         Strain longitudinal global -19,5 %. FEVE 62 %.\n\
         Conclusao: funcao sistolica preservada, strain dentro da normalidade.",
    ),
    (
        "holter.pdf",
        "Holter 24 horas",
        "HOLTER DE 24 HORAS\nPaciente: Zanoni Prado\n\
         Ritmo sinusal predominante. FC minima 52 bpm, media 71 bpm, maxima 128 bpm.\n\
         Extrassistoles ventriculares isoladas e raras, sem pausas significativas.\n\
         Conclusao: sem arritmias complexas.",
    ),
    (
        "hemograma.pdf",
        "Exame de sangue",
        "HEMOGRAMA COMPLETO\nPaciente: Nilo Teixeira\n\
         Hemoglobina 13,5 g/dL (12,0-16,0). Hematocrito 40,1 %.\n\
         Leucocitos 6.800 /mm3. Plaquetas 250.000 /mm3.\n\
         Glicemia de jejum 92 mg/dL. Creatinina 0,9 mg/dL.",
    ),
];

#[tokio::test]
#[ignore = "sends synthetic text to TypeSafe: RUN_JEV_LIVE=1 CLASSIFIER_REMOTE_ALLOWED=1"]
async fn the_real_jev_answers_in_the_expected_shape() {
    let ligado = |nome: &str| std::env::var(nome).as_deref() == Ok("1");
    assert!(
        ligado("RUN_JEV_LIVE") && ligado("CLASSIFIER_REMOTE_ALLOWED"),
        "set RUN_JEV_LIVE=1 and CLASSIFIER_REMOTE_ALLOWED=1 to send the synthetic documents"
    );
    let raiz: HashMap<String, String> =
        dotenvy::from_path_iter(Path::new(env!("CARGO_MANIFEST_DIR")).join(".env"))
            .map(|itens| itens.filter_map(Result::ok).collect())
            .unwrap_or_default();
    let ambiente = ambiente_em_camadas(|nome| std::env::var(nome).ok(), vec![raiz]);
    let chave = ["RLCD_API_KEY", "TYPESAFE_API_KEY"]
        .into_iter()
        .filter_map(&ambiente)
        .find(|c| !c.is_empty())
        .expect("no key: set TYPESAFE_API_KEY (or RLCD_API_KEY) in the environment or .env");
    let modelo = ambiente("RLCD_MODEL").unwrap_or_else(|| MODELO_PADRAO.to_owned());
    let base = ambiente("RLCD_BASE_URL");
    let cliente = ClienteJev::new(Chave::new(chave).unwrap(), &modelo, base.as_deref())
        .expect("model, key and base URL");
    let avaliador = Avaliador::new(NonZeroUsize::MIN, Duration::from_secs(30));

    for (nome_arquivo, titulo, texto) in DOCUMENTOS {
        let entrada = EntradaClassificacao {
            nome_arquivo: nome_arquivo.to_owned(),
            titulo: titulo.to_owned(),
            extensao: "pdf".to_owned(),
            total_paginas: 1,
            paginas: vec![Pagina {
                n: 1,
                texto: texto.to_owned(),
                origem: OrigemTexto::Nativo,
            }],
            nomes_conhecidos: Vec::new(),
        };
        let prep = montar_estado(&entrada, PaginasModo::Duas).unwrap();
        assert!(
            prep.bloqueio.is_empty() && !prep.sem_conteudo(),
            "{nome_arquivo}"
        );
        let c = classificar(
            &prep,
            0.7,
            Envio::Autorizado {
                provedor: &cliente,
                avaliador: &avaliador,
            },
        )
        .await;
        assert!(
            matches!(
                c.estado,
                EstadoClassificacao::Ok | EstadoClassificacao::Incerto
            ),
            "{nome_arquivo}: {:?} {:?}",
            c.estado,
            c.erro_categoria
        );
        let tipo = c.tipo.unwrap();
        let probabilidades = c.probabilidades.unwrap();
        let soma: f64 = probabilidades.iter().sum();
        assert!((soma - 1.0).abs() <= 1e-3, "{nome_arquivo}: sum {soma}");
        assert_eq!(c.confianca, Some(probabilidades[tipo.indice()]));
        let model = c.model.clone().unwrap();
        assert!(
            modelo_fixo(&model).unwrap(),
            "{nome_arquivo}: echoed {model}"
        );
        assert!(c.uso.is_some_and(|u| u.input_tokens > 0), "{nome_arquivo}");
        eprintln!(
            "{nome_arquivo}: {:?} {} ({:.3}), margin {:.3}, provider confidence {:?}, model {model}, {} ms",
            c.estado,
            tipo.rotulo(),
            c.confianca.unwrap(),
            c.margem.unwrap(),
            c.confianca_provedor,
            c.latencia_ms
        );
    }
}
