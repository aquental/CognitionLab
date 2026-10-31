//! Building the state sent to the provider (TestoDoc `montar_estado`; TD §6;
//! spec/CLASSIFIER.md §6.7).

use std::collections::BTreeMap;

use super::canonico;
use super::categorias::{DATA, LIMITE_PAGINA, LIMITE_TOTAL, REMOVIDO};
use super::indicios::nomes_dos_indicios;
use super::listas::{PALAVRAS_DE_ROTULO, TERMOS_GENERICOS, VOCABULARIO_TIPO};
use super::mascaramento::{ErroMascaramento, conferir, nomes_dos_rotulos, remover_dados_pessoais};
use super::texto::{compor, dobrar, prefixo, re_alnum, strip, strip_chars};
use super::tipos::{
    Arquivo, Camada, EntradaClassificacao, EstadoEnviado, EstadoPreparado, OrigemTexto,
    PaginaEnviada, PaginasModo,
};

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

fn somar(destino: &mut BTreeMap<Camada, u32>, origem: BTreeMap<Camada, u32>) {
    for (camada, n) in origem {
        let total = destino.entry(camada).or_default();
        *total = total.saturating_add(n);
    }
}

fn conta(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `_sem_marcador_partido`: drops the piece of a marker the cut left at the end
/// ("… Paciente: [REMO"); it says nothing, and its letters would count as useful text.
fn sem_marcador_partido(texto: &str) -> &str {
    if let Some(inicio) = texto.rfind('[') {
        let resto = texto.get(inicio..).unwrap_or_default();
        if [REMOVIDO, DATA]
            .iter()
            .any(|m| *m != resto && m.starts_with(resto))
        {
            return texto.get(..inicio).unwrap_or_default();
        }
    }
    texto
}

/// `_caracteres_uteis`: letters and digits outside markers and label words.
fn caracteres_uteis(texto: &str) -> usize {
    // The two markers never overlap, so replacing one and then the other is the same as
    // `re.sub(r"\[(?:REMOVIDO|DATA)\]", " ", texto)`.
    let sem_marcadores = texto.replace(REMOVIDO, " ").replace(DATA, " ");
    sem_marcadores
        .split(|ch: char| !re_alnum(ch))
        .filter(|p| !p.is_empty() && !PALAVRAS_DE_ROTULO.contains(&dobrar(p).as_str()))
        .map(|p| p.chars().count())
        .sum()
}

/// `_termo_do_tipo`: some vocabulary term other than the generic ones, in the file
/// name, the title or a sent page.
fn termo_do_tipo(estado: &EstadoEnviado) -> bool {
    let especifico = |t: &String| !TERMOS_GENERICOS.contains(&t.as_str());
    estado.arquivo.termos_nome_arquivo.iter().any(especifico)
        || estado.arquivo.termos_titulo.iter().any(especifico)
        || estado
            .paginas
            .iter()
            .any(|p| termos_permitidos(&p.texto).iter().any(especifico))
}

/// `montar_estado`: the minimal state for the provider, without personal data, already
/// checked, within the limits (TD §6, steps 0 to 10).
///
/// # Errors
///
/// [`ErroMascaramento`] if a masking regex fails; nothing may then be sent.
pub fn montar_estado(
    entrada: &EntradaClassificacao,
    modo: PaginasModo,
) -> Result<EstadoPreparado, ErroMascaramento> {
    // Step 0: every page in composed form before anything else.
    let compostas: Vec<String> = entrada.paginas.iter().map(|p| compor(&p.texto)).collect();
    // Pages are selected before the empty ones are dropped (A2).
    let selecionadas = modo
        .limite()
        .map_or(compostas.len(), |n| n.min(compostas.len()));
    let todo_texto = compostas.join("\n");
    let vocabulario: Vec<String> = VOCABULARIO_TIPO.iter().map(|t| (*t).to_owned()).collect();
    let mut conhecidos = entrada.nomes_conhecidos.clone();
    conhecidos.extend(nomes_dos_indicios(&todo_texto)?);
    conhecidos.extend(nomes_dos_rotulos(&todo_texto, &vocabulario)?);

    let mut mascaramentos = BTreeMap::new();
    let mut bloqueio = BTreeMap::new();
    let mut paginas: Vec<PaginaEnviada> = Vec::new();
    let mut origens: Vec<OrigemTexto> = Vec::new();
    let limite_total = usize::try_from(LIMITE_TOTAL).unwrap_or(usize::MAX);
    let limite_pagina = usize::try_from(LIMITE_PAGINA).unwrap_or(usize::MAX);
    let mut total = 0_usize;
    let mut truncado = false;
    for (pagina, texto) in entrada.paginas.iter().zip(&compostas).take(selecionadas) {
        if strip(texto).is_empty() {
            continue;
        }
        let restante = limite_total.saturating_sub(total);
        if restante == 0 {
            truncado = true;
            break;
        }
        let resultado = remover_dados_pessoais(texto, &conhecidos, &vocabulario)?;
        somar(&mut mascaramentos, resultado.contagens);
        // The whole masked page is checked before the cut (finding 45 of TestoDoc).
        somar(&mut bloqueio, conferir(&resultado.texto, &conhecidos)?);
        let limite = limite_pagina.min(restante);
        let mut texto = resultado.texto;
        if texto.chars().count() > limite {
            texto = sem_marcador_partido(prefixo(&texto, limite)).to_owned();
            truncado = true;
        }
        if texto.is_empty() {
            break;
        }
        total += texto.chars().count();
        paginas.push(PaginaEnviada { n: pagina.n, texto });
        if !origens.contains(&pagina.origem) {
            origens.push(pagina.origem);
        }
    }

    let estado = EstadoEnviado {
        arquivo: Arquivo {
            termos_nome_arquivo: termos_permitidos(&entrada.nome_arquivo),
            termos_titulo: termos_permitidos(&entrada.titulo),
            extensao: prefixo(strip_chars(&dobrar(&entrada.extensao), "."), 10).to_owned(),
            total_paginas: entrada.total_paginas,
        },
        paginas_modo: modo,
        paginas,
    };
    // The pages were checked whole in the loop; here, each other string of the state, one
    // by one, never the serialized JSON (A9).
    let outras = estado
        .arquivo
        .termos_nome_arquivo
        .iter()
        .chain(&estado.arquivo.termos_titulo)
        .map(String::as_str)
        .chain([estado.arquivo.extensao.as_str(), modo.as_str()]);
    for texto in outras {
        somar(&mut bloqueio, conferir(texto, &conhecidos)?);
    }
    // Sorted by the string value, as Python's `sorted` of the names.
    origens.sort_by_key(|o| o.as_str());
    let caracteres_uteis = estado
        .paginas
        .iter()
        .map(|p| caracteres_uteis(&p.texto))
        .sum();
    Ok(EstadoPreparado {
        digest: canonico::digest(&estado.para_value()),
        paginas_enviadas: conta(estado.paginas.len()),
        caracteres_enviados: conta(total),
        truncado,
        mascaramentos,
        origens_texto: origens,
        bloqueio,
        caracteres_uteis: conta(caracteres_uteis),
        termo_do_tipo: termo_do_tipo(&estado),
        estado,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::classifier::tipos::Pagina;

    fn entrada(
        paginas: Vec<(u32, String, OrigemTexto)>,
        extensao: &str,
        conhecidos: &[&str],
    ) -> EntradaClassificacao {
        EntradaClassificacao {
            nome_arquivo: "a.pdf".to_owned(),
            titulo: String::new(),
            extensao: extensao.to_owned(),
            total_paginas: conta(paginas.len()),
            paginas: paginas
                .into_iter()
                .map(|(n, texto, origem)| Pagina { n, texto, origem })
                .collect(),
            nomes_conhecidos: conhecidos.iter().map(|c| (*c).to_owned()).collect(),
        }
    }

    // Expected values come from TestoDoc's `montar_estado` on the same input.

    #[test]
    fn a_page_left_empty_by_the_cut_stops_the_loop() {
        let mut paginas: Vec<(u32, String, OrigemTexto)> = (1..=9)
            .map(|n| (n, "x".repeat(5999), OrigemTexto::Nativo))
            .collect();
        paginas.push((10, "x".repeat(6000), OrigemTexto::Nativo));
        // Page 11 masks to "[REMOVIDO] Holter", which 9 code points cut to nothing.
        paginas.push((11, "123.456.789-09 Holter".to_owned(), OrigemTexto::Nativo));
        paginas.push((12, "Holter de 24 h".to_owned(), OrigemTexto::Nativo));
        let prep = montar_estado(&entrada(paginas, "pdf", &[]), PaginasModo::Todas).unwrap();
        assert_eq!(prep.paginas_enviadas, 10);
        assert_eq!(prep.caracteres_enviados, 59_991);
        assert!(prep.truncado);
        assert_eq!(prep.estado.paginas.last().map(|p| p.n), Some(10));
    }

    #[test]
    fn origins_are_sorted_by_name() {
        let paginas = vec![
            (1, "Holter de 24 h".to_owned(), OrigemTexto::OcrLocal),
            (2, "Holter".to_owned(), OrigemTexto::Nativo),
        ];
        let prep = montar_estado(&entrada(paginas, "pdf", &[]), PaginasModo::Todas).unwrap();
        assert_eq!(
            prep.origens_texto,
            [OrigemTexto::Nativo, OrigemTexto::OcrLocal]
        );
    }

    #[test]
    fn strings_outside_the_pages_are_checked_one_by_one() {
        let paginas = vec![(1, "Holter de 24 h".to_owned(), OrigemTexto::Nativo)];
        let prep =
            montar_estado(&entrada(paginas, ".AB123.", &["AB123"]), PaginasModo::Uma).unwrap();
        assert_eq!(prep.estado.arquivo.extensao, "ab123");
        assert_eq!(prep.bloqueio, BTreeMap::from([(Camada::NomeConhecido, 1)]));
    }

    #[test]
    fn a_cut_marker_is_dropped() {
        assert_eq!(sem_marcador_partido("Paciente: [REMO"), "Paciente: ");
        assert_eq!(sem_marcador_partido("Data [DA"), "Data ");
        assert_eq!(
            sem_marcador_partido("x [REMOVIDO]"),
            "x [REMOVIDO]",
            "a whole marker stays"
        );
        assert_eq!(sem_marcador_partido("valor [3"), "valor [3", "not a marker");
    }

    #[test]
    fn useful_characters_skip_markers_and_label_words() {
        // "Paciente" and "Idade" are label words; "Holter", "24" and "h" count.
        assert_eq!(
            caracteres_uteis("Paciente: [REMOVIDO] Idade: [DATA]\nHolter 24 h"),
            9
        );
    }

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
