//! Layer 5 of the masking: dates, written out (on the folded text) and then numeric
//! (TD §7.3, "Camada 5"; TestoDoc `mascaramento.py:476–498`).

use super::regexes::regexes;
use super::{ErroMascaramento, Mapeado, Trecho, substituir};
use crate::classifier::categorias::DATA;
use crate::classifier::pyre::PyRegex;
use crate::classifier::texto::dobrar;

fn trechos(padrao: &PyRegex, onde: &str) -> Result<Vec<Trecho>, ErroMascaramento> {
    let m = Mapeado::new(onde);
    let mut saida = Vec::new();
    for captura in padrao.finditer(onde) {
        if let Some(achado) = captura?.get(0) {
            saida.push((m.ch(achado.start())?, m.ch(achado.end())?, DATA.to_owned()));
        }
    }
    Ok(saida)
}

/// `_camada_datas`.
pub(super) fn camada(texto: &str) -> Result<(String, usize), ErroMascaramento> {
    let re = regexes()?;
    let extenso = trechos(&re.datas_extenso, &dobrar(texto))?;
    let n = extenso.len();
    let texto = substituir(texto, extenso);
    let numericas = trechos(&re.datas_numericas, &texto)?;
    let n = n + numericas.len();
    Ok((substituir(&texto, numericas), n))
}
