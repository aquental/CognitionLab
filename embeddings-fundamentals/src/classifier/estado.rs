//! Building the state sent to the provider (TestoDoc `montar_estado`; TD §6). Phase 3
//! adds the rest; this phase holds the vocabulary filter.

use super::listas::VOCABULARIO_TIPO;
use super::texto::{compor, dobrar};

/// The `[a-z0-9]+` tokens of `dobrar(compor(texto))` that are in `VOCABULARIO_TIPO`, in
/// order of first occurrence and without repeats (TestoDoc `termos_permitidos`).
/// Everything else in a file name or title stays out of the state.
#[must_use]
pub fn termos_permitidos(texto: &str) -> Vec<String> {
    let dobrado = dobrar(&compor(texto));
    let mut termos: Vec<String> = Vec::new();
    for termo in dobrado.split(|ch: char| !(ch.is_ascii_lowercase() || ch.is_ascii_digit())) {
        if VOCABULARIO_TIPO.contains(&termo) && !termos.iter().any(|t| t == termo) {
            termos.push(termo.to_owned());
        }
    }
    termos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_vocabulary_terms_leave() {
        assert_eq!(
            termos_permitidos("Ana_Lima_holter_24h_HOLTER.pdf"),
            ["holter"]
        );
        assert_eq!(termos_permitidos("ECO 3D strain"), ["eco", "3d", "strain"]);
        assert_eq!(termos_permitidos("digitalizado_0042"), Vec::<String>::new());
    }

    #[test]
    fn folds_accents_and_composes_first() {
        assert_eq!(
            termos_permitidos("PRESSÃO arterial"),
            ["pressao", "arterial"]
        );
        assert_eq!(termos_permitidos("pressa\u{303}o"), ["pressao"]);
    }

    #[test]
    fn non_ascii_letters_split_tokens() {
        // `[a-z0-9]` is ASCII: "ecoß" splits into "eco" and nothing.
        assert_eq!(termos_permitidos("ecoßmapa"), ["eco", "mapa"]);
    }
}
