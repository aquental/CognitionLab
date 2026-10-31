//! The `classify` command in process, with a double for the extraction
//! (spec/CLASSIFIER.md §10, §12). Every exit code, the OCR and visual-page rules, the
//! double opt-in and the error envelope. Every input is synthetic.
#![expect(clippy::unwrap_used, reason = "test code")]

use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use embeddings_fundamentals::classifier::{
    EstadoClassificacao, MotivoCli, MotivoCodigo, OrigemTexto, PaginasModo,
};
use embeddings_fundamentals::classify::{
    ClassifyArgs, Fonte, LinhaCli, Motivo, executar, texto_de_saida,
};
use embeddings_fundamentals::extraction::ExtractionError;
use serde_json::Value;

const HOLTER: &str = "Holter de 24 horas. Ritmo sinusal predominante, \
    extrassístoles ventriculares isoladas e raras, sem pausas significativas.";

fn falha() -> ExtractionError {
    ExtractionError::Parse {
        what: "fixture",
        reason: "synthetic failure".to_owned(),
    }
}

/// The extraction double: pages, visual flags and OCR answers by page number.
struct FonteFalsa {
    pdf_ok: bool,
    ocr_ok: bool,
    paginas: Option<u32>,
    textos: Vec<Option<String>>,
    visuais: Vec<bool>,
    ocr: Vec<Option<String>>,
    imagem: Result<Option<String>, ()>,
    chamadas_ocr: Mutex<Vec<u32>>,
    conferencias_ocr: Mutex<u32>,
}

impl FonteFalsa {
    fn pdf(textos: &[&str]) -> Self {
        Self {
            pdf_ok: true,
            ocr_ok: true,
            paginas: Some(u32::try_from(textos.len()).unwrap()),
            textos: textos.iter().map(|t| Some((*t).to_owned())).collect(),
            visuais: vec![false; textos.len()],
            ocr: vec![Some("texto do ocr".to_owned()); textos.len()],
            imagem: Ok(Some(HOLTER.to_owned())),
            chamadas_ocr: Mutex::new(Vec::new()),
            conferencias_ocr: Mutex::new(0),
        }
    }

    fn indice(n: NonZeroU32) -> usize {
        usize::try_from(n.get() - 1).unwrap()
    }
}

impl Fonte for FonteFalsa {
    async fn ferramentas_pdf(&self) -> bool {
        self.pdf_ok
    }
    async fn ocr_disponivel(&self, _pdf: bool) -> bool {
        *self.conferencias_ocr.lock().unwrap() += 1;
        self.ocr_ok
    }
    async fn paginas(&self, _pdf: &Path) -> Result<NonZeroU32, ExtractionError> {
        self.paginas.and_then(NonZeroU32::new).ok_or_else(falha)
    }
    async fn texto(&self, _pdf: &Path, n: NonZeroU32) -> Result<String, ExtractionError> {
        self.textos[Self::indice(n)].clone().ok_or_else(falha)
    }
    async fn visual(&self, _pdf: &Path, n: NonZeroU32) -> bool {
        self.visuais[Self::indice(n)]
    }
    async fn ocr_pagina(&self, _pdf: &Path, n: NonZeroU32) -> Result<String, ExtractionError> {
        self.chamadas_ocr.lock().unwrap().push(n.get());
        self.ocr[Self::indice(n)].clone().ok_or_else(falha)
    }
    async fn ocr_imagem(&self, _imagem: &Path) -> Result<Option<String>, ExtractionError> {
        self.imagem.clone().map_err(|()| falha())
    }
}

/// A file in a fresh temporary directory (the double never reads it).
fn arquivo(nome: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let caminho = dir.path().join(nome);
    std::fs::write(&caminho, b"synthetic").unwrap();
    (dir, caminho)
}

fn args(caminho: &Path) -> ClassifyArgs {
    ClassifyArgs {
        arquivo: caminho.to_owned(),
        paginas: None,
        json: true,
        enviar: false,
        limiar: None,
        titulo: None,
        nomes_paciente: Vec::new(),
        mostrar_estado: false,
    }
}

/// Runs the command with these variables, and returns the line and every variable the
/// command looked up.
async fn rodar(
    a: &ClassifyArgs,
    fonte: &FonteFalsa,
    vars: &[(&str, &str)],
) -> (LinhaCli, Vec<String>) {
    let vars: HashMap<String, String> = vars
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    let lidas = RefCell::new(Vec::new());
    let ambiente = |nome: &str| {
        lidas.borrow_mut().push(nome.to_owned());
        vars.get(nome).cloned()
    };
    let linha = executar(a, fonte, &ambiente).await;
    (linha, lidas.into_inner())
}

fn motivo_cli(linha: &LinhaCli) -> Option<MotivoCli> {
    match linha.motivo_codigo {
        Some(Motivo::Cli(m)) => Some(m),
        _ => None,
    }
}

#[tokio::test]
async fn an_invalid_mode_or_threshold_is_code_5_before_anything_else() {
    let (_d, pdf) = arquivo("holter.pdf");
    let fonte = FonteFalsa::pdf(&[HOLTER]);
    for (paginas, env) in [(Some("3"), None), (None, Some("Todas")), (Some(""), None)] {
        let mut a = args(Path::new("/nao/existe.docx"));
        a.paginas = paginas.map(str::to_owned);
        let vars: Vec<(&str, &str)> = env.map(|e| ("CLASSIFIER_PAGINAS", e)).into_iter().collect();
        let (linha, _) = rodar(&a, &fonte, &vars).await;
        assert_eq!(linha.codigo_saida, 5);
        assert_eq!(motivo_cli(&linha), Some(MotivoCli::ConfiguracaoInvalida));
        assert_eq!((linha.paginas_modo, linha.limiar_aplicado), (None, None));
    }
    for limiar in ["1.5", "-0.1", "nan", "inf", "abc", ""] {
        let mut a = args(&pdf);
        a.limiar = Some(limiar.to_owned());
        let (linha, _) = rodar(&a, &fonte, &[]).await;
        assert_eq!(linha.codigo_saida, 5, "{limiar}");
        assert_eq!(linha.paginas_modo, Some(PaginasModo::Duas));
        assert_eq!(linha.limiar_aplicado, None);
    }
    // Python's float(): surrounding spaces and `_` between digits are fine.
    let mut a = args(&pdf);
    a.limiar = Some(" 0.8_5 ".to_owned());
    let (linha, _) = rodar(&a, &fonte, &[("CLASSIFIER_CONFIANCA_MIN", "x")]).await;
    assert_eq!(linha.limiar_aplicado, Some(0.85));
    let (linha, _) = rodar(&args(&pdf), &fonte, &[("CLASSIFIER_CONFIANCA_MIN", "0.9")]).await;
    assert_eq!(linha.limiar_aplicado, Some(0.9));
    let (linha, _) = rodar(&args(&pdf), &fonte, &[]).await;
    assert_eq!(linha.limiar_aplicado, Some(0.70));
}

#[tokio::test]
async fn the_file_must_exist_and_have_a_supported_format() {
    let fonte = FonteFalsa::pdf(&[HOLTER]);
    let (dir, docx) = arquivo("laudo.docx");
    for caminho in [dir.path().join("falta.pdf"), dir.path().to_owned()] {
        let (linha, _) = rodar(&args(&caminho), &fonte, &[]).await;
        assert_eq!(linha.codigo_saida, 2);
        assert_eq!(motivo_cli(&linha), Some(MotivoCli::ArquivoInexistente));
    }
    let (linha, _) = rodar(&args(&docx), &fonte, &[]).await;
    assert_eq!(motivo_cli(&linha), Some(MotivoCli::FormatoNaoSuportado));
    assert_eq!(linha.codigo_saida, 2);
    let (_d, maiusculo) = arquivo("HOLTER.PDF");
    let (linha, _) = rodar(&args(&maiusculo), &fonte, &[]).await;
    assert_eq!(linha.estado, EstadoClassificacao::Preparado);
}

#[tokio::test]
async fn without_both_opt_ins_nothing_about_sending_is_read() {
    let (_d, pdf) = arquivo("holter.pdf");
    let fonte = FonteFalsa::pdf(&[HOLTER]);
    let invalidos = [
        ("RLCD_MODEL", "jev-latest"),
        ("RLCD_BASE_URL", "http://ex.com"),
        ("RLCD_TIMEOUT_S", "x"),
    ];
    for permitido in [None, Some("0"), Some("true"), Some(" 1"), Some("1 ")] {
        let mut a = args(&pdf);
        a.enviar = true;
        let mut vars = invalidos.to_vec();
        vars.extend(permitido.map(|p| ("CLASSIFIER_REMOTE_ALLOWED", p)));
        let (linha, lidas) = rodar(&a, &fonte, &vars).await;
        assert_eq!(
            linha.estado,
            EstadoClassificacao::Preparado,
            "{permitido:?}"
        );
        assert_eq!(linha.codigo_saida, 0);
        assert!(!linha.envio_realizado);
        assert_eq!(
            linha.motivo_codigo,
            Some(Motivo::Classificacao(MotivoCodigo::EnvioNaoAutorizado))
        );
        assert_eq!(linha.model, None);
        assert!(
            lidas
                .iter()
                .all(|v| !v.starts_with("RLCD_") && v != "TYPESAFE_API_KEY"),
            "{lidas:?}"
        );
    }
    // Allowed but not asked for: prepared, without the not-authorized reason.
    let (linha, lidas) = rodar(&args(&pdf), &fonte, &[("CLASSIFIER_REMOTE_ALLOWED", "1")]).await;
    assert_eq!(linha.estado, EstadoClassificacao::Preparado);
    assert_eq!(linha.motivo_codigo, None);
    assert!(lidas.iter().all(|v| !v.starts_with("RLCD_")));
}

#[cfg(feature = "classifier-jev")]
#[tokio::test]
async fn authorized_sending_validates_model_key_url_and_timeout() {
    let (_d, pdf) = arquivo("holter.pdf");
    let fonte = FonteFalsa::pdf(&[HOLTER]);
    let mut a = args(&pdf);
    a.enviar = true;
    let base = [
        ("CLASSIFIER_REMOTE_ALLOWED", "1"),
        ("RLCD_API_KEY", "sk-sintetica"),
    ];
    for (extra, motivo) in [
        (
            vec![("RLCD_MODEL", "jev-latest")],
            MotivoCli::ModeloInvalido,
        ),
        (
            vec![("RLCD_BASE_URL", "http://ex.com")],
            MotivoCli::ConfiguracaoInvalida,
        ),
        (
            vec![("RLCD_TIMEOUT_S", "0")],
            MotivoCli::ConfiguracaoInvalida,
        ),
        (
            vec![("RLCD_TIMEOUT_S", "inf")],
            MotivoCli::ConfiguracaoInvalida,
        ),
        (
            vec![("RLCD_TIMEOUT_S", "x")],
            MotivoCli::ConfiguracaoInvalida,
        ),
    ] {
        let mut vars = base.to_vec();
        vars.extend(extra);
        let (linha, _) = rodar(&a, &fonte, &vars).await;
        assert_eq!(motivo_cli(&linha), Some(motivo), "{vars:?}");
        assert_eq!(linha.codigo_saida, 5);
    }
    let (linha, _) = rodar(
        &a,
        &fonte,
        &[
            ("RLCD_MODEL", "jev-latest"),
            ("CLASSIFIER_REMOTE_ALLOWED", "1"),
        ],
    )
    .await;
    assert_eq!(motivo_cli(&linha), Some(MotivoCli::ModeloInvalido));
    assert_eq!(linha.model.as_deref(), Some("jev-latest"));
    for chaves in [vec![], vec![("RLCD_API_KEY", ""), ("TYPESAFE_API_KEY", "")]] {
        let mut vars = vec![("CLASSIFIER_REMOTE_ALLOWED", "1")];
        vars.extend(chaves);
        let (linha, _) = rodar(&a, &fonte, &vars).await;
        assert_eq!(motivo_cli(&linha), Some(MotivoCli::SemChave));
        assert_eq!(linha.model.as_deref(), Some("jev-1.13.0"));
    }
}

#[cfg(not(feature = "classifier-jev"))]
#[tokio::test]
async fn a_binary_without_the_adapter_refuses_authorized_sending() {
    let (_d, pdf) = arquivo("holter.pdf");
    let mut a = args(&pdf);
    a.enviar = true;
    let (linha, lidas) = rodar(
        &a,
        &FonteFalsa::pdf(&[HOLTER]),
        &[("CLASSIFIER_REMOTE_ALLOWED", "1")],
    )
    .await;
    assert_eq!(motivo_cli(&linha), Some(MotivoCli::ConfiguracaoInvalida));
    assert!(lidas.iter().all(|v| !v.starts_with("RLCD_")));
}

#[tokio::test]
async fn missing_tools_are_code_5() {
    let (_d, pdf) = arquivo("holter.pdf");
    let mut sem_poppler = FonteFalsa::pdf(&[HOLTER]);
    sem_poppler.pdf_ok = false;
    let (linha, _) = rodar(&args(&pdf), &sem_poppler, &[]).await;
    assert_eq!(motivo_cli(&linha), Some(MotivoCli::OcrIndisponivel));
    assert_eq!(linha.codigo_saida, 5);
    let (_d2, png) = arquivo("eco.png");
    let mut sem_ocr = FonteFalsa::pdf(&[HOLTER]);
    sem_ocr.ocr_ok = false;
    let (linha, _) = rodar(&args(&png), &sem_ocr, &[]).await;
    assert_eq!(motivo_cli(&linha), Some(MotivoCli::OcrIndisponivel));
    // In a PDF, tesseract is checked only at the first page that needs OCR.
    let (linha, _) = rodar(&args(&pdf), &sem_ocr, &[]).await;
    assert_eq!(linha.estado, EstadoClassificacao::Preparado);
    assert_eq!(
        *sem_ocr.conferencias_ocr.lock().unwrap(),
        1,
        "only the image run checked"
    );
    let mut vazio_sem_ocr = FonteFalsa::pdf(&[""]);
    vazio_sem_ocr.ocr_ok = false;
    let (linha, _) = rodar(&args(&pdf), &vazio_sem_ocr, &[]).await;
    assert_eq!(motivo_cli(&linha), Some(MotivoCli::OcrIndisponivel));
}

#[tokio::test]
async fn unreadable_inputs_are_code_2() {
    let (_d, pdf) = arquivo("holter.pdf");
    let mut sem_contagem = FonteFalsa::pdf(&[HOLTER]);
    sem_contagem.paginas = None;
    let mut grande = FonteFalsa::pdf(&[HOLTER]);
    grande.paginas = Some(201);
    let mut pagina_ruim = FonteFalsa::pdf(&[HOLTER, HOLTER]);
    pagina_ruim.textos[1] = None;
    for (fonte, motivo) in [
        (&sem_contagem, MotivoCli::ArquivoIlegivel),
        (&grande, MotivoCli::LimitePdf),
        (&pagina_ruim, MotivoCli::ArquivoIlegivel),
    ] {
        let (linha, _) = rodar(&args(&pdf), fonte, &[]).await;
        assert_eq!(motivo_cli(&linha), Some(motivo));
        assert_eq!(linha.codigo_saida, 2);
    }
    let (_d2, png) = arquivo("eco.jpeg");
    for imagem in [Ok(None), Err(())] {
        let mut fonte = FonteFalsa::pdf(&[]);
        fonte.imagem = imagem;
        let (linha, _) = rodar(&args(&png), &fonte, &[]).await;
        assert_eq!(motivo_cli(&linha), Some(MotivoCli::ArquivoIlegivel));
    }
}

#[tokio::test]
async fn ocr_runs_on_empty_or_sparse_visual_selected_pages() {
    let (_d, pdf) = arquivo("holter.pdf");
    let longo = format!("{HOLTER} {HOLTER} {HOLTER}");
    let mut fonte = FonteFalsa::pdf(&["", "  curto  ", &longo, "curto", ""]);
    fonte.visuais = vec![false, true, true, false, false];
    fonte.ocr[1] = Some("  Holter 24 h  \n".to_owned());
    let mut a = args(&pdf);
    a.paginas = Some("todas".to_owned());
    a.mostrar_estado = true;
    let (linha, _) = rodar(&a, &fonte, &[]).await;
    // Page 1: empty, OCR. Page 2: visual and sparse, OCR joined after the stripped
    // native text. Page 3: visual but long. Page 4: sparse but not visual. Page 5: empty.
    assert_eq!(*fonte.chamadas_ocr.lock().unwrap(), [1, 2, 5]);
    assert_eq!(
        linha.origens_texto,
        [OrigemTexto::Nativo, OrigemTexto::OcrLocal]
    );
    let estado = linha.estado_preparado.unwrap();
    assert_eq!(estado.paginas[0].texto, "texto do ocr");
    assert_eq!(estado.paginas[1].texto, "curto\nHolter 24 h");
    assert_eq!(estado.paginas[3].texto, "curto");
    assert_eq!(estado.arquivo.total_paginas, 5);

    // Only the selected pages are read by OCR, and an OCR failure keeps the page.
    let mut fonte = FonteFalsa::pdf(&["", "", ""]);
    fonte.ocr[0] = None;
    let mut a = args(&pdf);
    a.paginas = Some("2".to_owned());
    let (linha, _) = rodar(&a, &fonte, &[]).await;
    assert_eq!(*fonte.chamadas_ocr.lock().unwrap(), [1, 2]);
    assert_eq!(linha.origens_texto, [OrigemTexto::OcrLocal]);
    assert_eq!(linha.total_paginas, Some(3));
}

#[tokio::test]
async fn each_state_has_its_code_and_the_masked_state_only_when_safe() {
    let (_d, pdf) = arquivo("holter_24h.pdf");
    let mut a = args(&pdf);
    a.mostrar_estado = true;
    let (linha, _) = rodar(&a, &FonteFalsa::pdf(&[HOLTER]), &[]).await;
    assert_eq!(
        (linha.estado, linha.codigo_saida),
        (EstadoClassificacao::Preparado, 0)
    );
    assert_eq!(linha.termos_nome_arquivo, ["holter"]);
    assert_eq!(linha.latencia_ms, Some(0));
    let estado = linha.estado_preparado.clone().unwrap();
    let json: Value = serde_json::from_str(&texto_de_saida(&linha, true).unwrap()).unwrap();
    // The masked state is printed in wire order (preserve_order is on in tests).
    assert_eq!(
        serde_json::to_string(&json["estado_preparado"]).unwrap(),
        estado.json_fio()
    );

    // A9: a known name that is also a file-name term blocks; the state is absent.
    let (_d2, eco) = arquivo("eco_mapa.pdf");
    let mut a = args(&eco);
    a.mostrar_estado = true;
    a.nomes_paciente = vec!["Joana Mapa".to_owned()];
    let (linha, _) = rodar(&a, &FonteFalsa::pdf(&[HOLTER]), &[]).await;
    assert_eq!(
        (linha.estado, linha.codigo_saida),
        (EstadoClassificacao::BloqueadoPrivacidade, 6)
    );
    assert!(linha.estado_preparado.is_none());
    assert!(
        linha
            .mascaramentos
            .contains_key("conferencia_nome_conhecido")
    );
    assert!(
        !texto_de_saida(&linha, true)
            .unwrap()
            .contains("estado_preparado")
    );
    assert!(
        !texto_de_saida(&linha, false)
            .unwrap()
            .contains("estado_preparado")
    );

    // No content: code 3, no state.
    let mut a = args(&pdf);
    a.mostrar_estado = true;
    let mut vazio = FonteFalsa::pdf(&[" "]);
    vazio.ocr[0] = Some(String::new());
    let (linha, _) = rodar(&a, &vazio, &[]).await;
    assert_eq!(
        (linha.estado, linha.codigo_saida),
        (EstadoClassificacao::SemConteudo, 3)
    );
    assert!(linha.estado_preparado.is_none());
    let (_d3, png) = arquivo("eco.png");
    let mut imagem_vazia = FonteFalsa::pdf(&[]);
    imagem_vazia.imagem = Ok(Some(" \n".to_owned()));
    let (linha, _) = rodar(&args(&png), &imagem_vazia, &[]).await;
    assert_eq!(linha.codigo_saida, 3);
    // A blank OCR text is no page at all (TD §5.2): `total_paginas` is 0.
    assert_eq!(linha.total_paginas, Some(0));
    assert_eq!(
        linha.motivo_codigo,
        Some(Motivo::Classificacao(MotivoCodigo::SemConteudo))
    );

    // An error before the classification never carries the state.
    let mut a = args(&pdf);
    a.mostrar_estado = true;
    a.limiar = Some("2".to_owned());
    let (linha, _) = rodar(&a, &FonteFalsa::pdf(&[HOLTER]), &[]).await;
    assert!(linha.estado_preparado.is_none());
}

#[tokio::test]
async fn the_line_has_the_keys_in_order_and_the_envelope_is_complete() {
    let (_d, pdf) = arquivo("holter.pdf");
    let (linha, _) = rodar(&args(&pdf), &FonteFalsa::pdf(&[HOLTER]), &[]).await;
    let texto = texto_de_saida(&linha, true).unwrap();
    assert!(!texto.contains('\n'));
    let json: Value = serde_json::from_str(&texto).unwrap();
    let chaves: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        chaves,
        [
            "estado",
            "codigo_saida",
            "envio_realizado",
            "provider",
            "model",
            "adapter_version",
            "perguntas_versao",
            "mascaramento_versao",
            "origens_texto",
            "paginas_modo",
            "total_paginas",
            "paginas_enviadas",
            "caracteres_enviados",
            "truncado",
            "mascaramentos",
            "termos_nome_arquivo",
            "entrada_digest",
            "tipo",
            "rotulo",
            "confianca",
            "margem",
            "limiar_aplicado",
            "probabilidades",
            "confianca_provedor",
            "erro_categoria",
            "motivo_codigo",
            "latencia_ms",
            "provider_request_id",
            "uso",
        ]
    );

    let (linha, _) = rodar(
        &args(Path::new("/nao/existe.pdf")),
        &FonteFalsa::pdf(&[]),
        &[],
    )
    .await;
    let json: Value = serde_json::from_str(&texto_de_saida(&linha, true).unwrap()).unwrap();
    assert_eq!(json["estado"], "erro");
    assert_eq!(json["codigo_saida"], 2);
    assert_eq!(json["motivo_codigo"], "arquivo_inexistente");
    assert_eq!(json["envio_realizado"], false);
    assert_eq!(json["provider"], "typesafe");
    assert_eq!(json["adapter_version"], "cognitionlab-jev-http/1");
    assert_eq!(json["mascaramento_versao"], "mascaramento-5+cognitionlab-1");
    assert_eq!(json["paginas_modo"], "2");
    assert_eq!(json["limiar_aplicado"], 0.7);
    assert_eq!(json["probabilidades"], serde_json::json!({}));
    assert_eq!(json["mascaramentos"], serde_json::json!({}));
    assert_eq!(json["origens_texto"], serde_json::json!([]));
    assert_eq!(json["termos_nome_arquivo"], serde_json::json!([]));
    for nulo in [
        "model",
        "total_paginas",
        "paginas_enviadas",
        "caracteres_enviados",
        "truncado",
        "entrada_digest",
        "tipo",
        "rotulo",
        "confianca",
        "margem",
        "confianca_provedor",
        "erro_categoria",
        "latencia_ms",
        "provider_request_id",
        "uso",
    ] {
        assert!(json[nulo].is_null(), "{nulo}");
    }
}

#[tokio::test]
async fn neither_output_mode_carries_the_path_or_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let pasta = dir.path().join("Paciente Sintetico Zanoni Prado");
    std::fs::create_dir(&pasta).unwrap();
    let pdf = pasta.join("Zanoni_Prado_holter.pdf");
    std::fs::write(&pdf, b"synthetic").unwrap();
    let texto = "Paciente: Zanoni Prado\nHolter de 24 horas, ritmo sinusal, sem pausas.";
    for fonte in [FonteFalsa::pdf(&[texto]), {
        let mut f = FonteFalsa::pdf(&[texto]);
        f.paginas = None;
        f
    }] {
        let (linha, _) = rodar(&args(&pdf), &fonte, &[]).await;
        for json in [true, false] {
            let saida = texto_de_saida(&linha, json).unwrap();
            for proibido in ["Zanoni", "Prado", "Paciente Sintetico", "ritmo sinusal"] {
                assert!(!saida.contains(proibido), "{proibido} in {saida}");
            }
        }
    }
}

#[cfg(feature = "classifier-jev")]
mod com_servidor {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn servidor(status: &'static str, corpo: String) -> String {
        let escuta = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", escuta.local_addr().unwrap().port());
        tokio::spawn(async move {
            let (mut s, _) = escuta.accept().await.unwrap();
            let mut lido = Vec::new();
            let mut buf = [0_u8; 8192];
            // Read until the body announced by content-length is in.
            loop {
                let n = s.read(&mut buf).await.unwrap();
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
            let resposta = format!(
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{corpo}",
                corpo.len()
            );
            s.write_all(resposta.as_bytes()).await.unwrap();
        });
        base
    }

    fn corpo_ok() -> String {
        let mut p = serde_json::Map::new();
        for c in embeddings_fundamentals::classifier::Categoria::TODAS {
            let v = if c.valor() == "holter" { 0.92 } else { 0.01 };
            p.insert(c.valor().to_owned(), serde_json::json!(v));
        }
        serde_json::json!({"model": "jev-1.13.0", "usage": {"input_tokens": 10, "output_tokens": 1},
            "answers": {"tipo_documento": {"type": "choice", "choice": "holter",
                                           "confidence": 0.9, "probabilities": p}}})
        .to_string()
    }

    #[tokio::test]
    async fn with_the_local_server_the_command_classifies() {
        let (_d, pdf) = arquivo("holter.pdf");
        let fonte = FonteFalsa::pdf(&[HOLTER]);
        let mut a = args(&pdf);
        a.enviar = true;
        for (status, corpo, estado, codigo) in [
            ("200 OK", corpo_ok(), EstadoClassificacao::Ok, 0),
            (
                "429 Too Many Requests",
                String::new(),
                EstadoClassificacao::Erro,
                4,
            ),
        ] {
            let base = servidor(status, corpo).await;
            let vars = [
                ("CLASSIFIER_REMOTE_ALLOWED", "1"),
                ("TYPESAFE_API_KEY", "sk-sintetica"),
                ("RLCD_BASE_URL", base.as_str()),
            ];
            let (linha, _) = rodar(&a, &fonte, &vars).await;
            assert_eq!(
                (linha.estado, linha.codigo_saida),
                (estado, codigo),
                "{status}"
            );
            assert!(linha.envio_realizado);
            if estado == EstadoClassificacao::Ok {
                assert_eq!(linha.rotulo, Some("Holter"));
                assert_eq!(linha.confianca, Some(0.92));
            }
        }
    }
}
