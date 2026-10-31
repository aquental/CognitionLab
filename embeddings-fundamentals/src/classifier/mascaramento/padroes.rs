//! Layer 4 of the masking: registry numbers, documents, contacts and long digit runs
//! (TD §7.3, "Camada 4"; TestoDoc `mascaramento.py:400–473`).

use super::regexes::{Filtro, Regexes, regexes};
use super::{ErroMascaramento, Mapeado, Trecho, substituir};
use crate::classifier::categorias::REMOVIDO;
use crate::classifier::texto::dobrar;

/// `_nao_e_faixa` and `_nao_e_contagem`: whether a match at `[inicio, fim)` (code
/// points) is accepted, looking at the folded text around it.
fn aceitar(
    re: &Regexes,
    filtro: Filtro,
    dobrado: &Mapeado<'_>,
    inicio: usize,
    fim: usize,
) -> Result<bool, ErroMascaramento> {
    let antes = |padrao: &crate::classifier::pyre::PyRegex| {
        padrao.search(
            dobrado.s,
            dobrado.byte(inicio.saturating_sub(40)),
            dobrado.byte(inicio),
        )
    };
    Ok(match filtro {
        Filtro::Sempre => true,
        Filtro::NaoEFaixa => antes(&re.antes_de_faixa)?.is_none(),
        Filtro::NaoEContagem => {
            antes(&re.antes_de_contagem)?.is_none()
                && re
                    .depois_de_contagem
                    .match_at(dobrado.s, dobrado.byte(fim))?
                    .is_none()
        }
    })
}

/// `_camada_padroes`: registry numbers (the acronym stays) and labelled identifiers
/// first, then each pattern of `_PADROES` in turn on the text the previous one left.
pub(super) fn camada(texto: &str) -> Result<(String, usize), ErroMascaramento> {
    let re = regexes()?;
    let t = Mapeado::new(texto);
    let mut trechos: Vec<Trecho> = Vec::new();
    for captura in re.registro.finditer(texto) {
        let captura = captura?;
        let (Some(todo), Some(sigla)) = (captura.get(0), captura.name("sigla")) else {
            continue;
        };
        trechos.push((
            t.ch(todo.start())?,
            t.ch(todo.end())?,
            format!("{} {REMOVIDO}", sigla.as_str()),
        ));
    }
    for captura in re.identificador.finditer(texto) {
        if let Some(num) = captura?.name("num") {
            trechos.push((t.ch(num.start())?, t.ch(num.end())?, REMOVIDO.to_owned()));
        }
    }
    let mut n = trechos.len();
    let mut texto = substituir(texto, trechos);
    for (padrao, filtro) in &re.padroes {
        let dobrado = dobrar(&texto);
        let (t, d) = (Mapeado::new(&texto), Mapeado::new(&dobrado));
        let mut trechos: Vec<Trecho> = Vec::new();
        for captura in padrao.finditer(&texto) {
            let Some(m) = captura?.get(0) else {
                continue;
            };
            let (inicio, fim) = (t.ch(m.start())?, t.ch(m.end())?);
            if aceitar(re, *filtro, &d, inicio, fim)? {
                trechos.push((inicio, fim, REMOVIDO.to_owned()));
            }
        }
        n += trechos.len();
        texto = substituir(&texto, trechos);
    }
    Ok((texto, n))
}
