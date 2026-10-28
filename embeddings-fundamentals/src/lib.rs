//! Medical Document Embedding Lab.
//!
//! A reproducible pipeline that extracts anonymized medical documents and images,
//! embeds them through interchangeable engines, and evaluates retrieval quality for
//! Portuguese queries. See `spec/PRD.md` and `spec/PLAN.md`.

pub mod chunking;
pub mod cli;
pub mod corpus;
pub mod domain;
pub mod embedding;
pub mod extraction;
pub mod ids;
pub mod pipeline;
pub mod record;
pub mod retrieval;
pub mod storage;
pub mod tools;
