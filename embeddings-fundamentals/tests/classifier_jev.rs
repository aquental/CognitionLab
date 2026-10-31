//! The Jev adapter against a local HTTP server (spec/CLASSIFIER.md §14.5). No network:
//! a `TcpListener` on `127.0.0.1` answers with canned responses. Every input is synthetic.
#![cfg(feature = "classifier-jev")]
#![expect(clippy::unwrap_used, reason = "test code")]

use std::io;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use embeddings_fundamentals::classifier::jev::{
    Chave, ClienteJev, categoria_transporte, corpo_request,
};
use embeddings_fundamentals::classifier::{
    Avaliador, Categoria, Classificacao, EntradaClassificacao, Envio, ErroCategoria,
    EstadoClassificacao, EstadoPreparado, MotivoCodigo, OrigemTexto, Pagina, PaginasModo,
    RequestNeutro, RlcdProvider, classificar, estado::montar_estado,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const CHAVE: &str = "sk-sintetica-0123456789";
const TEXTO: &str = "Holter de 24 horas. Ritmo sinusal predominante, \
    extrassístoles ventriculares isoladas e raras, sem pausas significativas.";

/// One canned answer: the raw HTTP response and how long to wait before sending it.
struct Roteiro {
    resposta: Vec<u8>,
    espera: Duration,
}

fn http(status: &str, headers: &[(&str, &str)], corpo: &str) -> Vec<u8> {
    let mut s = format!(
        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n",
        corpo.len()
    );
    for (k, v) in headers {
        s.push_str(&format!("{k}: {v}\r\n"));
    }
    s.push_str("\r\n");
    s.push_str(corpo);
    s.into_bytes()
}

fn roteiro(resposta: Vec<u8>) -> Roteiro {
    Roteiro {
        resposta,
        espera: Duration::ZERO,
    }
}

/// Reads one request: the head, then `content-length` bytes of body.
async fn ler_pedido(s: &mut TcpStream) -> Vec<u8> {
    let mut lido = Vec::new();
    let mut buf = [0_u8; 4096];
    loop {
        let n = s.read(&mut buf).await.unwrap();
        if n == 0 {
            return lido;
        }
        lido.extend_from_slice(&buf[..n]);
        if let Some(fim) = lido.windows(4).position(|w| w == b"\r\n\r\n") {
            let cabeca = String::from_utf8_lossy(&lido[..fim]).to_lowercase();
            let tamanho: usize = cabeca
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .map_or(0, |v| v.trim().parse().unwrap());
            while lido.len() < fim + 4 + tamanho {
                let n = s.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                lido.extend_from_slice(&buf[..n]);
            }
            return lido;
        }
    }
}

/// A server that takes one connection per script entry, each in its own task, and
/// returns the raw requests in arrival order.
async fn servidor(roteiros: Vec<Roteiro>) -> (String, JoinHandle<Vec<Vec<u8>>>) {
    let escuta = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", escuta.local_addr().unwrap().port());
    let tarefa = tokio::spawn(async move {
        let mut tarefas = Vec::new();
        for r in roteiros {
            let (mut s, _) = escuta.accept().await.unwrap();
            tarefas.push(tokio::spawn(async move {
                let pedido = ler_pedido(&mut s).await;
                tokio::time::sleep(r.espera).await;
                let _ = s.write_all(&r.resposta).await;
                let _ = s.shutdown().await;
                pedido
            }));
        }
        let mut pedidos = Vec::new();
        for t in tarefas {
            pedidos.push(t.await.unwrap());
        }
        pedidos
    });
    (base, tarefa)
}

fn cliente(base: &str) -> ClienteJev {
    ClienteJev::new(
        Chave::new(CHAVE.to_owned()).unwrap(),
        "jev-1.13.0",
        Some(base),
    )
    .unwrap()
}

fn preparar() -> EstadoPreparado {
    let entrada = EntradaClassificacao {
        nome_arquivo: "documento.pdf".to_owned(),
        titulo: String::new(),
        extensao: "pdf".to_owned(),
        total_paginas: 1,
        paginas: vec![Pagina {
            n: 1,
            texto: TEXTO.to_owned(),
            origem: OrigemTexto::Nativo,
        }],
        nomes_conhecidos: Vec::new(),
    };
    montar_estado(&entrada, PaginasModo::Duas).unwrap()
}

fn probabilidades(categoria: Categoria, p: f64) -> Value {
    let mut m = serde_json::Map::new();
    for c in Categoria::TODAS {
        m.insert(
            c.valor().to_owned(),
            json!(if c == categoria { p } else { (1.0 - p) / 8.0 }),
        );
    }
    Value::Object(m)
}

fn corpo_ok(categoria: Categoria, p: f64) -> String {
    json!({
        "model": "jev-1.13.0",
        "answers": {"tipo_documento": {"type": "choice", "choice": categoria.valor(),
                                       "confidence": 0.77, "probabilities": probabilidades(categoria, p)}},
        "usage": {"input_tokens": 812, "output_tokens": 2},
    })
    .to_string()
}

async fn classificar_com(
    roteiros: Vec<Roteiro>,
    prazo: Duration,
) -> (Classificacao, Vec<Vec<u8>>, EstadoPreparado) {
    let (base, tarefa) = servidor(roteiros).await;
    let prep = preparar();
    let cliente = cliente(&base);
    let avaliador = Avaliador::new(NonZeroUsize::MIN, prazo);
    let c = classificar(
        &prep,
        0.7,
        Envio::Autorizado {
            provedor: &cliente,
            avaliador: &avaliador,
        },
    )
    .await;
    let pedidos = if c.erro_categoria == Some(ErroCategoria::Timeout) {
        tarefa.abort();
        Vec::new()
    } else {
        tarefa.await.unwrap()
    };
    (c, pedidos, prep)
}

#[tokio::test]
async fn a_valid_answer_sends_the_exact_body_and_headers() {
    let resposta = http(
        "200 OK",
        &[
            ("content-type", "application/json"),
            ("x-typesafe-request-id", "req-abc"),
        ],
        &corpo_ok(Categoria::Holter, 0.9),
    );
    let (c, pedidos, prep) = classificar_com(vec![roteiro(resposta)], Duration::from_secs(5)).await;
    assert_eq!(c.estado, EstadoClassificacao::Ok);
    assert_eq!(c.tipo, Some(Categoria::Holter));
    assert_eq!(c.confianca, Some(0.9));
    assert_eq!(c.confianca_provedor, Some(0.77));
    assert_eq!(c.provider_request_id.as_deref(), Some("req-abc"));
    assert_eq!(c.model.as_deref(), Some("jev-1.13.0"));
    assert_eq!(c.uso.map(|u| u.input_tokens), Some(812));
    assert_eq!(c.adapter_version, "cognitionlab-jev-http/1");

    let pedido = &pedidos[0];
    let fim = pedido.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let cabeca = String::from_utf8(pedido[..fim].to_vec()).unwrap();
    let mut linhas = cabeca.lines();
    assert_eq!(linhas.next(), Some("POST /v1/systemone HTTP/1.1"));
    let headers: Vec<(String, String)> = linhas
        .map(|l| {
            let (k, v) = l.split_once(':').unwrap();
            (k.to_lowercase(), v.trim().to_owned())
        })
        .collect();
    let header = |k: &str| {
        headers
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(
        header("authorization"),
        Some(format!("Bearer {CHAVE}").as_str())
    );
    assert_eq!(header("content-type"), Some("application/json"));
    assert_eq!(header("accept"), Some("application/json"));
    assert_eq!(
        header("user-agent"),
        Some(concat!(
            "cognitionlab-classifier/",
            env!("CARGO_PKG_VERSION")
        ))
    );
    assert_eq!(header("typesafe-danger-zone-long-context"), None);
    assert_eq!(header("content-encoding"), None);
    let corpo = &pedido[fim + 4..];
    let cliente = cliente("http://127.0.0.1:1");
    assert_eq!(
        corpo,
        corpo_request(&RequestNeutro::new(&prep), cliente.model()).as_bytes()
    );
    assert!(std::str::from_utf8(corpo).unwrap().starts_with(&format!(
        r#"{{"state":{},"model":"jev-1.13.0","questions":{{"tipo_documento":{{"type":"choice""#,
        prep.estado.json_fio()
    )));
}

#[tokio::test]
async fn a_missing_request_id_is_not_an_error() {
    let resposta = http("200 OK", &[], &corpo_ok(Categoria::Mapa, 0.5));
    let (c, _, _) = classificar_com(vec![roteiro(resposta)], Duration::from_secs(5)).await;
    assert_eq!(c.estado, EstadoClassificacao::Incerto);
    assert_eq!(c.provider_request_id, None);
}

#[tokio::test]
async fn an_invalid_200_keeps_the_request_id() {
    let resposta = http(
        "200 OK",
        &[("x-typesafe-request-id", "req-x")],
        "nao e json",
    );
    let (c, _, _) = classificar_com(vec![roteiro(resposta)], Duration::from_secs(5)).await;
    assert_eq!(c.estado, EstadoClassificacao::Erro);
    assert_eq!(c.erro_categoria, Some(ErroCategoria::RespostaInvalida));
    assert_eq!(c.motivo_codigo, Some(MotivoCodigo::ProvedorErro));
    assert_eq!(c.provider_request_id.as_deref(), Some("req-x"));
    assert_eq!(c.model.as_deref(), Some("jev-1.13.0"));
    assert_eq!(c.uso, None);
}

#[tokio::test]
async fn statuses_become_categories_without_reading_the_body() {
    for (status, categoria) in [
        ("400 Bad Request", ErroCategoria::RespostaInvalida),
        ("401 Unauthorized", ErroCategoria::Autenticacao),
        ("403 Forbidden", ErroCategoria::Autenticacao),
        ("404 Not Found", ErroCategoria::Outro),
        ("413 Payload Too Large", ErroCategoria::Outro),
        ("415 Unsupported Media Type", ErroCategoria::Outro),
        ("422 Unprocessable Entity", ErroCategoria::RespostaInvalida),
        ("429 Too Many Requests", ErroCategoria::QuotaProvedor),
        ("500 Internal Server Error", ErroCategoria::Outro),
        ("529 Overloaded", ErroCategoria::Outro),
        ("204 No Content", ErroCategoria::Outro),
    ] {
        let corpo = if status.starts_with("204") {
            String::new()
        } else {
            corpo_ok(Categoria::Holter, 0.9)
        };
        let resposta = http(status, &[("x-typesafe-request-id", "req-s")], &corpo);
        let (c, _, _) = classificar_com(vec![roteiro(resposta)], Duration::from_secs(5)).await;
        assert_eq!(c.estado, EstadoClassificacao::Erro, "{status}");
        assert_eq!(c.erro_categoria, Some(categoria), "{status}");
        assert_eq!(c.tipo, None, "{status}");
        assert_eq!(c.uso, None, "{status}");
    }
}

#[tokio::test]
async fn a_slow_server_is_a_timeout() {
    let lento = Roteiro {
        resposta: http("200 OK", &[], &corpo_ok(Categoria::Holter, 0.9)),
        espera: Duration::from_secs(3),
    };
    let (c, _, _) = classificar_com(vec![lento], Duration::from_millis(200)).await;
    assert_eq!(c.estado, EstadoClassificacao::Erro);
    assert_eq!(c.erro_categoria, Some(ErroCategoria::Timeout));
    assert!(c.latencia_ms >= 200);
}

#[tokio::test]
async fn waiting_for_a_slot_counts_in_the_deadline() {
    let roteiros = (0..2)
        .map(|_| Roteiro {
            resposta: http("200 OK", &[], &corpo_ok(Categoria::Holter, 0.9)),
            espera: Duration::from_millis(250),
        })
        .collect();
    let (base, tarefa) = servidor(roteiros).await;
    let prep = preparar();
    let cliente = cliente(&base);
    let avaliador = Avaliador::new(NonZeroUsize::MIN, Duration::from_millis(400));
    let req = RequestNeutro::new(&prep);
    let (a, b) = tokio::join!(
        avaliador.avaliar(&cliente, &req),
        avaliador.avaliar(&cliente, &req)
    );
    assert!(a.resultado.is_ok());
    assert_eq!(b.resultado, Err(ErroCategoria::Timeout));
    tarefa.abort();
}

#[tokio::test]
async fn a_closed_port_is_a_network_error() {
    let escuta = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", escuta.local_addr().unwrap().port());
    drop(escuta);
    let prep = preparar();
    let cliente = cliente(&base);
    let avaliador = Avaliador::new(NonZeroUsize::MIN, Duration::from_secs(5));
    let r = avaliador
        .avaliar(&cliente, &RequestNeutro::new(&prep))
        .await;
    assert_eq!(r.resultado, Err(ErroCategoria::Rede));
}

#[tokio::test]
async fn a_cut_body_is_a_network_error() {
    let mut resposta = http("200 OK", &[], &corpo_ok(Categoria::Holter, 0.9));
    resposta.truncate(resposta.len() - 20);
    let (c, _, _) = classificar_com(vec![roteiro(resposta)], Duration::from_secs(5)).await;
    assert_eq!(c.erro_categoria, Some(ErroCategoria::Rede));
}

#[tokio::test]
async fn a_reqwest_timeout_is_a_timeout_not_a_network_error() {
    let lento = Roteiro {
        resposta: http("200 OK", &[], "{}"),
        espera: Duration::from_secs(3),
    };
    let (base, tarefa) = servidor(vec![lento]).await;
    let erro = reqwest::Client::builder()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap()
        .post(format!("{base}/v1/systemone"))
        .send()
        .await
        .unwrap_err();
    assert!(erro.is_timeout());
    assert_eq!(categoria_transporte(&erro), ErroCategoria::Timeout);
    tarefa.abort();
}

#[derive(Clone, Default)]
struct Registro(Arc<Mutex<Vec<u8>>>);

impl io::Write for Registro {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Every log line of this test binary. A global subscriber, so that the callsites of the
/// tests running in parallel cannot cache "no interest" (which a per-thread subscriber
/// allows): the check below then covers the logs of every test here.
fn registro() -> &'static Registro {
    static REGISTRO: OnceLock<Registro> = OnceLock::new();
    REGISTRO.get_or_init(|| {
        let registro = Registro::default();
        let escritor = registro.clone();
        tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_writer(move || escritor.clone())
                .finish(),
        )
        .unwrap();
        registro
    })
}

#[tokio::test]
async fn logs_carry_neither_the_key_nor_the_error_body_nor_the_text() {
    let registro = registro();
    let eco = format!(r#"{{"detail":"SEGREDO-DO-CORPO {TEXTO}"}}"#);
    for resposta in [
        http("422 Unprocessable Entity", &[], &eco),
        http("200 OK", &[], &eco),
    ] {
        let (c, _, _) = classificar_com(vec![roteiro(resposta)], Duration::from_secs(5)).await;
        assert_eq!(c.estado, EstadoClassificacao::Erro);
    }
    let logs = String::from_utf8(registro.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("jev:"), "the failures are logged: {logs}");
    for proibido in [CHAVE, "SEGREDO-DO-CORPO", "Ritmo sinusal", "extrassístoles"] {
        assert!(!logs.contains(proibido), "{proibido} in {logs}");
    }
}
