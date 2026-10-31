//! Input, prepared state and result types (spec/CLASSIFIER.md §5).
//!
//! Field names and enum values serialize exactly as in TestoDoc's Python, so outputs
//! compare with the conformance vectors and with TestoDoc's CLI.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use super::categorias::{Categoria, TEXTO_UTIL_MIN};

/// Where a page's text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrigemTexto {
    /// The PDF's own text layer.
    Nativo,
    /// Local OCR (tesseract).
    OcrLocal,
    /// A vision caption (only in TestoDoc's worker).
    Visao,
}

impl OrigemTexto {
    /// The serialized value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nativo => "nativo",
            Self::OcrLocal => "ocr_local",
            Self::Visao => "visao",
        }
    }
}

/// One page of the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagina {
    /// Page number, from 1 in production.
    pub n: u32,
    /// The page's text; may be empty.
    pub texto: String,
    /// Where the text came from.
    #[serde(default = "origem_padrao")]
    pub origem: OrigemTexto,
}

const fn origem_padrao() -> OrigemTexto {
    OrigemTexto::Nativo
}

/// What the classifier reads (TestoDoc `EntradaClassificacao`, TD §5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntradaClassificacao {
    /// File name with its extension; may be empty.
    pub nome_arquivo: String,
    /// Title; may be empty.
    pub titulo: String,
    /// Extension, normalized in the state.
    pub extensao: String,
    /// Copied to the state; not checked against `paginas.len()`.
    pub total_paginas: u32,
    /// Pages in document order.
    pub paginas: Vec<Pagina>,
    /// Names, or identifiers with a digit, to remove.
    #[serde(default)]
    pub nomes_conhecidos: Vec<String>,
}

/// Which pages are sent (`"1"`, `"2"` or `"todas"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaginasModo {
    /// The first page.
    #[serde(rename = "1")]
    Uma,
    /// The first two pages.
    #[serde(rename = "2")]
    Duas,
    /// Every page, up to the total limit.
    #[serde(rename = "todas")]
    Todas,
}

impl PaginasModo {
    /// The serialized value, which is also what the state carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Uma => "1",
            Self::Duas => "2",
            Self::Todas => "todas",
        }
    }

    /// How many pages the mode selects, `None` for all.
    #[must_use]
    pub const fn limite(self) -> Option<usize> {
        match self {
            Self::Uma => Some(1),
            Self::Duas => Some(2),
            Self::Todas => None,
        }
    }
}

impl fmt::Display for PaginasModo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A pages mode other than `1`, `2` or `todas`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("pages mode must be 1, 2 or todas")]
pub struct PaginasModoInvalido;

impl FromStr for PaginasModo {
    type Err = PaginasModoInvalido;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1" => Ok(Self::Uma),
            "2" => Ok(Self::Duas),
            "todas" => Ok(Self::Todas),
            _ => Err(PaginasModoInvalido),
        }
    }
}

/// A masking layer, as counted in `mascaramentos` and in a privacy block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Camada {
    /// Values of personal labels.
    Rotulo,
    /// Known names and identifiers.
    NomeConhecido,
    /// The name after Dr., Dra. and the like.
    Tratamento,
    /// Registry numbers, documents, contacts and long digit runs.
    Padrao,
    /// Dates.
    Data,
}

impl Camada {
    /// The serialized value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rotulo => "rotulo",
            Self::NomeConhecido => "nome_conhecido",
            Self::Tratamento => "tratamento",
            Self::Padrao => "padrao",
            Self::Data => "data",
        }
    }
}

/// The `arquivo` part of the sent state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arquivo {
    /// File-name terms from the type vocabulary.
    pub termos_nome_arquivo: Vec<String>,
    /// Title terms from the type vocabulary.
    pub termos_titulo: Vec<String>,
    /// Folded extension, without dots at the ends, at most 10 code points.
    pub extensao: String,
    /// Copied from the input.
    pub total_paginas: u32,
}

/// One sent page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaginaEnviada {
    /// Page number.
    pub n: u32,
    /// Masked and cut text.
    pub texto: String,
}

/// The state sent to the provider (TD §6, step 5). It has two serializations, the
/// canonical one for the digest and the wire one (spec/CLASSIFIER.md §5.4); neither
/// is derived, so neither depends on field order or on `serde_json` features.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoEnviado {
    /// File terms and metadata.
    pub arquivo: Arquivo,
    /// The pages mode.
    pub paginas_modo: PaginasModo,
    /// The sent pages.
    pub paginas: Vec<PaginaEnviada>,
}

/// The result of preparing the state (TestoDoc `EstadoPreparado`).
///
/// It keeps counts, never removed values (invariant 8, spec/CLASSIFIER.md §16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EstadoPreparado {
    /// The state as it would be sent.
    pub estado: EstadoEnviado,
    /// sha256 of the canonical JSON of the state.
    pub digest: [u8; 32],
    /// Pages that entered the state.
    pub paginas_enviadas: u32,
    /// Code points of text sent.
    pub caracteres_enviados: u32,
    /// Some page or the total was cut.
    pub truncado: bool,
    /// Removals per layer, only the layers above zero.
    pub mascaramentos: BTreeMap<Camada, u32>,
    /// Text origins of the sent pages, sorted, without repeats.
    pub origens_texto: Vec<OrigemTexto>,
    /// What the final check found; empty means the state may be sent.
    pub bloqueio: BTreeMap<Camada, u32>,
    /// Letters and digits outside markers and label words.
    pub caracteres_uteis: u32,
    /// Some exam-type term other than the generic ones is present.
    pub termo_do_tipo: bool,
}

impl EstadoPreparado {
    /// Text was sent but says nothing about the type.
    #[must_use]
    pub const fn texto_insuficiente(&self) -> bool {
        self.paginas_enviadas > 0 && !self.termo_do_tipo && self.caracteres_uteis < TEXTO_UTIL_MIN
    }

    /// Nothing worth sending.
    #[must_use]
    pub const fn sem_conteudo(&self) -> bool {
        self.paginas_enviadas == 0 || self.texto_insuficiente()
    }
}

/// A validated choice: the chosen type and the nine probabilities, indexed by
/// [`Categoria::indice`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RespostaEscolha {
    /// The provider's choice.
    pub escolha: Categoria,
    /// Probability of each type, in the order of [`Categoria::TODAS`].
    pub probabilidades: [f64; 9],
}

/// State of a classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoClassificacao {
    /// Prepared, not sent (CLI only).
    Preparado,
    /// Valid answer at or above the threshold.
    Ok,
    /// Valid answer below the threshold.
    Incerto,
    /// Nothing to send.
    SemConteudo,
    /// The final check found personal data; nothing was sent.
    BloqueadoPrivacidade,
    /// The call failed or the answer was invalid.
    Erro,
}

/// Why a call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErroCategoria {
    /// The deadline passed.
    Timeout,
    /// Connection, DNS, TLS or body read failed.
    Rede,
    /// The answer, or the request as the service saw it, was invalid.
    RespostaInvalida,
    /// The key was refused.
    Autenticacao,
    /// The provider's rate limit.
    QuotaProvedor,
    /// Anything else.
    Outro,
}

/// Stable reason of a classification state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotivoCodigo {
    /// No selected page had text.
    SemConteudo,
    /// Too little useful text and no exam-type term.
    TextoInsuficiente,
    /// The final privacy check found something.
    ConferenciaPrivacidade,
    /// The provider call failed or answered invalidly.
    ProvedorErro,
    /// Sending was asked for but not authorized (spec/CLASSIFIER.md §3.2, R-P1).
    EnvioNaoAutorizado,
}

/// Reasons only the CLI produces, before a classification exists (TestoDoc
/// `cli/classificar_documento.py`, without the office-conversion ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotivoCli {
    /// A setting is invalid.
    ConfiguracaoInvalida,
    /// No API key.
    SemChave,
    /// The model is not a pinned Jev version.
    ModeloInvalido,
    /// The input file does not exist.
    ArquivoInexistente,
    /// The input format is not supported.
    FormatoNaoSuportado,
    /// The PDF has more pages than allowed.
    LimitePdf,
    /// The input could not be read.
    ArquivoIlegivel,
    /// A required tool or OCR language is missing.
    OcrIndisponivel,
    /// An unexpected failure.
    ErroInterno,
}

/// Token usage reported by the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Uso {
    /// Billable input tokens.
    pub input_tokens: u64,
    /// Output tokens.
    pub output_tokens: u64,
}

/// The result of a classification (spec/CLASSIFIER.md §5.5; TD §9.1).
///
/// It is not serialized as a whole: the CLI's JSON line is a separate struct
/// (spec/CLASSIFIER.md §10.3), which serializes `probabilidades` with
/// [`serializar_probabilidades`].
#[derive(Debug, Clone, PartialEq)]
pub struct Classificacao {
    /// The final state.
    pub estado: EstadoClassificacao,
    /// The pages mode.
    pub paginas_modo: PaginasModo,
    /// Copied from the input.
    pub total_paginas: u32,
    /// Pages that entered the state.
    pub paginas_enviadas: u32,
    /// Code points of text sent.
    pub caracteres_enviados: u32,
    /// Some page or the total was cut.
    pub truncado: bool,
    /// Removals per layer, plus `conferencia_<layer>` when the final check blocks.
    pub mascaramentos: BTreeMap<String, u32>,
    /// Text origins of the sent pages, sorted, without repeats.
    pub origens_texto: Vec<OrigemTexto>,
    /// Hex sha256 of the canonical JSON of the state (64 digits).
    pub entrada_digest: String,
    /// The chosen type, only with a valid answer.
    pub tipo: Option<Categoria>,
    /// Probability of the chosen type (TD §10).
    pub confianca: Option<f64>,
    /// The highest probability minus the second highest, at least 0, whichever type was
    /// chosen (spec/CLASSIFIER.md §7.2).
    pub margem: Option<f64>,
    /// Probability of each type, in the order of [`Categoria::TODAS`]; `None` without a
    /// valid answer.
    pub probabilidades: Option<[f64; 9]>,
    /// Jev's own `confidence`, informative only (extension DV7).
    pub confianca_provedor: Option<f64>,
    /// Why the call failed.
    pub erro_categoria: Option<ErroCategoria>,
    /// Stable reason of the state.
    pub motivo_codigo: Option<MotivoCodigo>,
    /// Always `"typesafe"`.
    pub provider: &'static str,
    /// The configured model; after a call, the one the answer echoes.
    pub model: String,
    /// Identity of the adapter (spec/CLASSIFIER.md §8.1).
    pub adapter_version: &'static str,
    /// [`super::PERGUNTAS_VERSAO`].
    pub perguntas_versao: &'static str,
    /// [`super::MASCARAMENTO_VERSAO_PORTE`] (DV10).
    pub mascaramento_versao: &'static str,
    /// The threshold the decision used.
    pub limiar_aplicado: f64,
    /// The provider's request id, when it sent one.
    pub provider_request_id: Option<String>,
    /// Duration of the provider call, 0 without a call.
    pub latencia_ms: u64,
    /// Token usage, when the provider reported it.
    pub uso: Option<Uso>,
}

/// Serializes probabilities as an object keyed by [`Categoria::valor`] in the order of
/// [`Categoria::TODAS`], and `None` as `{}` (TD §11.2). For `#[serde(serialize_with)]`.
///
/// The order is written explicitly, so it does not depend on `serde_json`'s
/// `preserve_order` feature.
///
/// # Errors
///
/// Whatever the serializer returns.
pub fn serializar_probabilidades<S: Serializer>(
    probabilidades: &Option<[f64; 9]>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let Some(probabilidades) = probabilidades else {
        return serializer.serialize_map(Some(0))?.end();
    };
    let mut mapa = serializer.serialize_map(Some(Categoria::TODAS.len()))?;
    for (categoria, probabilidade) in Categoria::TODAS.into_iter().zip(probabilidades) {
        mapa.serialize_entry(categoria.valor(), probabilidade)?;
    }
    mapa.end()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn enums_serialize_with_the_python_names() {
        assert_eq!(json(&OrigemTexto::OcrLocal), "\"ocr_local\"");
        assert_eq!(json(&Camada::NomeConhecido), "\"nome_conhecido\"");
        assert_eq!(
            json(&EstadoClassificacao::BloqueadoPrivacidade),
            "\"bloqueado_privacidade\""
        );
        assert_eq!(json(&ErroCategoria::QuotaProvedor), "\"quota_provedor\"");
        assert_eq!(
            json(&MotivoCodigo::EnvioNaoAutorizado),
            "\"envio_nao_autorizado\""
        );
        assert_eq!(json(&MotivoCli::OcrIndisponivel), "\"ocr_indisponivel\"");
        for origem in [
            OrigemTexto::Nativo,
            OrigemTexto::OcrLocal,
            OrigemTexto::Visao,
        ] {
            assert_eq!(json(&origem), format!("\"{}\"", origem.as_str()));
        }
        for camada in [
            Camada::Rotulo,
            Camada::NomeConhecido,
            Camada::Tratamento,
            Camada::Padrao,
            Camada::Data,
        ] {
            assert_eq!(json(&camada), format!("\"{}\"", camada.as_str()));
        }
    }

    #[test]
    fn pages_mode_parses_only_the_three_values() {
        for modo in [PaginasModo::Uma, PaginasModo::Duas, PaginasModo::Todas] {
            assert_eq!(modo.as_str().parse::<PaginasModo>(), Ok(modo));
            assert_eq!(json(&modo), format!("\"{}\"", modo.as_str()));
        }
        for invalido in ["", "0", "3", "Todas", " 1", "todas "] {
            assert_eq!(
                invalido.parse::<PaginasModo>(),
                Err(PaginasModoInvalido),
                "{invalido:?}"
            );
        }
        assert_eq!(PaginasModo::Duas.limite(), Some(2));
        assert_eq!(PaginasModo::Todas.limite(), None);
    }

    fn ida_e_volta<T>(valores: &[T], texto: impl Fn(T) -> &'static str)
    where
        T: Serialize + serde::de::DeserializeOwned + PartialEq + fmt::Debug + Copy,
    {
        for &valor in valores {
            let serializado = json(&valor);
            assert_eq!(serializado, format!("\"{}\"", texto(valor)));
            assert_eq!(serde_json::from_str::<T>(&serializado).unwrap(), valor);
        }
        assert!(serde_json::from_str::<T>("\"desconhecido\"").is_err());
    }

    #[test]
    fn enums_deserialize_what_they_serialize() {
        ida_e_volta(
            &[
                OrigemTexto::Nativo,
                OrigemTexto::OcrLocal,
                OrigemTexto::Visao,
            ],
            OrigemTexto::as_str,
        );
        ida_e_volta(
            &[
                Camada::Rotulo,
                Camada::NomeConhecido,
                Camada::Tratamento,
                Camada::Padrao,
                Camada::Data,
            ],
            Camada::as_str,
        );
        ida_e_volta(
            &[PaginasModo::Uma, PaginasModo::Duas, PaginasModo::Todas],
            PaginasModo::as_str,
        );
        ida_e_volta(
            &[
                EstadoClassificacao::Preparado,
                EstadoClassificacao::Ok,
                EstadoClassificacao::Incerto,
                EstadoClassificacao::SemConteudo,
                EstadoClassificacao::BloqueadoPrivacidade,
                EstadoClassificacao::Erro,
            ],
            |e| match e {
                EstadoClassificacao::Preparado => "preparado",
                EstadoClassificacao::Ok => "ok",
                EstadoClassificacao::Incerto => "incerto",
                EstadoClassificacao::SemConteudo => "sem_conteudo",
                EstadoClassificacao::BloqueadoPrivacidade => "bloqueado_privacidade",
                EstadoClassificacao::Erro => "erro",
            },
        );
        ida_e_volta(
            &[
                ErroCategoria::Timeout,
                ErroCategoria::Rede,
                ErroCategoria::RespostaInvalida,
                ErroCategoria::Autenticacao,
                ErroCategoria::QuotaProvedor,
                ErroCategoria::Outro,
            ],
            |e| match e {
                ErroCategoria::Timeout => "timeout",
                ErroCategoria::Rede => "rede",
                ErroCategoria::RespostaInvalida => "resposta_invalida",
                ErroCategoria::Autenticacao => "autenticacao",
                ErroCategoria::QuotaProvedor => "quota_provedor",
                ErroCategoria::Outro => "outro",
            },
        );
        ida_e_volta(
            &[
                MotivoCodigo::SemConteudo,
                MotivoCodigo::TextoInsuficiente,
                MotivoCodigo::ConferenciaPrivacidade,
                MotivoCodigo::ProvedorErro,
                MotivoCodigo::EnvioNaoAutorizado,
            ],
            |m| match m {
                MotivoCodigo::SemConteudo => "sem_conteudo",
                MotivoCodigo::TextoInsuficiente => "texto_insuficiente",
                MotivoCodigo::ConferenciaPrivacidade => "conferencia_privacidade",
                MotivoCodigo::ProvedorErro => "provedor_erro",
                MotivoCodigo::EnvioNaoAutorizado => "envio_nao_autorizado",
            },
        );
        ida_e_volta(
            &[
                MotivoCli::ConfiguracaoInvalida,
                MotivoCli::SemChave,
                MotivoCli::ModeloInvalido,
                MotivoCli::ArquivoInexistente,
                MotivoCli::FormatoNaoSuportado,
                MotivoCli::LimitePdf,
                MotivoCli::ArquivoIlegivel,
                MotivoCli::OcrIndisponivel,
                MotivoCli::ErroInterno,
            ],
            |m| match m {
                MotivoCli::ConfiguracaoInvalida => "configuracao_invalida",
                MotivoCli::SemChave => "sem_chave",
                MotivoCli::ModeloInvalido => "modelo_invalido",
                MotivoCli::ArquivoInexistente => "arquivo_inexistente",
                MotivoCli::FormatoNaoSuportado => "formato_nao_suportado",
                MotivoCli::LimitePdf => "limite_pdf",
                MotivoCli::ArquivoIlegivel => "arquivo_ilegivel",
                MotivoCli::OcrIndisponivel => "ocr_indisponivel",
                MotivoCli::ErroInterno => "erro_interno",
            },
        );
    }

    #[test]
    fn pages_mode_displays_its_value() {
        for modo in [PaginasModo::Uma, PaginasModo::Duas, PaginasModo::Todas] {
            assert_eq!(modo.to_string(), modo.as_str());
        }
    }

    #[derive(Serialize)]
    struct ComProbabilidades {
        #[serde(serialize_with = "serializar_probabilidades")]
        probabilidades: Option<[f64; 9]>,
    }

    #[test]
    fn no_probabilities_serialize_as_an_empty_object() {
        let linha = ComProbabilidades {
            probabilidades: None,
        };
        assert_eq!(json(&linha), r#"{"probabilidades":{}}"#);
    }

    #[test]
    fn probabilities_serialize_in_the_order_of_the_categories() {
        // Not alphabetical: `vop` comes first and `ecg` before `outro` (TD §11.2).
        let linha = ComProbabilidades {
            probabilidades: Some([
                0.5, 0.25, 0.125, 0.0625, 0.03125, 0.015625, 0.0078125, 0.00390625, 0.00390625,
            ]),
        };
        assert_eq!(
            json(&linha),
            concat!(
                r#"{"probabilidades":{"vop":0.5,"eco_convencional":0.25,"eco_avancado":0.125,"#,
                r#""calorimetria":0.0625,"ergoespirometria":0.03125,"holter":0.015625,"#,
                r#""mapa":0.0078125,"ecg":0.00390625,"outro":0.00390625}}"#
            )
        );
    }

    #[test]
    fn page_origin_defaults_to_native() {
        let pagina: Pagina = serde_json::from_str(r#"{"n": 1, "texto": "x"}"#).unwrap();
        assert_eq!(pagina.origem, OrigemTexto::Nativo);
    }

    fn preparado(
        paginas_enviadas: u32,
        termo_do_tipo: bool,
        caracteres_uteis: u32,
    ) -> EstadoPreparado {
        EstadoPreparado {
            estado: EstadoEnviado {
                arquivo: Arquivo {
                    termos_nome_arquivo: vec![],
                    termos_titulo: vec![],
                    extensao: String::new(),
                    total_paginas: 0,
                },
                paginas_modo: PaginasModo::Duas,
                paginas: vec![],
            },
            digest: [0; 32],
            paginas_enviadas,
            caracteres_enviados: 0,
            truncado: false,
            mascaramentos: BTreeMap::new(),
            origens_texto: vec![],
            bloqueio: BTreeMap::new(),
            caracteres_uteis,
            termo_do_tipo,
        }
    }

    #[test]
    fn no_content_follows_td_section_6_step_10() {
        assert!(preparado(0, true, 100).sem_conteudo(), "no page sent");
        assert!(
            !preparado(0, true, 100).texto_insuficiente(),
            "insufficient needs a sent page"
        );
        assert!(preparado(1, false, TEXTO_UTIL_MIN - 1).texto_insuficiente());
        assert!(preparado(1, false, TEXTO_UTIL_MIN - 1).sem_conteudo());
        assert!(
            !preparado(1, false, TEXTO_UTIL_MIN).sem_conteudo(),
            "the minimum itself is enough"
        );
        assert!(
            !preparado(1, true, 0).sem_conteudo(),
            "a type term is enough"
        );
    }
}
