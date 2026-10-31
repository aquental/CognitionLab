//! Layer 3 of the masking: the name after a title (Dr., Dra., Sr., Prof.…)
//! (TD §7.3, "Camada 3"; TestoDoc `mascaramento.py:357–397`).

use super::regexes::regexes;
use super::{ErroMascaramento, Mapeado, Trecho, substituir};
use crate::classifier::categorias::REMOVIDO;
use crate::classifier::listas::TERMOS_CLINICOS;
use crate::classifier::texto::{dobrar, is_upper, re_espaco};

/// The `\S+` runs of `s`, as (start, end) in code points.
fn palavras(s: &str) -> Vec<(usize, usize, &str)> {
    let mut saida = Vec::new();
    let mut inicio: Option<(usize, usize)> = None;
    for (i, (byte, ch)) in s.char_indices().enumerate() {
        match (inicio, re_espaco(ch)) {
            (None, false) => inicio = Some((i, byte)),
            (Some((a, b)), true) => {
                saida.push((a, i, s.get(b..byte).unwrap_or_default()));
                inicio = None;
            }
            _ => {}
        }
    }
    if let Some((a, b)) = inicio {
        saida.push((a, s.chars().count(), s.get(b..).unwrap_or_default()));
    }
    saida
}

/// `_camada_tratamentos`: each name ends before the first acronym or report term and
/// before the first word that does not start with a capital; particles only stay
/// between two words of the name (`_fim_do_nome`).
pub(super) fn camada(texto: &str, preservar: &[&str]) -> Result<(String, usize), ErroMascaramento> {
    let re = regexes()?;
    let t = Mapeado::new(texto);
    let parada = |s: &str| TERMOS_CLINICOS.contains(&s) || preservar.contains(&s);
    let mut trechos: Vec<Trecho> = Vec::new();
    for captura in re.tratamento.finditer(texto) {
        let captura = captura?;
        let Some(nome) = captura.name("nome") else {
            continue;
        };
        let inicio = t.ch(nome.start())?;
        let mut fim = inicio;
        for (_, fim_palavra, palavra) in palavras(nome.as_str()) {
            if parada(&dobrar(palavra)) {
                break;
            }
            if re.particula_de_nome.fullmatch(palavra)? {
                continue;
            }
            if !palavra.chars().next().is_some_and(is_upper) {
                break;
            }
            fim = inicio + fim_palavra;
        }
        if fim > inicio {
            trechos.push((inicio, fim, REMOVIDO.to_owned()));
        }
    }
    let n = trechos.len();
    Ok((substituir(texto, trechos), n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_runs_of_non_space_in_code_points() {
        assert_eq!(
            palavras(" Ana\u{1f}Lima  Sá "),
            [(1, 4, "Ana"), (5, 9, "Lima"), (11, 13, "Sá")]
        );
        assert!(palavras("").is_empty());
    }
}
