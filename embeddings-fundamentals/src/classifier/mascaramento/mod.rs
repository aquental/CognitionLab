//! Removal of personal data before anything leaves the machine (TestoDoc
//! `mascaramento.py`; spec/CLASSIFIER.md §6.5). Later phases add the five layers, the
//! marker collapse and the final check.

pub mod conhecidos;

pub use conhecidos::partes_de_nome;
