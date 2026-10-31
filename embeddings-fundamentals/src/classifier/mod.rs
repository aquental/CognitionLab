//! Exam-type classifier: a Rust port of TestoDoc's RLCD/Jev classifier.
//!
//! The specification is `spec/CLASSIFIER.md`. Behaviour is fixed by TestoDoc's
//! conformance vectors (`tests/fixtures/classifier/vetores.json`), which
//! `tests/classifier_vectors.rs` reads directly. The core (masking, state, digest and
//! decision) is pure: no I/O and no global mutable state.

pub mod categorias;
pub mod tipos;

pub use categorias::{
    CRITERIO_ID, Categoria, CategoriaDesconhecida, DATA, ETAPA, LIMITE_PAGINA, LIMITE_TOTAL,
    MASCARAMENTO_VERSAO, PERGUNTA, PERGUNTAS_VERSAO, REMOVIDO, TEXTO_UTIL_MIN, TOLERANCIA_LIMIAR,
    TOLERANCIA_SOMA_PROBABILIDADES, UNICODE_VERSAO,
};
pub use tipos::{
    Arquivo, Camada, EntradaClassificacao, ErroCategoria, EstadoClassificacao, EstadoEnviado,
    EstadoPreparado, MotivoCli, MotivoCodigo, OrigemTexto, Pagina, PaginaEnviada, PaginasModo,
    PaginasModoInvalido, RespostaEscolha, Uso,
};
