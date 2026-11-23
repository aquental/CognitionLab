//! Exam-type classifier: a Rust port of StealthApp's RLCD/Jev classifier.
//!
//! The specification is `spec/CLASSIFIER.md`. Behaviour is fixed by StealthApp's
//! conformance vectors (`tests/fixtures/classifier/vectors.json`), which
//! `tests/classifier_vectors.rs` reads directly. The core (masking, state, digest and
//! decision) is pure: no I/O and no global mutable state.

pub mod canonical;
pub mod categories;
pub mod decision;
#[cfg(feature = "classifier-jev")]
pub mod jev;
pub mod masking;
pub mod patient_hints;
pub mod provider;
pub mod pyclasses;
pub mod pyre;
pub mod state;
pub mod text;
pub mod types;
pub mod word_lists;

pub use categories::{
    CRITERIO_ID, Categoria, CategoriaDesconhecida, DATA, ETAPA, LIMITE_PAGINA, LIMITE_TOTAL,
    MASCARAMENTO_VERSAO, MASCARAMENTO_VERSAO_PORTE, PERGUNTA, PERGUNTAS_VERSAO, REMOVIDO,
    TEXTO_UTIL_MIN, TOLERANCIA_LIMIAR, TOLERANCIA_SOMA_PROBABILIDADES, UNICODE_VERSAO,
};
pub use provider::{
    ADAPTER_VERSION, Avaliador, CRITERIO_TIPO_DOCUMENTO, CriterioEscolha, Envio, MODELO_PADRAO,
    PROVIDER, RequestNeutro, RespostaProvedor, RlcdProvider, classificar,
};
pub use types::{
    Arquivo, Camada, Classificacao, EntradaClassificacao, ErroCategoria, EstadoClassificacao,
    EstadoEnviado, EstadoPreparado, MotivoCli, MotivoCodigo, OrigemTexto, Pagina, PaginaEnviada,
    PaginasModo, PaginasModoInvalido, RespostaEscolha, Uso, serializar_probabilidades,
};
