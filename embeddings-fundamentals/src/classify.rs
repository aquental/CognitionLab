//! The `classify` command (spec/CLASSIFIER.md §10, §12): extract a PDF or an image,
//! build the masked state and classify it, sending it to Jev only with `--enviar` and
//! `CLASSIFIER_REMOTE_ALLOWED=1` (R-P1).
//!
//! [`executar`] is the whole command except printing. It takes the extraction as a
//! [`Fonte`] (poppler and tesseract in the binary, a double in tests) and the
//! environment as a lookup function, so every exit code is testable in process. Nothing
//! it returns or logs carries the file path, the exam text or the key (R-P2, R-P3).

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::Args;
use serde::{Serialize, Serializer};
use tokio::process::Command;

use crate::classifier::decisao::float_de_texto;
use crate::classifier::estado::montar_estado;
use crate::classifier::provedor::{ADAPTER_VERSION, PROVIDER};
use crate::classifier::texto::strip;
use crate::classifier::{
    Categoria, Classificacao, EntradaClassificacao, ErroCategoria, EstadoClassificacao,
    EstadoEnviado, MASCARAMENTO_VERSAO_PORTE, MotivoCli, MotivoCodigo, OrigemTexto,
    PERGUNTAS_VERSAO, Pagina, PaginasModo, Uso, serializar_probabilidades,
};
use crate::extraction::poppler::{PopplerTextExtractor, TesseractOcr};
use crate::extraction::{ExtractionConfig, ExtractionError, OcrEngine, TextExtractor};
use crate::tools::{self, Tool};

/// Most PDF pages read (TestoDoc `MAX_PDF_PAGES`).
pub const LIMITE_PAGINAS_PDF: u32 = 200;
/// A visual page with fewer native code points than this is also read by OCR
/// (TestoDoc `TEXT_SPARSE_CHARS`).
const TEXTO_ESPARSO: usize = 220;
/// A page is visual when its images cover this share of it (`MIN_IMAGE_AREA_RATIO`).
const AREA_VISUAL: f64 = 0.02;
const PRAZO_FERRAMENTA: Duration = Duration::from_secs(60);

/// Arguments of `classify`. `--paginas` and `--limiar` are text, so that an invalid
/// value is the command's own error (code 5), not clap's.
#[derive(Debug, Clone, Args)]
pub struct ClassifyArgs {
    /// The PDF, PNG or JPEG to classify (an anonymized copy, spec/CLASSIFIER.md §3.1).
    pub arquivo: PathBuf,
    /// Pages sent: `1`, `2` or `todas` (default `CLASSIFIER_PAGINAS`, else `2`).
    #[arg(long)]
    pub paginas: Option<String>,
    /// Exactly one JSON line on stdout.
    #[arg(long)]
    pub json: bool,
    /// Send the masked state to Jev; needs `CLASSIFIER_REMOTE_ALLOWED=1` too.
    #[arg(long)]
    pub enviar: bool,
    /// Confidence threshold in [0, 1] (default `CLASSIFIER_CONFIANCA_MIN`, else 0.70).
    #[arg(long)]
    pub limiar: Option<String>,
    /// Document title (default: the file name without its extension).
    #[arg(long)]
    pub titulo: Option<String>,
    /// A name to remove (repeatable).
    #[arg(long = "nome-paciente")]
    pub nomes_paciente: Vec<String>,
    /// Add the masked state to the output; synthetic inputs only (R-P4).
    #[arg(long)]
    pub mostrar_estado: bool,
}

/// The extraction the command needs (spec/CLASSIFIER.md §10.1 step 4, §10.2, §10.4).
pub trait Fonte: Send + Sync {
    /// `pdfinfo`, `pdftotext` and `pdfimages` answer.
    fn ferramentas_pdf(&self) -> impl Future<Output = bool> + Send;
    /// `tesseract` answers and has the `por` language; with `pdf`, `pdftoppm` too.
    fn ocr_disponivel(&self, pdf: bool) -> impl Future<Output = bool> + Send;
    /// Number of pages of the PDF.
    fn paginas(
        &self,
        pdf: &Path,
    ) -> impl Future<Output = Result<NonZeroU32, ExtractionError>> + Send;
    /// Native text of one page.
    fn texto(
        &self,
        pdf: &Path,
        n: NonZeroU32,
    ) -> impl Future<Output = Result<String, ExtractionError>> + Send;
    /// Whether the page is visual (§10.4); a failed inspection is visual.
    fn visual(&self, pdf: &Path, n: NonZeroU32) -> impl Future<Output = bool> + Send;
    /// OCR of one page.
    fn ocr_pagina(
        &self,
        pdf: &Path,
        n: NonZeroU32,
    ) -> impl Future<Output = Result<String, ExtractionError>> + Send;
    /// Decodes the image, then reads it by OCR. `Ok(None)`: it does not decode.
    fn ocr_imagem(
        &self,
        imagem: &Path,
    ) -> impl Future<Output = Result<Option<String>, ExtractionError>> + Send;
}

/// The reason in an output line: the classification's, or the CLI's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Motivo {
    /// From [`Classificacao::motivo_codigo`].
    Classificacao(MotivoCodigo),
    /// From a stop before the classification.
    Cli(MotivoCli),
}

/// The output line (spec/CLASSIFIER.md §10.3), keys in this order. Fields that exist
/// only after the classification are `None` in the error envelope.
#[derive(Debug, Clone, Serialize)]
pub struct LinhaCli {
    /// Final state.
    pub estado: EstadoClassificacao,
    /// The process exit code.
    pub codigo_saida: u8,
    /// Whether the state reached the provider.
    pub envio_realizado: bool,
    /// Always `typesafe`.
    pub provider: &'static str,
    /// The model, once sending was authorized and configured.
    pub model: Option<String>,
    /// Adapter identity.
    pub adapter_version: &'static str,
    /// Question version.
    pub perguntas_versao: &'static str,
    /// Masking version (DV10).
    pub mascaramento_versao: &'static str,
    /// Text origins of the sent pages.
    pub origens_texto: Vec<OrigemTexto>,
    /// Pages mode, after step 1.
    pub paginas_modo: Option<PaginasModo>,
    /// Pages of the input.
    pub total_paginas: Option<u32>,
    /// Pages sent.
    pub paginas_enviadas: Option<u32>,
    /// Code points sent.
    pub caracteres_enviados: Option<u32>,
    /// Some page or the total was cut.
    pub truncado: Option<bool>,
    /// Removals per layer, and the final check's findings.
    pub mascaramentos: BTreeMap<String, u32>,
    /// File-name terms from the type vocabulary (never a name).
    pub termos_nome_arquivo: Vec<String>,
    /// Hex digest of the state.
    pub entrada_digest: Option<String>,
    /// Chosen type.
    pub tipo: Option<Categoria>,
    /// Its label.
    pub rotulo: Option<&'static str>,
    /// Probability of the chosen type.
    pub confianca: Option<f64>,
    /// Top-two difference.
    pub margem: Option<f64>,
    /// Threshold, after step 1.
    pub limiar_aplicado: Option<f64>,
    /// The nine probabilities, `{}` without an answer.
    #[serde(serialize_with = "serializar_probabilidades")]
    pub probabilidades: Option<[f64; 9]>,
    /// Jev's `confidence` (DV7).
    pub confianca_provedor: Option<f64>,
    /// Why the call failed.
    pub erro_categoria: Option<ErroCategoria>,
    /// Stable reason.
    pub motivo_codigo: Option<Motivo>,
    /// Call duration.
    pub latencia_ms: Option<u64>,
    /// Provider request id.
    pub provider_request_id: Option<String>,
    /// Token usage.
    pub uso: Option<Uso>,
    /// The masked state in wire order, only with `--mostrar-estado` and only when it
    /// holds nothing the final check found (TP `classificar_documento.py:369`).
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serializar_estado"
    )]
    pub estado_preparado: Option<EstadoEnviado>,
}

#[derive(Serialize)]
struct ArquivoFio<'a> {
    termos_nome_arquivo: &'a [String],
    termos_titulo: &'a [String],
    extensao: &'a str,
    total_paginas: u32,
}

#[derive(Serialize)]
struct PaginaFio<'a> {
    n: u32,
    texto: &'a str,
}

/// The wire order of [`EstadoEnviado::json_fio`]: struct fields serialize in
/// declaration order, whatever `serde_json`'s features.
#[derive(Serialize)]
struct EstadoFio<'a> {
    arquivo: ArquivoFio<'a>,
    paginas_modo: PaginasModo,
    paginas: Vec<PaginaFio<'a>>,
}

fn serializar_estado<S: Serializer>(
    estado: &Option<EstadoEnviado>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    estado
        .as_ref()
        .map(|e| EstadoFio {
            arquivo: ArquivoFio {
                termos_nome_arquivo: &e.arquivo.termos_nome_arquivo,
                termos_titulo: &e.arquivo.termos_titulo,
                extensao: &e.arquivo.extensao,
                total_paginas: e.arquivo.total_paginas,
            },
            paginas_modo: e.paginas_modo,
            paginas: e
                .paginas
                .iter()
                .map(|p| PaginaFio {
                    n: p.n,
                    texto: &p.texto,
                })
                .collect(),
        })
        .serialize(serializer)
}

/// Exit code of a CLI stop (TD §5.4).
#[must_use]
pub const fn codigo_cli(motivo: MotivoCli) -> u8 {
    match motivo {
        MotivoCli::ErroInterno => 1,
        MotivoCli::ArquivoInexistente
        | MotivoCli::FormatoNaoSuportado
        | MotivoCli::LimitePdf
        | MotivoCli::ArquivoIlegivel => 2,
        MotivoCli::ConfiguracaoInvalida
        | MotivoCli::SemChave
        | MotivoCli::ModeloInvalido
        | MotivoCli::OcrIndisponivel => 5,
    }
}

/// Exit code of a classification state.
#[must_use]
pub const fn codigo_estado(estado: EstadoClassificacao) -> u8 {
    match estado {
        EstadoClassificacao::Ok | EstadoClassificacao::Incerto | EstadoClassificacao::Preparado => {
            0
        }
        EstadoClassificacao::SemConteudo => 3,
        EstadoClassificacao::Erro => 4,
        EstadoClassificacao::BloqueadoPrivacidade => 6,
    }
}

/// The fixed phrase of a reason: never a path, an exception text or exam text (DV9).
#[must_use]
pub const fn frase(motivo: Motivo) -> &'static str {
    match motivo {
        Motivo::Cli(MotivoCli::ConfiguracaoInvalida) => "invalid configuration",
        Motivo::Cli(MotivoCli::SemChave) => "no API key (RLCD_API_KEY or TYPESAFE_API_KEY)",
        Motivo::Cli(MotivoCli::ModeloInvalido) => "RLCD_MODEL is not an exact Jev version",
        Motivo::Cli(MotivoCli::ArquivoInexistente) => "the input file does not exist",
        Motivo::Cli(MotivoCli::FormatoNaoSuportado) => "only PDF, PNG and JPEG are supported",
        Motivo::Cli(MotivoCli::LimitePdf) => "the PDF has more than 200 pages",
        Motivo::Cli(MotivoCli::ArquivoIlegivel) => "the input file could not be read",
        Motivo::Cli(MotivoCli::OcrIndisponivel) => {
            "poppler or tesseract (with Portuguese) is not installed"
        }
        Motivo::Cli(MotivoCli::ErroInterno) => "internal error",
        Motivo::Classificacao(MotivoCodigo::SemConteudo) => "no text in the selected pages",
        Motivo::Classificacao(MotivoCodigo::TextoInsuficiente) => {
            "too little text and no exam-type term"
        }
        Motivo::Classificacao(MotivoCodigo::ConferenciaPrivacidade) => {
            "the final privacy check found personal data; nothing was sent"
        }
        Motivo::Classificacao(MotivoCodigo::ProvedorErro) => "the provider call failed",
        Motivo::Classificacao(MotivoCodigo::EnvioNaoAutorizado) => {
            "sending needs CLASSIFIER_REMOTE_ALLOWED=1; the state was only prepared"
        }
    }
}

/// What step 1 established before a stop.
#[derive(Debug, Clone, Copy, Default)]
struct Passo1 {
    modo: Option<PaginasModo>,
    limiar: Option<f64>,
}

fn envelope(motivo: MotivoCli, passo1: Passo1, model: Option<String>) -> LinhaCli {
    LinhaCli {
        estado: EstadoClassificacao::Erro,
        codigo_saida: codigo_cli(motivo),
        envio_realizado: false,
        provider: PROVIDER,
        model,
        adapter_version: ADAPTER_VERSION,
        perguntas_versao: PERGUNTAS_VERSAO,
        mascaramento_versao: MASCARAMENTO_VERSAO_PORTE,
        origens_texto: Vec::new(),
        paginas_modo: passo1.modo,
        total_paginas: None,
        paginas_enviadas: None,
        caracteres_enviados: None,
        truncado: None,
        mascaramentos: BTreeMap::new(),
        termos_nome_arquivo: Vec::new(),
        entrada_digest: None,
        tipo: None,
        rotulo: None,
        confianca: None,
        margem: None,
        limiar_aplicado: passo1.limiar,
        probabilidades: None,
        confianca_provedor: None,
        erro_categoria: None,
        motivo_codigo: Some(Motivo::Cli(motivo)),
        latencia_ms: None,
        provider_request_id: None,
        uso: None,
        estado_preparado: None,
    }
}

fn linha(c: Classificacao, termos: Vec<String>, estado: Option<EstadoEnviado>) -> LinhaCli {
    let envio_realizado = matches!(
        c.estado,
        EstadoClassificacao::Ok | EstadoClassificacao::Incerto
    ) || c.motivo_codigo == Some(MotivoCodigo::ProvedorErro);
    LinhaCli {
        estado: c.estado,
        codigo_saida: codigo_estado(c.estado),
        envio_realizado,
        provider: c.provider,
        model: c.model,
        adapter_version: c.adapter_version,
        perguntas_versao: c.perguntas_versao,
        mascaramento_versao: c.mascaramento_versao,
        origens_texto: c.origens_texto,
        paginas_modo: Some(c.paginas_modo),
        total_paginas: Some(c.total_paginas),
        paginas_enviadas: Some(c.paginas_enviadas),
        caracteres_enviados: Some(c.caracteres_enviados),
        truncado: Some(c.truncado),
        mascaramentos: c.mascaramentos,
        termos_nome_arquivo: termos,
        entrada_digest: Some(c.entrada_digest),
        tipo: c.tipo,
        rotulo: c.tipo.map(Categoria::rotulo),
        confianca: c.confianca,
        margem: c.margem,
        limiar_aplicado: Some(c.limiar_aplicado),
        probabilidades: c.probabilidades,
        confianca_provedor: c.confianca_provedor,
        erro_categoria: c.erro_categoria,
        motivo_codigo: c.motivo_codigo.map(Motivo::Classificacao),
        latencia_ms: Some(c.latencia_ms),
        provider_request_id: c.provider_request_id,
        uso: c.uso,
        estado_preparado: estado,
    }
}

/// `CLASSIFIER_CONFIANCA_MIN` or `--limiar`: Python's `float()`, finite, in [0, 1].
fn limiar_valido(texto: &str) -> Option<f64> {
    float_de_texto(texto).filter(|l| l.is_finite() && (0.0..=1.0).contains(l))
}

/// Supported input kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Formato {
    Pdf,
    Imagem,
}

/// The provider, once sending is authorized (step 3).
#[cfg(feature = "classifier-jev")]
type Provedor = crate::classifier::jev::ClienteJev;

/// Without the adapter there is no provider type to name; this one is never built.
#[cfg(not(feature = "classifier-jev"))]
type Provedor = SemAdaptador;

/// The provider of a binary built without `classifier-jev`: never constructed.
#[cfg(not(feature = "classifier-jev"))]
#[derive(Debug)]
pub enum SemAdaptador {}

#[cfg(not(feature = "classifier-jev"))]
impl crate::classifier::RlcdProvider for SemAdaptador {
    fn provider(&self) -> &'static str {
        match *self {}
    }
    fn model(&self) -> &str {
        match *self {}
    }
    fn adapter_version(&self) -> &'static str {
        match *self {}
    }
    async fn evaluate(
        &self,
        _req: &crate::classifier::RequestNeutro<'_>,
    ) -> crate::classifier::RespostaProvedor {
        match *self {}
    }
}

/// Step 3's outcome: the provider, its deadline and the model; or the stop, with the
/// model when it was read.
type Configurado = Result<(Provedor, Duration, String), (MotivoCli, Option<String>)>;

/// Step 3: model, key, base URL and timeout, read only now (R-P1, invariant 7).
#[cfg(feature = "classifier-jev")]
fn configurar_envio(ambiente: &dyn Fn(&str) -> Option<String>) -> Configurado {
    use crate::classifier::jev::{Chave, ClienteJev, ErroConfiguracao, modelo_fixo};
    let model =
        ambiente("RLCD_MODEL").unwrap_or_else(|| crate::classifier::MODELO_PADRAO.to_owned());
    let falha = |motivo: MotivoCli| -> Configurado { Err((motivo, Some(model.clone()))) };
    if !modelo_fixo(&model).unwrap_or(false) {
        return falha(MotivoCli::ModeloInvalido);
    }
    let chave = ["RLCD_API_KEY", "TYPESAFE_API_KEY"]
        .into_iter()
        .filter_map(ambiente)
        .find(|c| !c.is_empty());
    let Some(chave) = chave.and_then(|c| Chave::new(c).ok()) else {
        return falha(MotivoCli::SemChave);
    };
    let base = ambiente("RLCD_BASE_URL");
    if crate::classifier::jev::url_do_endpoint(base.as_deref()).is_err() {
        return falha(MotivoCli::ConfiguracaoInvalida);
    }
    let prazo = ambiente("RLCD_TIMEOUT_S")
        .map_or(Some(20.0), |t| float_de_texto(&t))
        .filter(|t| t.is_finite() && *t > 0.0)
        .and_then(|t| Duration::try_from_secs_f64(t).ok());
    let Some(prazo) = prazo else {
        return falha(MotivoCli::ConfiguracaoInvalida);
    };
    match ClienteJev::new(chave, &model, base.as_deref()) {
        Ok(cliente) => Ok((cliente, prazo, model)),
        Err(ErroConfiguracao::ModeloInvalido) => falha(MotivoCli::ModeloInvalido),
        Err(ErroConfiguracao::SemChave) => falha(MotivoCli::SemChave),
        Err(ErroConfiguracao::BaseUrlInvalida) => falha(MotivoCli::ConfiguracaoInvalida),
        Err(ErroConfiguracao::Cliente) => falha(MotivoCli::ErroInterno),
    }
}

/// A binary without `classifier-jev` cannot send: authorized sending is a
/// configuration error, and no variable is read.
#[cfg(not(feature = "classifier-jev"))]
fn configurar_envio(_ambiente: &dyn Fn(&str) -> Option<String>) -> Configurado {
    Err((MotivoCli::ConfiguracaoInvalida, None))
}

/// Step 5 for a PDF. `Err` is the CLI stop.
async fn extrair_pdf<F: Fonte>(
    fonte: &F,
    pdf: &Path,
    modo: PaginasModo,
) -> Result<Vec<Pagina>, MotivoCli> {
    let total = fonte
        .paginas(pdf)
        .await
        .map_err(|_| MotivoCli::ArquivoIlegivel)?;
    if total.get() > LIMITE_PAGINAS_PDF {
        return Err(MotivoCli::LimitePdf);
    }
    let selecionadas = modo.limite().map_or(total.get(), |k| {
        u32::try_from(k).unwrap_or(u32::MAX).min(total.get())
    });
    let mut paginas = Vec::new();
    let mut ocr_conferido = false;
    for n in (1..=total.get()).filter_map(NonZeroU32::new) {
        let texto = fonte
            .texto(pdf, n)
            .await
            .map_err(|_| MotivoCli::ArquivoIlegivel)?;
        let mut pagina = Pagina {
            n: n.get(),
            texto,
            origem: OrigemTexto::Nativo,
        };
        if n.get() <= selecionadas {
            let nativo = strip(&pagina.texto).to_owned();
            let precisa = nativo.is_empty()
                || (nativo.chars().count() < TEXTO_ESPARSO && fonte.visual(pdf, n).await);
            if precisa {
                if !ocr_conferido {
                    if !fonte.ocr_disponivel(true).await {
                        return Err(MotivoCli::OcrIndisponivel);
                    }
                    ocr_conferido = true;
                }
                // An OCR failure leaves the page as it was (TP: `ValueError` → `continue`).
                if let Ok(ocr) = fonte.ocr_pagina(pdf, n).await {
                    let ocr = strip(&ocr);
                    if !ocr.is_empty() {
                        pagina.texto = [nativo.as_str(), ocr]
                            .into_iter()
                            .filter(|t| !t.is_empty())
                            .collect::<Vec<_>>()
                            .join("\n");
                        pagina.origem = OrigemTexto::OcrLocal;
                    }
                }
            }
        }
        paginas.push(pagina);
    }
    Ok(paginas)
}

/// The command without printing (spec/CLASSIFIER.md §10.1). `ambiente` looks up a
/// variable; it is called for the sending variables only when sending is authorized.
pub async fn executar<F: Fonte>(
    args: &ClassifyArgs,
    fonte: &F,
    ambiente: &dyn Fn(&str) -> Option<String>,
) -> LinhaCli {
    // 1. Pages mode and threshold.
    let mut passo1 = Passo1::default();
    let modo = args
        .paginas
        .clone()
        .or_else(|| ambiente("CLASSIFIER_PAGINAS"))
        .unwrap_or_else(|| "2".to_owned());
    let Ok(modo) = modo.parse::<PaginasModo>() else {
        return envelope(MotivoCli::ConfiguracaoInvalida, passo1, None);
    };
    passo1.modo = Some(modo);
    let limiar = args
        .limiar
        .clone()
        .or_else(|| ambiente("CLASSIFIER_CONFIANCA_MIN"))
        .unwrap_or_else(|| "0.70".to_owned());
    let Some(limiar) = limiar_valido(&limiar) else {
        return envelope(MotivoCli::ConfiguracaoInvalida, passo1, None);
    };
    passo1.limiar = Some(limiar);

    // 2. The file and its format.
    let existe = tokio::fs::metadata(&args.arquivo)
        .await
        .is_ok_and(|m| m.is_file());
    if !existe {
        return envelope(MotivoCli::ArquivoInexistente, passo1, None);
    }
    let extensao = args
        .arquivo
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    let formato = match extensao.as_str() {
        "pdf" => Formato::Pdf,
        "png" | "jpg" | "jpeg" => Formato::Imagem,
        _ => return envelope(MotivoCli::FormatoNaoSuportado, passo1, None),
    };

    // 3. Sending: authorized only by both opt-ins; nothing else is read otherwise.
    let autorizado = args.enviar && ambiente("CLASSIFIER_REMOTE_ALLOWED").as_deref() == Some("1");
    let envio = if autorizado {
        match configurar_envio(ambiente) {
            Ok(envio) => Some(envio),
            Err((motivo, model)) => return envelope(motivo, passo1, model),
        }
    } else {
        None
    };
    let model = envio.as_ref().map(|(_, _, model)| model.clone());

    // 4. Tools, then 5. extraction.
    let paginas = match formato {
        Formato::Pdf => {
            if !fonte.ferramentas_pdf().await {
                return envelope(MotivoCli::OcrIndisponivel, passo1, model);
            }
            match extrair_pdf(fonte, &args.arquivo, modo).await {
                Ok(paginas) => paginas,
                Err(motivo) => return envelope(motivo, passo1, model),
            }
        }
        Formato::Imagem => {
            if !fonte.ocr_disponivel(false).await {
                return envelope(MotivoCli::OcrIndisponivel, passo1, model);
            }
            match fonte.ocr_imagem(&args.arquivo).await {
                Ok(Some(texto)) if !strip(&texto).is_empty() => vec![Pagina {
                    n: 1,
                    texto,
                    origem: OrigemTexto::OcrLocal,
                }],
                Ok(Some(_)) => Vec::new(),
                Ok(None) | Err(_) => {
                    return envelope(MotivoCli::ArquivoIlegivel, passo1, model);
                }
            }
        }
    };

    // 6. State and classification.
    let nome_arquivo = args
        .arquivo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let titulo = args.titulo.clone().unwrap_or_else(|| {
        args.arquivo
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let entrada = EntradaClassificacao {
        nome_arquivo,
        titulo,
        extensao,
        total_paginas: u32::try_from(paginas.len()).unwrap_or(u32::MAX),
        paginas,
        nomes_conhecidos: args.nomes_paciente.clone(),
    };
    let Ok(prep) = montar_estado(&entrada, modo) else {
        return envelope(MotivoCli::ErroInterno, passo1, model);
    };
    let classificacao = match &envio {
        Some((provedor, prazo, _)) => {
            let avaliador = crate::classifier::Avaliador::new(std::num::NonZeroUsize::MIN, *prazo);
            crate::classifier::classificar(
                &prep,
                limiar,
                crate::classifier::Envio::Autorizado {
                    provedor,
                    avaliador: &avaliador,
                },
            )
            .await
        }
        None if args.enviar => {
            crate::classifier::classificar::<Provedor>(
                &prep,
                limiar,
                crate::classifier::Envio::NaoAutorizado,
            )
            .await
        }
        None => {
            crate::classifier::classificar::<Provedor>(
                &prep,
                limiar,
                crate::classifier::Envio::NaoPedido,
            )
            .await
        }
    };
    let mostrar = args.mostrar_estado
        && prep.bloqueio.is_empty()
        && !prep.sem_conteudo()
        && matches!(
            classificacao.estado,
            EstadoClassificacao::Preparado
                | EstadoClassificacao::Ok
                | EstadoClassificacao::Incerto
                | EstadoClassificacao::Erro
        );
    let termos = prep.estado.arquivo.termos_nome_arquivo.clone();
    linha(classificacao, termos, mostrar.then(|| prep.estado.clone()))
}

/// Order of the readable output: the JSON keys.
const CAMPOS: [&str; 29] = [
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
];

/// The text printed on stdout: the JSON line, or one `campo: valor` per line.
///
/// # Errors
///
/// The line could not be serialized (a bug).
pub fn texto_de_saida(linha: &LinhaCli, json: bool) -> Result<String, serde_json::Error> {
    if json {
        return serde_json::to_string(linha);
    }
    let valor = serde_json::to_value(linha)?;
    let mut saida: Vec<String> = CAMPOS
        .iter()
        .map(|campo| {
            format!(
                "{campo}: {}",
                valor.get(campo).unwrap_or(&serde_json::Value::Null)
            )
        })
        .collect();
    if let Some(estado) = &linha.estado_preparado {
        saida.push(format!("estado_preparado: {}", estado.json_fio()));
    }
    Ok(saida.join("\n"))
}

/// Poppler and tesseract, as the crate's extractors run them, with the classifier's
/// OCR language (`por`, TestoDoc's `ocr_local.IDIOMA`).
#[derive(Debug)]
pub struct FontePoppler {
    texto: PopplerTextExtractor,
    ocr: TesseractOcr,
}

impl FontePoppler {
    /// Probes the installed tools.
    pub async fn detect() -> Self {
        let config = ExtractionConfig {
            ocr_lang: "por".to_owned(),
            ..ExtractionConfig::default()
        };
        Self {
            texto: PopplerTextExtractor::detect().await,
            ocr: TesseractOcr::detect(config).await,
        }
    }
}

async fn saida_de(ferramenta: &str, args: &[&std::ffi::OsStr]) -> Option<String> {
    let filho = Command::new(ferramenta)
        .args(args)
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(PRAZO_FERRAMENTA, filho).await {
        Ok(Ok(saida)) if saida.status.success() => {
            Some(String::from_utf8_lossy(&saida.stdout).into_owned())
        }
        _ => None,
    }
}

const fn ferramenta(name: &'static str, version_args: &'static [&'static str]) -> Tool {
    Tool { name, version_args }
}

/// Sum of the image areas of one page, in points², from `pdfimages -list`: the rows of
/// type `image` or `stencil` (a mask is the transparency of the row before it).
/// `None` when a row cannot be read or has a zero resolution.
#[must_use]
pub fn area_das_imagens(lista: &str) -> Option<f64> {
    let mut linhas = lista.lines();
    let cabecalho: Vec<&str> = linhas.next()?.split_whitespace().collect();
    let coluna = |nome: &str| cabecalho.iter().position(|c| *c == nome);
    let (tipo, largura, altura) = (coluna("type")?, coluna("width")?, coluna("height")?);
    // `object ID` is one header cell per word but the rows keep the two numbers apart
    // in the same way, so positions match.
    let (x_ppi, y_ppi) = (coluna("x-ppi")?, coluna("y-ppi")?);
    if !linhas.next()?.starts_with("---") {
        return None;
    }
    let mut area = 0.0;
    for linha in linhas.filter(|l| !l.trim().is_empty()) {
        let celulas: Vec<&str> = linha.split_whitespace().collect();
        if !matches!(*celulas.get(tipo)?, "image" | "stencil") {
            continue;
        }
        let numero = |i: usize| celulas.get(i)?.parse::<f64>().ok();
        let (w, h, xr, yr) = (
            numero(largura)?,
            numero(altura)?,
            numero(x_ppi)?,
            numero(y_ppi)?,
        );
        if xr <= 0.0 || yr <= 0.0 {
            return None;
        }
        area += (w / xr * 72.0) * (h / yr * 72.0);
    }
    Some(area)
}

/// Area of page `n` in points², from `pdfinfo -f n -l n`: `Page n size: W x H pts`.
#[must_use]
pub fn area_da_pagina(info: &str, n: NonZeroU32) -> Option<f64> {
    info.lines().find_map(|linha| {
        let resto = linha.strip_prefix("Page")?.trim_start();
        let (numero, resto) = resto.split_once(char::is_whitespace)?;
        if numero.parse::<u32>().ok()? != n.get() {
            return None;
        }
        let medidas = resto.trim_start().strip_prefix("size:")?;
        let mut partes = medidas.split_whitespace();
        let w: f64 = partes.next()?.parse().ok()?;
        (partes.next()? == "x").then_some(())?;
        let h: f64 = partes.next()?.parse().ok()?;
        Some(w * h)
    })
}

impl Fonte for FontePoppler {
    async fn ferramentas_pdf(&self) -> bool {
        let pdf = [
            ferramenta("pdfinfo", &["-v"]),
            ferramenta("pdftotext", &["-v"]),
            ferramenta("pdfimages", &["-v"]),
        ];
        tools::probe(&pdf).await.iter().all(|s| s.version.is_some())
    }

    async fn ocr_disponivel(&self, pdf: bool) -> bool {
        if pdf
            && tools::probe(&[ferramenta("pdftoppm", &["-v"])])
                .await
                .iter()
                .any(|s| s.version.is_none())
        {
            return false;
        }
        // `--list-langs` prints a header line, then one language per line.
        saida_de("tesseract", &["--list-langs".as_ref()])
            .await
            .is_some_and(|s| s.lines().any(|l| l.trim() == "por"))
    }

    async fn paginas(&self, pdf: &Path) -> Result<NonZeroU32, ExtractionError> {
        self.texto.page_count(pdf).await
    }

    async fn texto(&self, pdf: &Path, n: NonZeroU32) -> Result<String, ExtractionError> {
        Ok(self.texto.extract_page(pdf, n).await?.text())
    }

    async fn visual(&self, pdf: &Path, n: NonZeroU32) -> bool {
        let pagina = n.to_string();
        let lista: [&std::ffi::OsStr; 6] = [
            "-list".as_ref(),
            "-f".as_ref(),
            pagina.as_ref(),
            "-l".as_ref(),
            pagina.as_ref(),
            pdf.as_os_str(),
        ];
        let tamanho: [&std::ffi::OsStr; 5] = [
            "-f".as_ref(),
            pagina.as_ref(),
            "-l".as_ref(),
            pagina.as_ref(),
            pdf.as_os_str(),
        ];
        let figuras = saida_de("pdfimages", &lista).await;
        let info = saida_de("pdfinfo", &tamanho).await;
        // A failed inspection is visual: "é mais barato renderizar à toa do que perder
        // conteúdo" (TP `pdf_extract._is_visual`).
        let (Some(figuras), Some(info)) = (figuras, info) else {
            return true;
        };
        match (area_das_imagens(&figuras), area_da_pagina(&info, n)) {
            (Some(figuras), Some(pagina)) if pagina > 0.0 => figuras / pagina >= AREA_VISUAL,
            _ => true,
        }
    }

    async fn ocr_pagina(&self, pdf: &Path, n: NonZeroU32) -> Result<String, ExtractionError> {
        Ok(self.ocr.ocr_page(pdf, n).await?.text())
    }

    async fn ocr_imagem(&self, imagem: &Path) -> Result<Option<String>, ExtractionError> {
        let caminho = imagem.to_owned();
        let decodifica = tokio::task::spawn_blocking(move || image::open(caminho).is_ok())
            .await
            .unwrap_or(false);
        if !decodifica {
            return Ok(None);
        }
        Ok(Some(self.ocr.ocr_image(imagem).await?.text()))
    }
}

/// The environment: process variables first, then the `.env` found from the current
/// directory up (as `dotenvy::dotenv` would load it, without changing the process
/// environment).
fn ambiente_do_processo() -> impl Fn(&str) -> Option<String> {
    let arquivo: HashMap<String, String> = dotenvy::dotenv_iter()
        .map(|itens| itens.filter_map(Result::ok).collect())
        .unwrap_or_default();
    move |nome| {
        std::env::var(nome)
            .ok()
            .or_else(|| arquivo.get(nome).cloned())
    }
}

/// Runs `classify` in the binary: prints the line, a fixed phrase for the reason on
/// stderr, and returns the exit code.
pub async fn comando(args: &ClassifyArgs) -> ExitCode {
    let fonte = FontePoppler::detect().await;
    let ambiente = ambiente_do_processo();
    let linha = executar(args, &fonte, &ambiente).await;
    if let Some(motivo) = linha.motivo_codigo {
        eprintln!("classify: {}", frase(motivo));
    }
    match texto_de_saida(&linha, args.json) {
        Ok(texto) => {
            println!("{texto}");
            ExitCode::from(linha.codigo_saida)
        }
        Err(_) => {
            eprintln!("classify: {}", frase(Motivo::Cli(MotivoCli::ErroInterno)));
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CABECALHO: &str = "page   num  type   width height color comp bpc  enc interp  object ID x-ppi y-ppi size ratio\n\
        --------------------------------------------------------------------------------------------\n";

    #[test]
    fn image_area_counts_images_and_stencils_but_not_masks() {
        let lista = format!(
            "{CABECALHO}   1     0 image     256   192  rgb     3   8  image  no         3  0    77    77 29.4K  20%\n\
                1     1 smask     256   192  gray    1   8  image  no         4  0    77    77  1.0K  2%\n\
                1     2 stencil    72    72  -       1   1  image  no         5  0    72    72   61B 0.9%\n"
        );
        let esperado = (256.0 / 77.0 * 72.0) * (192.0 / 77.0 * 72.0) + 72.0 * 72.0;
        let area = area_das_imagens(&lista).unwrap();
        assert!((area - esperado).abs() < 1e-9, "{area}");
        assert_eq!(area_das_imagens(CABECALHO), Some(0.0));
    }

    #[test]
    fn unreadable_image_lists_are_none() {
        let zero = format!(
            "{CABECALHO}   1     0 image     256   192  rgb     3   8  image  no         3  0     0    77 29.4K  20%\n"
        );
        assert_eq!(area_das_imagens(&zero), None);
        let curta = format!("{CABECALHO}   1     0 image     256\n");
        assert_eq!(area_das_imagens(&curta), None);
        assert_eq!(area_das_imagens(""), None);
        assert_eq!(area_das_imagens("page num type\nsem regua\n"), None);
    }

    #[test]
    fn page_area_reads_the_requested_page() {
        let info = "Pages:           3\nPage    1 size:  595 x 842 pts (A4)\nPage    1 rot:   0\n\
                    Page    2 size:  612.5 x 792 pts (letter)\n";
        let dois = NonZeroU32::new(2).unwrap();
        assert_eq!(area_da_pagina(info, NonZeroU32::MIN), Some(595.0 * 842.0));
        assert_eq!(area_da_pagina(info, dois), Some(612.5 * 792.0));
        assert_eq!(area_da_pagina(info, NonZeroU32::new(3).unwrap()), None);
        assert_eq!(
            area_da_pagina("Page    1 size:  a x 842 pts", NonZeroU32::MIN),
            None
        );
    }

    #[test]
    fn exit_codes_follow_testodoc() {
        assert_eq!(codigo_estado(EstadoClassificacao::Preparado), 0);
        assert_eq!(codigo_estado(EstadoClassificacao::Ok), 0);
        assert_eq!(codigo_estado(EstadoClassificacao::Incerto), 0);
        assert_eq!(codigo_estado(EstadoClassificacao::SemConteudo), 3);
        assert_eq!(codigo_estado(EstadoClassificacao::Erro), 4);
        assert_eq!(codigo_estado(EstadoClassificacao::BloqueadoPrivacidade), 6);
        assert_eq!(codigo_cli(MotivoCli::ErroInterno), 1);
        assert_eq!(codigo_cli(MotivoCli::LimitePdf), 2);
        assert_eq!(codigo_cli(MotivoCli::OcrIndisponivel), 5);
    }
}
