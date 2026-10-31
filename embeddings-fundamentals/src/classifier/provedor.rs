//! The provider contract and `classificar` (spec/CLASSIFIER.md §5.6, §8.3, §9; TestoDoc
//! `rlcd_client` and `classificador_documento.classificar`).
//!
//! Nothing here does I/O by itself: the provider is a trait, and the only adapter that
//! talks to the network is `jev` (feature `classifier-jev`).

use std::collections::BTreeMap;
use std::future::Future;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use tokio::sync::Semaphore;

use super::categorias::{
    CRITERIO_ID, Categoria, MASCARAMENTO_VERSAO_PORTE, PERGUNTA, PERGUNTAS_VERSAO,
};
use super::decisao::decidir_classificacao;
use super::tipos::{
    Classificacao, ErroCategoria, EstadoClassificacao, EstadoEnviado, EstadoPreparado,
    MotivoCodigo, RespostaEscolha, Uso,
};

/// The provider's name in every result.
pub const PROVIDER: &str = "typesafe";
/// Identity of the port's Jev adapter (spec/CLASSIFIER.md §8.1, DV1).
pub const ADAPTER_VERSION: &str = "cognitionlab-jev-http/1";
/// The default model: an exact Jev version, never an alias.
pub const MODELO_PADRAO: &str = "jev-1.13.0";

/// Keys that never leave for the provider (TestoDoc `CHAVES_PROIBIDAS_NO_ESTADO`). The
/// state's types cannot hold them; a test checks it (spec/CLASSIFIER.md §9.2).
pub const CHAVES_PROIBIDAS_NO_ESTADO: [&str; 14] = [
    "profile_id",
    "user_id",
    "nome_completo",
    "nome",
    "email",
    "cpf",
    "telefone",
    "endereco",
    "cep",
    "data_nascimento",
    "contato_emergencia_nome",
    "contato_emergencia_telefone",
    "treinador_nome",
    "treinador_contato",
];

/// A choice criterion: the question and its options, in order.
#[derive(Debug)]
pub struct CriterioEscolha {
    /// Answer key.
    pub id: &'static str,
    /// The question's instructions.
    pub pergunta: &'static str,
    /// The options, in the order the provider sees them.
    pub opcoes: [Categoria; 9],
}

/// The classifier's single criterion (TestoDoc `CRITERIO_TIPO_DOCUMENTO`).
pub static CRITERIO_TIPO_DOCUMENTO: CriterioEscolha = CriterioEscolha {
    id: CRITERIO_ID,
    pergunta: PERGUNTA,
    opcoes: Categoria::TODAS,
};

/// The neutral request (TestoDoc `RLCDRequest`, without the pseudonymous reference and
/// the request id, which are not sent to Jev).
#[derive(Debug)]
pub struct RequestNeutro<'a> {
    /// The state to evaluate.
    pub estado: &'a EstadoEnviado,
    /// The question.
    pub criterio: &'static CriterioEscolha,
    /// [`PERGUNTAS_VERSAO`].
    pub perguntas_versao: &'static str,
    /// [`MASCARAMENTO_VERSAO_PORTE`] (DV10).
    pub politica_versao: &'static str,
    /// The first 16 hex digits of the state's digest.
    pub alvo_versao: String,
}

impl<'a> RequestNeutro<'a> {
    /// The request for a prepared state.
    #[must_use]
    pub fn new(prep: &'a EstadoPreparado) -> Self {
        let mut alvo_versao = hex::encode(prep.digest);
        alvo_versao.truncate(16);
        Self {
            estado: &prep.estado,
            criterio: &CRITERIO_TIPO_DOCUMENTO,
            perguntas_versao: PERGUNTAS_VERSAO,
            politica_versao: MASCARAMENTO_VERSAO_PORTE,
            alvo_versao,
        }
    }
}

/// The provider's answer. A failure is a value, never an `Err` of `evaluate`: "nenhuma
/// sobe para a política" (TestoDoc `typesafe.py`).
#[derive(Debug, Clone, PartialEq)]
pub struct RespostaProvedor {
    /// The validated choice, or why there is none.
    pub resultado: Result<RespostaEscolha, ErroCategoria>,
    /// The model the answer echoes, or the requested one.
    pub model: String,
    /// The provider's request id, when it sent one.
    pub provider_request_id: Option<String>,
    /// Duration of the call; [`Avaliador::avaliar`] sets it.
    pub latencia_ms: u64,
    /// Token usage, when reported.
    pub uso: Option<Uso>,
    /// Jev's own `confidence` (DV7).
    pub confianca_provedor: Option<f64>,
}

impl RespostaProvedor {
    /// A failure with no metadata but the requested model.
    #[must_use]
    pub fn erro(categoria: ErroCategoria, model: &str) -> Self {
        Self {
            resultado: Err(categoria),
            model: model.to_owned(),
            provider_request_id: None,
            latencia_ms: 0,
            uso: None,
            confianca_provedor: None,
        }
    }
}

/// A provider (TestoDoc `RLCDClient`). Used through generics, never as `dyn`.
pub trait RlcdProvider: Send + Sync {
    /// The provider's name.
    fn provider(&self) -> &'static str;
    /// The requested model.
    fn model(&self) -> &str;
    /// The adapter's identity.
    fn adapter_version(&self) -> &'static str;
    /// Evaluates the request. Never fails: a failure is a [`RespostaProvedor`] with an
    /// [`ErroCategoria`].
    fn evaluate(&self, req: &RequestNeutro<'_>) -> impl Future<Output = RespostaProvedor> + Send;
}

/// Deadline and concurrency of the calls (spec/CLASSIFIER.md §8.3). The wait for a slot
/// counts inside the deadline, as in TestoDoc, and there is no retry.
#[derive(Debug)]
pub struct Avaliador {
    semaforo: Semaphore,
    prazo: Duration,
}

impl Avaliador {
    /// `vagas` concurrent calls, each with `prazo` from the start of the wait.
    #[must_use]
    pub fn new(vagas: NonZeroUsize, prazo: Duration) -> Self {
        Self {
            semaforo: Semaphore::new(vagas.get()),
            prazo,
        }
    }

    /// Calls the provider under the deadline. A missed deadline is
    /// [`ErroCategoria::Timeout`]; the latency is measured here in every case.
    pub async fn avaliar<P: RlcdProvider>(
        &self,
        provedor: &P,
        req: &RequestNeutro<'_>,
    ) -> RespostaProvedor {
        let inicio = Instant::now();
        let chamada = async {
            // The semaphore is never closed; a closed one would be a bug, reported as `outro`.
            let Ok(_vaga) = self.semaforo.acquire().await else {
                return RespostaProvedor::erro(ErroCategoria::Outro, provedor.model());
            };
            provedor.evaluate(req).await
        };
        let mut resposta = match tokio::time::timeout(self.prazo, chamada).await {
            Ok(resposta) => resposta,
            Err(_) => RespostaProvedor::erro(ErroCategoria::Timeout, provedor.model()),
        };
        resposta.latencia_ms = u64::try_from(inicio.elapsed().as_millis()).unwrap_or(u64::MAX);
        resposta
    }
}

/// Whether, and through what, the state may be sent (spec/CLASSIFIER.md §3.2, R-P1).
#[derive(Debug)]
pub enum Envio<'a, P> {
    /// Sending was not asked for.
    NaoPedido,
    /// Sending was asked for but not authorized.
    NaoAutorizado,
    /// Sending is authorized, through this provider.
    Autorizado {
        /// The provider.
        provedor: &'a P,
        /// Deadline and concurrency.
        avaliador: &'a Avaliador,
    },
}

/// `classificar` (TD §9.1, spec/CLASSIFIER.md §9.1). The provider is called only with
/// content, without a privacy block and with the sending authorized, in that order, so
/// a `preparado` always means "would be sent".
pub async fn classificar<P: RlcdProvider>(
    prep: &EstadoPreparado,
    limiar: f64,
    envio: Envio<'_, P>,
) -> Classificacao {
    let mut c = Classificacao {
        estado: EstadoClassificacao::Preparado,
        paginas_modo: prep.estado.paginas_modo,
        total_paginas: prep.estado.arquivo.total_paginas,
        paginas_enviadas: prep.paginas_enviadas,
        caracteres_enviados: prep.caracteres_enviados,
        truncado: prep.truncado,
        mascaramentos: prep
            .mascaramentos
            .iter()
            .map(|(camada, n)| (camada.as_str().to_owned(), *n))
            .collect::<BTreeMap<_, _>>(),
        origens_texto: prep.origens_texto.clone(),
        entrada_digest: hex::encode(prep.digest),
        tipo: None,
        confianca: None,
        margem: None,
        probabilidades: None,
        confianca_provedor: None,
        erro_categoria: None,
        motivo_codigo: None,
        provider: PROVIDER,
        model: None,
        adapter_version: ADAPTER_VERSION,
        perguntas_versao: PERGUNTAS_VERSAO,
        mascaramento_versao: MASCARAMENTO_VERSAO_PORTE,
        limiar_aplicado: limiar,
        provider_request_id: None,
        latencia_ms: 0,
        uso: None,
    };
    let chamada = match envio {
        Envio::Autorizado {
            provedor,
            avaliador,
        } => {
            c.provider = provedor.provider();
            c.model = Some(provedor.model().to_owned());
            c.adapter_version = provedor.adapter_version();
            Some((provedor, avaliador))
        }
        Envio::NaoAutorizado => {
            c.motivo_codigo = Some(MotivoCodigo::EnvioNaoAutorizado);
            None
        }
        Envio::NaoPedido => None,
    };
    if prep.sem_conteudo() {
        c.estado = EstadoClassificacao::SemConteudo;
        c.motivo_codigo = Some(if prep.texto_insuficiente() {
            MotivoCodigo::TextoInsuficiente
        } else {
            MotivoCodigo::SemConteudo
        });
        return c;
    }
    if !prep.bloqueio.is_empty() {
        for (camada, n) in &prep.bloqueio {
            *c.mascaramentos
                .entry(format!("conferencia_{}", camada.as_str()))
                .or_insert(0) += n;
        }
        c.estado = EstadoClassificacao::BloqueadoPrivacidade;
        c.motivo_codigo = Some(MotivoCodigo::ConferenciaPrivacidade);
        return c;
    }
    let Some((provedor, avaliador)) = chamada else {
        return c;
    };
    let resposta = avaliador.avaliar(provedor, &RequestNeutro::new(prep)).await;
    c.model = Some(resposta.model);
    c.provider_request_id = resposta.provider_request_id;
    c.latencia_ms = resposta.latencia_ms;
    c.uso = resposta.uso;
    match resposta.resultado {
        Err(categoria) => {
            c.estado = EstadoClassificacao::Erro;
            c.erro_categoria = Some(categoria);
            c.motivo_codigo = Some(MotivoCodigo::ProvedorErro);
        }
        Ok(escolha) => {
            let (estado, confianca, margem) =
                decidir_classificacao(escolha.escolha, &escolha.probabilidades, limiar);
            c.estado = estado;
            c.tipo = Some(escolha.escolha);
            c.confianca = Some(confianca);
            c.margem = Some(margem);
            c.probabilidades = Some(escolha.probabilidades);
            c.confianca_provedor = resposta.confianca_provedor;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::Value;

    use super::*;
    use crate::classifier::estado::montar_estado;
    use crate::classifier::tipos::{EntradaClassificacao, OrigemTexto, Pagina, PaginasModo};

    const HOLTER: &str = "Holter de 24 horas. Ritmo sinusal predominante, \
        extrassístoles ventriculares isoladas e raras, sem pausas significativas.";

    fn preparar(texto: &str, extensao: &str, conhecidos: &[&str]) -> EstadoPreparado {
        let entrada = EntradaClassificacao {
            nome_arquivo: "documento.pdf".to_owned(),
            titulo: String::new(),
            extensao: extensao.to_owned(),
            total_paginas: 1,
            paginas: vec![Pagina {
                n: 1,
                texto: texto.to_owned(),
                origem: OrigemTexto::Nativo,
            }],
            nomes_conhecidos: conhecidos.iter().map(|&s| s.to_owned()).collect(),
        };
        montar_estado(&entrada, PaginasModo::Duas).unwrap()
    }

    struct Duble {
        resposta: RespostaProvedor,
        espera: Duration,
        chamadas: AtomicUsize,
        alvos: Mutex<Vec<(String, &'static str, &'static str)>>,
    }

    impl Duble {
        fn new(resultado: Result<RespostaEscolha, ErroCategoria>) -> Self {
            Self {
                resposta: RespostaProvedor {
                    resultado,
                    model: "jev-1.13.0-eco".to_owned(),
                    provider_request_id: Some("req-1".to_owned()),
                    latencia_ms: 0,
                    uso: Some(Uso {
                        input_tokens: 120,
                        output_tokens: 3,
                    }),
                    confianca_provedor: Some(0.42),
                },
                espera: Duration::ZERO,
                chamadas: AtomicUsize::new(0),
                alvos: Mutex::new(Vec::new()),
            }
        }
    }

    impl RlcdProvider for Duble {
        fn provider(&self) -> &'static str {
            "duble"
        }
        fn model(&self) -> &str {
            "jev-1.13.0"
        }
        fn adapter_version(&self) -> &'static str {
            "duble/1"
        }
        async fn evaluate(&self, req: &RequestNeutro<'_>) -> RespostaProvedor {
            self.chamadas.fetch_add(1, Ordering::SeqCst);
            self.alvos.lock().unwrap().push((
                req.alvo_versao.clone(),
                req.perguntas_versao,
                req.politica_versao,
            ));
            tokio::time::sleep(self.espera).await;
            self.resposta.clone()
        }
    }

    fn escolha(categoria: Categoria, p: f64) -> Result<RespostaEscolha, ErroCategoria> {
        let mut probabilidades = [0.0; 9];
        probabilidades[categoria.indice()] = p;
        probabilidades[Categoria::Mapa.indice()] = 1.0 - p;
        Ok(RespostaEscolha {
            escolha: categoria,
            probabilidades,
        })
    }

    fn avaliador(vagas: usize, prazo_ms: u64) -> Avaliador {
        Avaliador::new(
            NonZeroUsize::new(vagas).unwrap(),
            Duration::from_millis(prazo_ms),
        )
    }

    async fn com(prep: &EstadoPreparado, duble: &Duble, limiar: f64) -> Classificacao {
        let avaliador = avaliador(1, 5_000);
        classificar(
            prep,
            limiar,
            Envio::Autorizado {
                provedor: duble,
                avaliador: &avaliador,
            },
        )
        .await
    }

    #[tokio::test]
    async fn no_content_never_calls() {
        let duble = Duble::new(escolha(Categoria::Holter, 0.9));
        let vazio = com(&preparar("  \n", "pdf", &[]), &duble, 0.7).await;
        assert_eq!(vazio.estado, EstadoClassificacao::SemConteudo);
        assert_eq!(vazio.motivo_codigo, Some(MotivoCodigo::SemConteudo));
        let pouco = com(&preparar("Exame 12", "pdf", &[]), &duble, 0.7).await;
        assert_eq!(pouco.estado, EstadoClassificacao::SemConteudo);
        assert_eq!(pouco.motivo_codigo, Some(MotivoCodigo::TextoInsuficiente));
        assert_eq!(pouco.model.as_deref(), Some("jev-1.13.0"));
        assert_eq!(pouco.adapter_version, "duble/1");
        assert_eq!(duble.chamadas.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_privacy_block_never_calls_and_counts_the_findings() {
        let duble = Duble::new(escolha(Categoria::Holter, 0.9));
        let prep = preparar(HOLTER, ".AB123.", &["AB123"]);
        assert!(!prep.bloqueio.is_empty());
        let c = com(&prep, &duble, 0.7).await;
        assert_eq!(c.estado, EstadoClassificacao::BloqueadoPrivacidade);
        assert_eq!(c.motivo_codigo, Some(MotivoCodigo::ConferenciaPrivacidade));
        assert_eq!(c.mascaramentos.get("conferencia_nome_conhecido"), Some(&1));
        assert_eq!(duble.chamadas.load(Ordering::SeqCst), 0);
        // Not authorized either: the block still wins over `preparado`.
        let c = classificar::<Duble>(&prep, 0.7, Envio::NaoAutorizado).await;
        assert_eq!(c.estado, EstadoClassificacao::BloqueadoPrivacidade);
        assert_eq!(c.motivo_codigo, Some(MotivoCodigo::ConferenciaPrivacidade));
    }

    #[tokio::test]
    async fn without_authorization_the_state_is_only_prepared() {
        let prep = preparar(HOLTER, "pdf", &[]);
        let nao_pedido = classificar::<Duble>(&prep, 0.7, Envio::NaoPedido).await;
        assert_eq!(nao_pedido.estado, EstadoClassificacao::Preparado);
        assert_eq!(nao_pedido.motivo_codigo, None);
        assert_eq!(nao_pedido.model, None);
        assert_eq!(nao_pedido.provider, PROVIDER);
        assert_eq!(nao_pedido.adapter_version, ADAPTER_VERSION);
        assert_eq!(nao_pedido.mascaramento_versao, MASCARAMENTO_VERSAO_PORTE);
        assert_eq!(nao_pedido.entrada_digest, hex::encode(prep.digest));
        let nao_autorizado = classificar::<Duble>(&prep, 0.7, Envio::NaoAutorizado).await;
        assert_eq!(nao_autorizado.estado, EstadoClassificacao::Preparado);
        assert_eq!(
            nao_autorizado.motivo_codigo,
            Some(MotivoCodigo::EnvioNaoAutorizado)
        );
        let vazio = preparar("", "pdf", &[]);
        let c = classificar::<Duble>(&vazio, 0.7, Envio::NaoAutorizado).await;
        assert_eq!(c.motivo_codigo, Some(MotivoCodigo::SemConteudo));
    }

    #[tokio::test]
    async fn a_valid_answer_is_decided_with_the_threshold() {
        let prep = preparar(HOLTER, "pdf", &[]);
        let duble = Duble::new(escolha(Categoria::Holter, 0.75));
        let ok = com(&prep, &duble, 0.75).await;
        assert_eq!(ok.estado, EstadoClassificacao::Ok);
        assert_eq!(ok.tipo, Some(Categoria::Holter));
        assert_eq!(ok.confianca, Some(0.75));
        assert!((ok.margem.unwrap() - 0.5).abs() < 1e-12);
        assert_eq!(ok.probabilidades.unwrap()[Categoria::Holter.indice()], 0.75);
        assert_eq!(ok.model.as_deref(), Some("jev-1.13.0-eco"));
        assert_eq!(ok.provider, "duble");
        assert_eq!(ok.provider_request_id.as_deref(), Some("req-1"));
        assert_eq!(ok.uso.unwrap().input_tokens, 120);
        assert_eq!(ok.confianca_provedor, Some(0.42));
        assert_eq!(ok.motivo_codigo, None);
        let incerto = com(&prep, &duble, 0.76).await;
        assert_eq!(incerto.estado, EstadoClassificacao::Incerto);
        assert_eq!(incerto.tipo, Some(Categoria::Holter));
        let alvos = duble.alvos.lock().unwrap();
        assert_eq!(alvos.len(), 2);
        assert_eq!(alvos[0].0, hex::encode(prep.digest)[..16]);
        assert_eq!(alvos[0].1, PERGUNTAS_VERSAO);
        assert_eq!(alvos[0].2, MASCARAMENTO_VERSAO_PORTE);
    }

    #[tokio::test]
    async fn a_failed_call_is_an_error_without_a_decision() {
        let prep = preparar(HOLTER, "pdf", &[]);
        let duble = Duble::new(Err(ErroCategoria::QuotaProvedor));
        let c = com(&prep, &duble, 0.7).await;
        assert_eq!(c.estado, EstadoClassificacao::Erro);
        assert_eq!(c.erro_categoria, Some(ErroCategoria::QuotaProvedor));
        assert_eq!(c.motivo_codigo, Some(MotivoCodigo::ProvedorErro));
        assert_eq!((c.tipo, c.confianca, c.probabilidades), (None, None, None));
        assert_eq!(c.confianca_provedor, None);
        assert_eq!(c.provider_request_id.as_deref(), Some("req-1"));
    }

    #[tokio::test]
    async fn the_deadline_includes_the_wait_for_a_slot() {
        let prep = preparar(HOLTER, "pdf", &[]);
        let mut duble = Duble::new(escolha(Categoria::Holter, 0.9));
        duble.espera = Duration::from_millis(200);
        let avaliador = avaliador(1, 300);
        let req = RequestNeutro::new(&prep);
        // The first call takes 200 ms of its 300; the second waits those 200 ms for the
        // slot, and its own 200 ms do not fit in what is left.
        let (primeira, segunda) = tokio::join!(
            avaliador.avaliar(&duble, &req),
            avaliador.avaliar(&duble, &req)
        );
        assert!(primeira.resultado.is_ok());
        assert!(primeira.latencia_ms >= 200);
        assert_eq!(segunda.resultado, Err(ErroCategoria::Timeout));
        assert_eq!(segunda.model, "jev-1.13.0");
        assert_eq!(segunda.provider_request_id, None);
        assert!(segunda.latencia_ms >= 300);
        // With two slots, both fit.
        let avaliador = avaliador_dois();
        let (a, b) = tokio::join!(
            avaliador.avaliar(&duble, &req),
            avaliador.avaliar(&duble, &req)
        );
        assert!(a.resultado.is_ok() && b.resultado.is_ok());
    }

    fn avaliador_dois() -> Avaliador {
        avaliador(2, 300)
    }

    fn chaves(valor: &Value, saida: &mut Vec<String>) {
        match valor {
            Value::Object(mapa) => {
                for (chave, v) in mapa {
                    saida.push(chave.clone());
                    chaves(v, saida);
                }
            }
            Value::Array(itens) => itens.iter().for_each(|v| chaves(v, saida)),
            _ => {}
        }
    }

    #[test]
    fn the_sent_state_has_only_the_minimal_keys() {
        let prep = preparar(HOLTER, "pdf", &[]);
        let mut todas = Vec::new();
        chaves(&prep.estado.para_value(), &mut todas);
        todas.sort();
        todas.dedup();
        assert_eq!(
            todas,
            [
                "arquivo",
                "extensao",
                "n",
                "paginas",
                "paginas_modo",
                "termos_nome_arquivo",
                "termos_titulo",
                "texto",
                "total_paginas"
            ]
        );
        assert!(
            todas
                .iter()
                .all(|c| !CHAVES_PROIBIDAS_NO_ESTADO.contains(&c.as_str()))
        );
    }
}
