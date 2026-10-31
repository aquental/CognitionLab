//! Layer 1 of the masking: labelled fields, in a line or in a table extracted by
//! columns (TD §7.3, "Camada 1"; TestoDoc `mascaramento.py:83–310`).

use super::regexes::{Regexes, regexes};
use super::{ErroMascaramento, Mapeado, Trecho, substituir};
use crate::classifier::categorias::{DATA, REMOVIDO};
use crate::classifier::listas::{
    NAO_E_NOME, PALAVRAS_DE_ROTULO, PARTICULAS, ROTULOS, ROTULOS_COM_NUMERO, ROTULOS_COMPOSTOS,
    ROTULOS_DE_CABECALHO, ROTULOS_DE_DATA, ROTULOS_DE_NOME, SEXOS, TERMOS_CLINICOS,
};
use crate::classifier::texto::{
    compor, dobrar, is_digit, is_upper, lstrip, rstrip, split, strip, strip_chars,
};

/// `_MAX_ROTULOS_EM_COLUNA`.
const MAX_ROTULOS_EM_COLUNA: usize = 8;

/// A label's value: (label, start, end), in code points.
type Valor = (String, usize, usize);

fn len(s: &str) -> usize {
    s.chars().count()
}

/// `" ".join(s.split())`.
fn normalizar(s: &str) -> String {
    split(s).collect::<Vec<_>>().join(" ")
}

/// `_valor_mascarado`: the value becomes `[REMOVIDO]`, except the `preservar` terms,
/// which stay. Returns the new value and whether something other than an existing
/// marker was removed.
fn valor_mascarado(valor: &str, preservar: &[&str]) -> (String, bool) {
    let mut partes: Vec<&str> = Vec::new();
    let mut removeu = false;
    for palavra in split(valor) {
        if preservar.contains(&strip_chars(&dobrar(palavra), ".,;:()")) {
            partes.push(palavra);
            continue;
        }
        removeu = removeu || (palavra != REMOVIDO && palavra != DATA);
        if partes.last() != Some(&REMOVIDO) {
            partes.push(REMOVIDO);
        }
    }
    // Python returns `valor.strip()` when it equals the new value: the same string.
    (partes.join(" "), removeu)
}

/// `_valores_em_linha`: (label, start, end) of each value after "Label:" on the same
/// line or, with the label alone at the end of the line, on the next non-empty line.
fn valores_em_linha(
    re: &Regexes,
    texto: &Mapeado<'_>,
    dobrado: &Mapeado<'_>,
) -> Result<Vec<Valor>, ErroMascaramento> {
    let mut achados = Vec::new();
    for captura in re.rotulo.finditer(dobrado.s) {
        let captura = captura?;
        let (Some(todo), Some(rot)) = (captura.get(0), captura.name("rot")) else {
            continue;
        };
        achados.push((
            dobrado.ch(todo.start())?,
            dobrado.ch(todo.end())?,
            rot.as_str().to_owned(),
        ));
    }
    let mut valores = Vec::new();
    for (i, (_, m_fim, rot)) in achados.iter().enumerate() {
        let rotulo = normalizar(rot);
        let fim_linha = dobrado.fim_da_linha(*m_fim)?;
        let mut fim = fim_linha;
        if let Some((proximo, _, _)) = achados.get(i + 1)
            && *proximo < fim
        {
            fim = *proximo;
        }
        if let Some((celula, _)) =
            re.fim_de_celula
                .search(dobrado.s, dobrado.byte(*m_fim), dobrado.byte(fim))?
        {
            fim = dobrado.ch(celula)?;
        }
        let valor = texto.fatia(*m_fim, fim);
        if !strip(valor).is_empty() {
            valores.push((rotulo, *m_fim, m_fim + len(rstrip(valor))));
            continue;
        }
        if fim != fim_linha || fim_linha >= texto.len() {
            continue;
        }
        let mut prox = fim_linha + 1;
        while prox < texto.len() {
            let fim_prox = texto.fim_da_linha(prox)?;
            let linha = texto.fatia(prox, fim_prox);
            if !strip(linha).is_empty() {
                if re.rotulo.match_at(dobrado.s, dobrado.byte(prox))?.is_none() {
                    let inicio = prox + (len(linha) - len(lstrip(linha)));
                    valores.push((rotulo, inicio, prox + len(rstrip(linha))));
                }
                break;
            }
            prox = fim_prox + 1;
        }
    }
    Ok(valores)
}

/// `_rotulo_composto`: 2 or 3 words, the first one in `_ROTULOS_COMPOSTOS`.
fn rotulo_composto(rotulo: &str) -> bool {
    let partes: Vec<&str> = split(rotulo).collect();
    (2..=3).contains(&partes.len())
        && partes
            .first()
            .is_some_and(|p| ROTULOS_COMPOSTOS.contains(p))
}

fn e_pessoal(rotulo: &str) -> bool {
    ROTULOS.contains(&rotulo) || rotulo_composto(rotulo)
}

fn e_de_nome(rotulo: &str) -> bool {
    ROTULOS_DE_NOME.contains(&rotulo)
        || (rotulo_composto(rotulo)
            && split(rotulo)
                .next()
                .is_some_and(|p| ["medico", "medica", "responsavel"].contains(&p)))
}

/// `_rotulo_de_linha`: the label, if the whole line is just a label ("Altura (cm)",
/// "Médico", "Estilo de vida:").
fn rotulo_de_linha(re: &Regexes, linha_dobrada: &str) -> Result<Option<String>, ErroMascaramento> {
    let sem_parenteses = re.parenteses.sub(linha_dobrada, "")?;
    let r = normalizar(strip(strip(&sem_parenteses).trim_end_matches(':')));
    let e_rotulo = ROTULOS.contains(&r.as_str())
        || ROTULOS_DE_CABECALHO.contains(&r.as_str())
        || rotulo_composto(&r);
    Ok(e_rotulo.then_some(r))
}

/// `_valor_compativel`: does the value in the label's position have the expected
/// shape? One mismatch drops the whole block.
fn valor_compativel(
    re: &Regexes,
    rotulo: &str,
    valor: &str,
    preservar: &[&str],
) -> Result<bool, ErroMascaramento> {
    let dobrado = dobrar(valor);
    let dobrado = strip(&dobrado);
    if e_de_nome(rotulo) {
        return Ok(parece_nome(re, valor, preservar)?.is_some());
    }
    if rotulo.starts_with("data") || ROTULOS_DE_DATA.contains(&rotulo) {
        let tudo = |s: &str| (0, s.len());
        let (a, b) = tudo(valor);
        let (c, d) = tudo(dobrado);
        return Ok(re.datas_numericas.search(valor, a, b)?.is_some()
            || re.datas_extenso.search(dobrado, c, d)?.is_some());
    }
    if rotulo == "sexo" {
        return Ok(SEXOS.contains(&dobrado));
    }
    if ROTULOS_COM_NUMERO.contains(&rotulo) {
        return Ok(valor.chars().any(is_digit));
    }
    Ok(true)
}

/// `_valores_em_colunas`: (label, start, end) of the values of a table extracted by
/// columns, matched by position. Not a sliding window (TD §7.3, "Avanço do laço").
fn valores_em_colunas(
    re: &Regexes,
    texto: &Mapeado<'_>,
    dobrado: &Mapeado<'_>,
    preservar: &[&str],
) -> Result<Vec<Valor>, ErroMascaramento> {
    let mut linhas: Vec<(usize, usize)> = Vec::new();
    let mut pos = 0;
    for linha in texto.s.split('\n') {
        if !strip(linha).is_empty() {
            let inicio = pos + (len(linha) - len(lstrip(linha)));
            linhas.push((inicio, pos + len(rstrip(linha))));
        }
        pos += len(linha) + 1;
    }
    let mut valores = Vec::new();
    let mut i = 0;
    while i < linhas.len() {
        let mut rotulos: Vec<String> = Vec::new();
        let mut j = i;
        while let Some(&(a, b)) = linhas.get(j) {
            if rotulos.len() >= MAX_ROTULOS_EM_COLUNA {
                break;
            }
            let Some(r) = rotulo_de_linha(re, dobrado.fatia(a, b))? else {
                break;
            };
            rotulos.push(r);
            j += 1;
        }
        if rotulos.len() < 2 {
            i = if j == i { j + 1 } else { j };
            continue;
        }
        let candidatos = linhas
            .get(j..(j + rotulos.len()).min(linhas.len()))
            .unwrap_or_default();
        let mut casam = candidatos.len() == rotulos.len();
        if casam {
            for (r, &(a, b)) in rotulos.iter().zip(candidatos) {
                if rotulo_de_linha(re, dobrado.fatia(a, b))?.is_some()
                    || !valor_compativel(re, r, texto.fatia(a, b), preservar)?
                {
                    casam = false;
                    break;
                }
            }
        }
        if casam {
            valores.extend(
                rotulos
                    .iter()
                    .zip(candidatos)
                    .map(|(r, &(a, b))| (r.clone(), a, b)),
            );
            i = j + rotulos.len();
        } else {
            i = j;
        }
    }
    Ok(valores)
}

/// `_camada_rotulos`: the personal values in columns, then the values in lines, all
/// masked; the count is taken before overlaps are dropped.
pub(super) fn camada(texto: &str, preservar: &[&str]) -> Result<(String, usize), ErroMascaramento> {
    let re = regexes()?;
    let dobrado = dobrar(texto);
    let (t, d) = (Mapeado::new(texto), Mapeado::new(&dobrado));
    let mut todos: Vec<Valor> = valores_em_colunas(re, &t, &d, preservar)?
        .into_iter()
        .filter(|(rotulo, _, _)| e_pessoal(rotulo))
        .collect();
    todos.extend(valores_em_linha(re, &t, &d)?);
    let mut trechos: Vec<Trecho> = Vec::with_capacity(todos.len());
    let mut n = 0;
    for (_, inicio, fim) in todos {
        let (novo, removeu) = valor_mascarado(t.fatia(inicio, fim), preservar);
        n += usize::from(removeu);
        trechos.push((inicio, fim, novo));
    }
    Ok((substituir(texto, trechos), n))
}

/// `_parece_nome`: the name, without title and with normalized spaces, if the value
/// looks like a person's name: 2 to 6 words of letters, the first one capitalized, no
/// acronym, report term, label word nor exam-type term. When in doubt, it is not a name.
fn parece_nome(
    re: &Regexes,
    valor: &str,
    preservar: &[&str],
) -> Result<Option<String>, ErroMascaramento> {
    let sem_tratamento = re.tratamento_no_inicio.sub(strip(valor), "")?;
    let palavras: Vec<&str> = split(&sem_tratamento).collect();
    let inicial_maiuscula = palavras
        .first()
        .and_then(|p| p.chars().next())
        .is_some_and(is_upper);
    if !(2..=6).contains(&palavras.len()) || !inicial_maiuscula {
        return Ok(None);
    }
    let parada = |s: &str| {
        TERMOS_CLINICOS.contains(&s)
            || PALAVRAS_DE_ROTULO.contains(&s)
            || preservar.contains(&s)
            || NAO_E_NOME.contains(&s)
    };
    for palavra in &palavras {
        let dobrada = dobrar(palavra);
        let dobrada = dobrada.trim_end_matches('.');
        if !re.palavra_de_nome.fullmatch(palavra)? || (len(dobrada) >= 2 && parada(dobrada)) {
            return Ok(None);
        }
    }
    let fora_particulas = palavras
        .iter()
        .filter(|p| !PARTICULAS.contains(&dobrar(p).as_str()))
        .count();
    if fora_particulas < 2 {
        return Ok(None);
    }
    Ok(Some(palavras.join(" ")))
}

/// The names in name labels of an already composed text (`nomes_dos_rotulos` without
/// its `compor`).
pub(super) fn nomes(texto: &str, preservar: &[&str]) -> Result<Vec<String>, ErroMascaramento> {
    let re = regexes()?;
    let dobrado = dobrar(texto);
    let (t, d) = (Mapeado::new(texto), Mapeado::new(&dobrado));
    let mut valores = valores_em_linha(re, &t, &d)?;
    valores.extend(valores_em_colunas(re, &t, &d, preservar)?);
    // Python's `sorted(key=start)` is stable, as `sort_by_key` is.
    valores.sort_by_key(|&(_, inicio, _)| inicio);
    let mut nomes: Vec<String> = Vec::new();
    for (rotulo, inicio, fim) in valores {
        if e_de_nome(&rotulo)
            && let Some(nome) = parece_nome(re, t.fatia(inicio, fim), preservar)?
            && !nomes.contains(&nome)
        {
            nomes.push(nome);
        }
    }
    Ok(nomes)
}

/// `nomes_dos_rotulos`: names of people in the values of name labels (patient,
/// physician, person in charge…), in a line or in columns, in order and without
/// repeats. The classifier treats them as known names in the whole document.
///
/// # Errors
///
/// [`ErroMascaramento`] if a regex fails.
pub fn nomes_dos_rotulos(
    texto: &str,
    preservar: &[String],
) -> Result<Vec<String>, ErroMascaramento> {
    let preservar: Vec<&str> = preservar.iter().map(String::as_str).collect();
    nomes(&compor(texto), &preservar)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values come from TestoDoc's `mascaramento.py` on the same input.

    #[test]
    fn masked_value_keeps_preserved_terms_and_merges_markers() {
        assert_eq!(
            valor_mascarado("Ana  Lima Holter (24 h)", &["holter"]),
            ("[REMOVIDO] Holter [REMOVIDO]".to_owned(), true)
        );
        assert_eq!(
            valor_mascarado("[REMOVIDO] [DATA]", &[]),
            ("[REMOVIDO]".to_owned(), false),
            "markers alone remove nothing new"
        );
    }

    fn nome(valor: &str) -> Option<String> {
        parece_nome(regexes().unwrap(), valor, &[]).unwrap()
    }

    #[test]
    fn a_name_has_two_to_six_words_and_two_that_are_not_particles() {
        assert_eq!(
            nome("Ana Luiza Costa Prado Ramos Lins").as_deref(),
            Some("Ana Luiza Costa Prado Ramos Lins")
        );
        assert_eq!(nome("Ana Luiza Costa Prado Ramos Lins Vilar"), None);
        assert_eq!(nome("Ana de"), None);
        assert_eq!(nome("Dra. Ana da Lima").as_deref(), Some("Ana da Lima"));
    }
}
