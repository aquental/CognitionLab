//! Medical Document Embedding Lab.
//!
//! A reproducible pipeline that extracts anonymized medical documents and images,
//! embeds them through interchangeable engines, and evaluates retrieval quality for
//! Portuguese queries. See `spec/PRD.md` and `spec/PLAN.md`.

pub mod adapters;
pub mod anonymization;
pub mod assets;
pub mod chunking;
pub mod cli;
pub mod config;
pub mod corpus;
pub mod cost;
pub mod domain;
pub mod embedding;
pub mod evaluation;
pub mod experiment;
pub mod extraction;
pub mod fidelity;
pub mod ids;
pub mod measurement;
pub mod pipeline;
pub mod record;
pub mod report;
pub mod retrieval;
pub mod storage;
pub mod tools;
