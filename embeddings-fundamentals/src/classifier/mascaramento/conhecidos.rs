//! Layer 2 of the masking: known names and identifiers (TD §7.3, "Camada 2").

use crate::classifier::listas::PARTICULAS;
use crate::classifier::texto::{compor, dobrar, re_letra};

/// The folded parts of a name, with 3 code points or more and no particles, in order
/// and without repeats (TestoDoc `partes_de_nome`). Parts are the `[^\W\d_]+` runs of
/// `dobrar(compor(nome))`.
#[must_use]
pub fn partes_de_nome(nome: &str) -> Vec<String> {
    let dobrado = dobrar(&compor(nome));
    let mut vistas: Vec<String> = Vec::new();
    for parte in dobrado.split(|ch: char| !re_letra(ch)) {
        if parte.chars().count() >= 3
            && !PARTICULAS.contains(&parte)
            && !vistas.iter().any(|v| v == parte)
        {
            vistas.push(parte.to_owned());
        }
    }
    vistas
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_drops_short_parts_particles_and_repeats() {
        assert_eq!(
            partes_de_nome("Maria dos Santos e Silva Santos"),
            ["maria", "santos", "silva"]
        );
        assert_eq!(partes_de_nome("Jo Bo"), Vec::<String>::new());
        assert_eq!(partes_de_nome("Ana Del Rey"), ["ana", "rey"]);
    }

    #[test]
    fn composes_before_folding() {
        assert_eq!(partes_de_nome("Joa\u{303}o"), ["joao"]);
    }

    #[test]
    fn letters_include_non_decimal_numbers_like_python() {
        // `[^\W\d_]` includes `²`: "Ana²" is one part, not "ana".
        assert_eq!(partes_de_nome("Ana² Lima3"), ["ana²", "lima"]);
    }
}
