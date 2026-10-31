//! The Jev HTTP adapter (spec/CLASSIFIER.md §8; feature `classifier-jev`).
//!
//! It speaks TypeSafe's wire format directly, as the SDK 0.7.4 that TestoDoc uses does
//! (`endpoints.prepare_system_one`, `transport.prepare`): one `POST /v1/systemone`, no
//! retry, no timeout of its own (the deadline is [`super::provedor::Avaliador`]'s). The
//! key never appears in `Debug`, errors or logs, and an error body is never read.

use std::fmt;

use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::categorias::CRITERIO_ID;
use super::decisao::validar_respostas;
use super::provedor::{ADAPTER_VERSION, PROVIDER, RequestNeutro, RespostaProvedor, RlcdProvider};
use super::pyre::{ErroPyre, Flags, PyRegex};
use super::texto::strip;
use super::tipos::{ErroCategoria, Uso};

/// The default service (TestoDoc `BASE_URL_PADRAO`).
pub const BASE_URL_PADRAO: &str = "https://api.typesafe.ai";
const CAMINHO: &str = "/v1/systemone";
const HEADER_REQUEST_ID: &str = "x-typesafe-request-id";
const JSON: &str = "application/json";
const USER_AGENT: &str = concat!("cognitionlab-classifier/", env!("CARGO_PKG_VERSION"));
/// TestoDoc `MODELO_JEV_RE`: an exact version, never an alias (`jev-latest`).
const MODELO_JEV_RE: &str = r"^jev-\d+\.\d+\.\d+$";

/// Why the client could not be built. No variant carries the key or the URL.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErroConfiguracao {
    /// The model is not an exact Jev version.
    #[error("the model is not an exact Jev version")]
    ModeloInvalido,
    /// The key is empty.
    #[error("no API key")]
    SemChave,
    /// The base URL is not `https`, nor `http` on the local machine.
    #[error("invalid base URL")]
    BaseUrlInvalida,
    /// The HTTP client could not be built.
    #[error("the HTTP client could not be built")]
    Cliente,
}

/// The API key. `Debug` never shows it.
pub struct Chave(String);

impl Chave {
    /// Wraps a non-empty key.
    ///
    /// # Errors
    ///
    /// [`ErroConfiguracao::SemChave`] for an empty key.
    pub fn new(chave: String) -> Result<Self, ErroConfiguracao> {
        if chave.is_empty() {
            return Err(ErroConfiguracao::SemChave);
        }
        Ok(Self(chave))
    }
}

impl fmt::Debug for Chave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Chave(<redigida>)")
    }
}

/// TestoDoc `modelo_rlcd_fixo` for TypeSafe: `MODELO_JEV_RE.match(modelo.strip())`, with
/// Python's `\d` (any Unicode decimal digit).
///
/// # Errors
///
/// The regex failed to compile or to run.
pub fn modelo_fixo(modelo: &str) -> Result<bool, ErroPyre> {
    Ok(PyRegex::new(MODELO_JEV_RE, Flags::NENHUMA)?
        .match_at(strip(modelo), 0)?
        .is_some())
}

/// The endpoint URL (spec/CLASSIFIER.md §8.2, §12). A blank base is the default. Any
/// other must be an absolute `https` URL, or `http` on `127.0.0.1` or `localhost`,
/// without credentials, query or fragment. The path is joined as text, after dropping
/// one final `/`, never with `Url::join`.
///
/// # Errors
///
/// [`ErroConfiguracao::BaseUrlInvalida`].
pub fn url_do_endpoint(base: Option<&str>) -> Result<String, ErroConfiguracao> {
    let base = base.map(strip).filter(|b| !b.is_empty());
    let Some(base) = base else {
        return Ok(format!("{BASE_URL_PADRAO}{CAMINHO}"));
    };
    let url = reqwest::Url::parse(base).map_err(|_| ErroConfiguracao::BaseUrlInvalida)?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    let esquema_ok = match url.scheme() {
        "https" => url.host_str().is_some(),
        "http" => local,
        _ => false,
    };
    let limpa = url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none();
    if !(esquema_ok && limpa) {
        return Err(ErroConfiguracao::BaseUrlInvalida);
    }
    Ok(format!(
        "{}{CAMINHO}",
        base.strip_suffix('/').unwrap_or(base)
    ))
}

fn texto_json(s: &str) -> String {
    Value::from(s).to_string()
}

/// The request body, compact, keys in the SDK's order (`state`, `model`, `questions`;
/// a question is `type`, `instructions`, `criteria`), the options in the order of
/// [`super::Categoria::TODAS`]. Written by hand, so it does not depend on `serde_json`'s
/// `preserve_order`.
#[must_use]
pub fn corpo_request(req: &RequestNeutro<'_>, model: &str) -> String {
    let criteria: Vec<String> = req
        .criterio
        .opcoes
        .iter()
        .map(|c| format!("{}:{}", texto_json(c.valor()), texto_json(c.descricao())))
        .collect();
    format!(
        r#"{{"state":{},"model":{},"questions":{{{}:{{"type":"choice","instructions":{},"criteria":{{{}}}}}}}}}"#,
        req.estado.json_fio(),
        texto_json(model),
        texto_json(req.criterio.id),
        texto_json(req.criterio.pergunta),
        criteria.join(",")
    )
}

/// A non-200 status (spec/CLASSIFIER.md §8.6).
#[must_use]
pub const fn categoria_status(status: u16) -> ErroCategoria {
    match status {
        401 | 403 => ErroCategoria::Autenticacao,
        429 => ErroCategoria::QuotaProvedor,
        400 | 422 => ErroCategoria::RespostaInvalida,
        _ => ErroCategoria::Outro,
    }
}

/// A transport failure before the status (spec/CLASSIFIER.md §8.5), timeout first, as the
/// SDK tests it.
#[must_use]
pub fn categoria_transporte(erro: &reqwest::Error) -> ErroCategoria {
    if erro.is_timeout() {
        ErroCategoria::Timeout
    } else if erro.is_connect() || erro.is_request() || erro.is_body() {
        ErroCategoria::Rede
    } else {
        ErroCategoria::Outro
    }
}

/// A failure while reading a 200 body. `reqwest` marks a cut body as `is_decode()`, not
/// `is_body()`, but nothing is decoded there (`bytes()` only reads), so any error but a
/// timeout is a read failure: `rede`, as the SDK's `httpx` read errors are.
fn categoria_leitura(erro: &reqwest::Error) -> ErroCategoria {
    if erro.is_timeout() {
        ErroCategoria::Timeout
    } else {
        ErroCategoria::Rede
    }
}

/// The SDK's `SystemOneResponse` envelope. Unknown fields are allowed (the service may
/// add some); `answers` is a map, so an extra answer reaches the validation.
#[derive(Deserialize)]
struct RespostaFio {
    model: String,
    answers: Map<String, Value>,
    usage: UsoFio,
}

#[derive(Deserialize)]
struct UsoFio {
    input_tokens: u64,
    output_tokens: u64,
}

/// One wire answer in TestoDoc's neutral format, with Jev's `confidence`. Only a
/// well-formed `choice` keeps its values; anything else becomes a raw answer the
/// validation refuses, never a one-hot (spec/CLASSIFIER.md §8.4).
fn neutro(valor: &Value) -> (Value, Option<f64>) {
    let forma = valor.as_object().and_then(|o| {
        let tipo = o.get("type")?.as_str()?;
        let escolha = o.get("choice")?.as_str()?;
        // `as_f64` is `None` for anything but a JSON number (DV3).
        let confianca = o.get("confidence")?.as_f64()?;
        let probabilidades = o.get("probabilities")?.as_object()?;
        (tipo == "choice" && probabilidades.values().all(Value::is_number)).then(|| {
            (
                json!({"escolha": escolha, "probabilidades": probabilidades}),
                confianca,
            )
        })
    });
    match forma {
        Some((bruto, confianca)) => (bruto, Some(confianca)),
        None => (json!({"escolha": null, "probabilidades": {}}), None),
    }
}

/// A 200 body: the envelope, then the conversion and the validation (spec §8.4).
fn interpretar(corpo: &[u8], pedido: &str) -> RespostaProvedor {
    let fio = match serde_json::from_slice::<RespostaFio>(corpo) {
        Ok(fio) if !fio.answers.is_empty() => fio,
        _ => return RespostaProvedor::erro(ErroCategoria::RespostaInvalida, pedido),
    };
    let mut confianca = None;
    let brutas: Map<String, Value> = fio
        .answers
        .iter()
        .map(|(id, valor)| {
            let (bruto, c) = neutro(valor);
            if id == CRITERIO_ID {
                confianca = c;
            }
            (id.clone(), bruto)
        })
        .collect();
    let validada = validar_respostas(&Value::Object(brutas));
    RespostaProvedor {
        resultado: validada.ok_or(ErroCategoria::RespostaInvalida),
        model: fio.model,
        provider_request_id: None,
        latencia_ms: 0,
        uso: Some(Uso {
            input_tokens: fio.usage.input_tokens,
            output_tokens: fio.usage.output_tokens,
        }),
        confianca_provedor: validada.and(confianca),
    }
}

/// The Jev client.
pub struct ClienteJev {
    chave: Chave,
    model: String,
    url: String,
    http: reqwest::Client,
}

impl fmt::Debug for ClienteJev {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClienteJev")
            .field("chave", &self.chave)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl ClienteJev {
    /// A client for `model` (sent as given) at `base` ([`url_do_endpoint`]).
    ///
    /// # Errors
    ///
    /// [`ErroConfiguracao`] for a model alias, a bad base URL or a client that cannot
    /// be built.
    pub fn new(chave: Chave, model: &str, base: Option<&str>) -> Result<Self, ErroConfiguracao> {
        if !modelo_fixo(model).map_err(|_| ErroConfiguracao::ModeloInvalido)? {
            return Err(ErroConfiguracao::ModeloInvalido);
        }
        let url = url_do_endpoint(base)?;
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|_| ErroConfiguracao::Cliente)?;
        Ok(Self {
            chave,
            model: model.to_owned(),
            url,
            http,
        })
    }
}

impl RlcdProvider for ClienteJev {
    fn provider(&self) -> &'static str {
        PROVIDER
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn adapter_version(&self) -> &'static str {
        ADAPTER_VERSION
    }

    async fn evaluate(&self, req: &RequestNeutro<'_>) -> RespostaProvedor {
        let enviada = self
            .http
            .post(&self.url)
            .bearer_auth(&self.chave.0)
            .header(CONTENT_TYPE, JSON)
            .header(ACCEPT, JSON)
            .body(corpo_request(req, &self.model))
            .send()
            .await;
        let resposta = match enviada {
            Ok(resposta) => resposta,
            Err(erro) => {
                let categoria = categoria_transporte(&erro);
                tracing::warn!(?categoria, "jev: transport failure");
                return RespostaProvedor::erro(categoria, &self.model);
            }
        };
        let status = resposta.status().as_u16();
        if status != 200 {
            // The body is never read: it may echo the state.
            let categoria = categoria_status(status);
            tracing::warn!(status, ?categoria, "jev: status");
            return RespostaProvedor::erro(categoria, &self.model);
        }
        let request_id = resposta
            .headers()
            .get(HEADER_REQUEST_ID)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let mut saida = match resposta.bytes().await {
            Ok(corpo) => interpretar(&corpo, &self.model),
            Err(erro) => RespostaProvedor::erro(categoria_leitura(&erro), &self.model),
        };
        if saida.resultado.is_err() {
            tracing::warn!(categoria = ?saida.resultado.err(), "jev: answer");
        }
        saida.provider_request_id = request_id;
        saida
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classifier::provedor::CRITERIO_TIPO_DOCUMENTO;
    use crate::classifier::tipos::{Arquivo, EstadoEnviado, PaginaEnviada, PaginasModo};
    use crate::classifier::{Categoria, MASCARAMENTO_VERSAO_PORTE, PERGUNTA, PERGUNTAS_VERSAO};

    #[test]
    fn the_model_must_be_an_exact_version_as_in_testodoc() {
        // Expected values from TestoDoc's `modelo_rlcd_fixo("typesafe", m)`, Python 3.14.
        for (modelo, fixo) in [
            ("jev-1.13.0", true),
            (" jev-1.13.0\n", true),
            ("jev-١.١٣.٠", true),
            ("jev-latest", false),
            ("jev-1.13", false),
            ("jev-1.13.0-rc", false),
            ("JEV-1.13.0", false),
            ("", false),
            ("jev-1.13.0\n\n", true),
            ("jev-1.13.0\u{2028}", true),
            ("jev-1.1³.0", false),
            ("jev-01.013.000", true),
        ] {
            assert_eq!(modelo_fixo(modelo).unwrap(), fixo, "{modelo:?}");
        }
    }

    #[test]
    fn the_endpoint_is_https_or_local_http() {
        let padrao = "https://api.typesafe.ai/v1/systemone";
        assert_eq!(url_do_endpoint(None).as_deref(), Ok(padrao));
        assert_eq!(url_do_endpoint(Some("")).as_deref(), Ok(padrao));
        assert_eq!(url_do_endpoint(Some(" \t")).as_deref(), Ok(padrao));
        for (base, url) in [
            ("https://proxy.ex.com", "https://proxy.ex.com/v1/systemone"),
            ("https://proxy.ex.com/", "https://proxy.ex.com/v1/systemone"),
            (
                "https://proxy.ex.com/base/",
                "https://proxy.ex.com/base/v1/systemone",
            ),
            (
                "http://127.0.0.1:8080",
                "http://127.0.0.1:8080/v1/systemone",
            ),
            ("http://localhost:9/", "http://localhost:9/v1/systemone"),
        ] {
            assert_eq!(url_do_endpoint(Some(base)).as_deref(), Ok(url), "{base}");
        }
        for base in [
            "http://proxy.ex.com",
            "http://127.0.0.1.ex.com",
            "ftp://proxy.ex.com",
            "https://u:p@proxy.ex.com",
            "https://proxy.ex.com/?a=1",
            "https://proxy.ex.com/#a",
            "proxy.ex.com",
            "/v1",
        ] {
            assert_eq!(
                url_do_endpoint(Some(base)),
                Err(ErroConfiguracao::BaseUrlInvalida),
                "{base}"
            );
        }
    }

    #[test]
    fn statuses_map_to_categories() {
        for (status, categoria) in [
            (400, ErroCategoria::RespostaInvalida),
            (401, ErroCategoria::Autenticacao),
            (403, ErroCategoria::Autenticacao),
            (404, ErroCategoria::Outro),
            (413, ErroCategoria::Outro),
            (415, ErroCategoria::Outro),
            (422, ErroCategoria::RespostaInvalida),
            (429, ErroCategoria::QuotaProvedor),
            (500, ErroCategoria::Outro),
            (529, ErroCategoria::Outro),
            (204, ErroCategoria::Outro),
        ] {
            assert_eq!(categoria_status(status), categoria, "{status}");
        }
    }

    #[test]
    fn the_key_never_shows() {
        let chave = Chave::new("sk-sintetica-123".to_owned()).unwrap();
        assert_eq!(format!("{chave:?}"), "Chave(<redigida>)");
        let cliente = ClienteJev::new(chave, "jev-1.13.0", None).unwrap();
        assert!(!format!("{cliente:?}").contains("sk-sintetica"));
        assert_eq!(
            Chave::new(String::new()).err(),
            Some(ErroConfiguracao::SemChave)
        );
        let chave = Chave::new("sk-sintetica-123".to_owned()).unwrap();
        assert_eq!(
            ClienteJev::new(chave, "jev-latest", None).err(),
            Some(ErroConfiguracao::ModeloInvalido)
        );
    }

    fn estado() -> EstadoEnviado {
        EstadoEnviado {
            arquivo: Arquivo {
                termos_nome_arquivo: vec!["holter".to_owned()],
                termos_titulo: Vec::new(),
                extensao: "pdf".to_owned(),
                total_paginas: 1,
            },
            paginas_modo: PaginasModo::Duas,
            paginas: vec![PaginaEnviada {
                n: 1,
                texto: "a\u{2028}\"\\\u{1f}\u{7f}é".to_owned(),
            }],
        }
    }

    #[test]
    fn the_body_has_the_sdks_order_and_escapes() {
        let estado = estado();
        let req = RequestNeutro {
            estado: &estado,
            criterio: &CRITERIO_TIPO_DOCUMENTO,
            perguntas_versao: PERGUNTAS_VERSAO,
            politica_versao: MASCARAMENTO_VERSAO_PORTE,
            alvo_versao: "0123456789abcdef".to_owned(),
        };
        let corpo = corpo_request(&req, "jev-1.13.0");
        // The escapes are those of the SDK's `pydantic_core.to_json` (checked against
        // typesafe-sdk 0.7.4): `\u001f` escaped, U+007F and U+2028 literal.
        let inicio = concat!(
            r#"{"state":{"arquivo":{"termos_nome_arquivo":["holter"],"termos_titulo":[],"#,
            r#""extensao":"pdf","total_paginas":1},"paginas_modo":"2","#,
            "\"paginas\":[{\"n\":1,\"texto\":\"a\u{2028}\\\"\\\\\\u001f\u{7f}é\"}]},",
            r#""model":"jev-1.13.0","questions":{"tipo_documento":{"type":"choice","instructions":"#,
        );
        assert!(corpo.starts_with(inicio), "{corpo}");
        let resto = &corpo[inicio.len()..];
        let instrucoes = texto_json(PERGUNTA);
        assert!(resto.starts_with(&instrucoes));
        let criteria = &resto[instrucoes.len()..];
        assert!(criteria.starts_with(r#","criteria":{"#) && criteria.ends_with("}}}}"));
        let mut posicoes = Categoria::TODAS.iter().map(|c| {
            criteria.find(&format!(
                "{}:{}",
                texto_json(c.valor()),
                texto_json(c.descricao())
            ))
        });
        let mut anterior = 0;
        for p in posicoes.by_ref() {
            let p = p.unwrap();
            assert!(p > anterior);
            anterior = p;
        }
        let parsed: Value = serde_json::from_str(&corpo).unwrap();
        assert_eq!(
            parsed["questions"]["tipo_documento"]["criteria"]
                .as_object()
                .unwrap()
                .len(),
            9
        );
    }

    fn opcao(categoria: Categoria, p: f64) -> Value {
        let mut m = Map::new();
        for c in Categoria::TODAS {
            m.insert(
                c.valor().to_owned(),
                json!(if c == categoria { p } else { (1.0 - p) / 8.0 }),
            );
        }
        Value::Object(m)
    }

    fn resposta(answers: &Value) -> Vec<u8> {
        json!({"model": "jev-1.13.0", "answers": answers,
               "usage": {"input_tokens": 900, "output_tokens": 1}, "extra": true})
        .to_string()
        .into_bytes()
    }

    fn choice(probabilities: &Value) -> Value {
        json!({"type": "choice", "choice": "holter", "confidence": 0.81, "probabilities": probabilities})
    }

    #[test]
    fn a_valid_answer_keeps_every_field() {
        let r = interpretar(
            &resposta(&json!({"tipo_documento": choice(&opcao(Categoria::Holter, 0.92))})),
            "jev-1.13.0-pedido",
        );
        let escolha = r.resultado.unwrap();
        assert_eq!(escolha.escolha, Categoria::Holter);
        assert_eq!(escolha.probabilidades[Categoria::Holter.indice()], 0.92);
        assert_eq!(r.model, "jev-1.13.0");
        assert_eq!(r.confianca_provedor, Some(0.81));
        assert_eq!(
            r.uso,
            Some(Uso {
                input_tokens: 900,
                output_tokens: 1
            })
        );
    }

    #[test]
    fn an_answer_that_fails_the_validation_keeps_model_and_usage() {
        let p = opcao(Categoria::Holter, 0.92);
        let mut quase = opcao(Categoria::Holter, 0.92);
        quase["holter"] = json!(0.90);
        for answers in [
            json!({"tipo_documento": choice(&p), "outra": choice(&p)}),
            json!({"outra": choice(&p)}),
            json!({"tipo_documento": {"type": "noul", "noul": 0.9}}),
            json!({"tipo_documento": {"type": "choice", "choice": "holter", "confidence": 0.8}}),
            json!({"tipo_documento": choice(&Value::Null)}),
            json!({"tipo_documento": choice(&json!({}))}),
            json!({"tipo_documento": choice(&json!({"holter": 0.5, "raio_x": 0.5}))}),
            json!({"tipo_documento": choice(&quase)}),
            json!({"tipo_documento": choice(&json!({"holter": "NaN"}))}),
            json!({"tipo_documento": choice(&json!({"holter": "1.0"}))}),
            json!({"tipo_documento": {"type": "choice", "choice": "raio_x", "confidence": 0.8,
                                      "probabilities": {"holter": 1.0}}}),
            json!({"tipo_documento": {"type": "choice", "choice": 3, "confidence": 0.8,
                                      "probabilities": {"holter": 1.0}}}),
            json!({"tipo_documento": "holter"}),
            json!({"tipo_documento": {"type": "score", "choice": "holter", "confidence": 0.81,
                                      "probabilities": p}}),
            json!({"tipo_documento": {"type": "choice", "choice": "holter", "confidence": "0.81",
                                      "probabilities": p}}),
        ] {
            let r = interpretar(&resposta(&answers), "jev-1.13.0-pedido");
            assert_eq!(
                r.resultado,
                Err(ErroCategoria::RespostaInvalida),
                "{answers}"
            );
            assert_eq!(r.model, "jev-1.13.0", "{answers}");
            assert!(r.uso.is_some(), "{answers}");
            assert_eq!(r.confianca_provedor, None, "{answers}");
        }
    }

    #[test]
    fn an_envelope_out_of_schema_keeps_only_the_requested_model() {
        let p = choice(&opcao(Categoria::Holter, 0.92));
        for corpo in [
            b"nao e json".to_vec(),
            b"".to_vec(),
            json!({"model": "jev-1.13.0", "answers": {"tipo_documento": p}}).to_string().into_bytes(),
            json!({"model": "jev-1.13.0", "answers": {}, "usage": {"input_tokens": 1, "output_tokens": 1}})
                .to_string().into_bytes(),
            json!({"answers": {"tipo_documento": p}, "usage": {"input_tokens": 1, "output_tokens": 1}})
                .to_string().into_bytes(),
            json!({"model": "jev-1.13.0", "answers": [p], "usage": {"input_tokens": 1, "output_tokens": 1}})
                .to_string().into_bytes(),
            json!({"model": "jev-1.13.0", "answers": {"tipo_documento": p},
                   "usage": {"input_tokens": -1, "output_tokens": 1}})
                .to_string().into_bytes(),
        ] {
            let r = interpretar(&corpo, "jev-1.13.0-pedido");
            let texto = String::from_utf8_lossy(&corpo);
            assert_eq!(r.resultado, Err(ErroCategoria::RespostaInvalida), "{texto}");
            assert_eq!(r.model, "jev-1.13.0-pedido", "{texto}");
            assert_eq!((r.uso, r.confianca_provedor), (None, None), "{texto}");
        }
    }
}
