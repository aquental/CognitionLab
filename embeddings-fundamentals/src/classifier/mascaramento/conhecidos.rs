//! Layer 2 of the masking: known names and identifiers (TD §7.3, "Camada 2").

use std::cmp::Reverse;

use super::{ErroMascaramento, Mapeado, Trecho, substituir};
use crate::classifier::categorias::REMOVIDO;
use crate::classifier::listas::PARTICULAS;
use crate::classifier::pyre::{Flags, PyRegex, re_escape};
use crate::classifier::texto::{compor, dobrar, is_digit, re_letra, strip};

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

/// Python's `sorted(itens, key=lambda p: (-len(p), p))`.
fn ordenados(itens: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = itens.into_iter().collect();
    v.sort_by(|a, b| {
        (Reverse(a.chars().count()), a.as_str()).cmp(&(Reverse(b.chars().count()), b.as_str()))
    });
    v.dedup();
    v
}

/// `_padroes_conhecidos`: one regex with every name part between `\b`, then one per
/// literal identifier (a known value with a digit and 3 code points or more), longest
/// first. Both run on the folded text.
fn padroes_conhecidos(conhecidos: &[String]) -> Result<Vec<PyRegex>, ErroMascaramento> {
    let mut partes: Vec<String> = Vec::new();
    let mut literais: Vec<String> = Vec::new();
    for c in conhecidos {
        let c = compor(c);
        let c = strip(&c);
        if c.is_empty() {
            continue;
        }
        if c.chars().any(is_digit) {
            if c.chars().count() >= 3 {
                literais.push(dobrar(c));
            }
        } else {
            partes.extend(partes_de_nome(c));
        }
    }
    let mut padroes = Vec::new();
    let partes = ordenados(partes);
    if !partes.is_empty() {
        let alternativas: Vec<String> = partes.iter().map(|p| re_escape(p)).collect();
        padroes.push(PyRegex::new(
            &format!(r"\b(?:{})\b", alternativas.join("|")),
            Flags::NENHUMA,
        )?);
    }
    for literal in ordenados(literais) {
        padroes.push(PyRegex::new(&re_escape(&literal), Flags::NENHUMA)?);
    }
    Ok(padroes)
}

/// `_camada_conhecidos`: each pattern in turn, on the text the previous one left.
pub(super) fn camada(
    texto: &str,
    conhecidos: &[String],
) -> Result<(String, usize), ErroMascaramento> {
    let mut texto = texto.to_owned();
    let mut n = 0;
    for padrao in padroes_conhecidos(conhecidos)? {
        let dobrado = dobrar(&texto);
        let d = Mapeado::new(&dobrado);
        let mut trechos: Vec<Trecho> = Vec::new();
        for captura in padrao.finditer(&dobrado) {
            if let Some(m) = captura?.get(0) {
                trechos.push((d.ch(m.start())?, d.ch(m.end())?, REMOVIDO.to_owned()));
            }
        }
        n += trechos.len();
        texto = substituir(&texto, trechos);
    }
    Ok((texto, n))
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
