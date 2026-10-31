//! The patient's name from TestoDoc's patient hints, used as a known name
//! (TD §7.1, §7.2; TestoDoc `patient_hints.py` and `classificador_documento.py:229–248`).
//!
//! Neither function composes its input: they get what the caller passes
//! (spec/CLASSIFIER.md §6.1).

use std::sync::LazyLock;

use super::listas::{PALAVRAS_DE_ROTULO, TERMOS_CLINICOS, VOCABULARIO_TIPO};
use super::mascaramento::{ErroMascaramento, partes_de_nome};
use super::pyre::{ErroPyre, Flags, PyRegex, re_escape};
use super::texto::{dobrar, primeira_linha, split, str_isupper, strip_chars, title, upper};

/// Regex 1 of `extract_patient_hints`: "Paciente:" / "Nome:" on the same line or the
/// next; IGNORECASE as a whole, its classes included.
pub const NOME_REGEX_1: &str = r"(?:paciente|nome\s*(?:do\s*paciente)?)\s*[:\-]?\s*\n?\s*([A-ZÁÉÍÓÚÂÊÔÃÕÇ][A-Za-zÁÉÍÓÚÂÊÔÃÕÇáéíóúâêôãõç\s]{3,60})";

/// Regex 2 (GE/Philips): a protocol line, then a line with the name in capitals; no
/// flags.
pub const NOME_REGEX_2: &str = r"(?:Protocolo:|Nome:)\s*(?:\n|\r\n)+(?:[^\n]{0,40}\n)?([A-ZÁÉÍÓÚÂÊÔÃÕÇ][A-ZÁÉÍÓÚÂÊÔÃÕÇ\s]{5,50})";

/// Values that are not a name (`nome.upper()` in this set is refused).
const NAO_SAO_NOME: [&str; 4] = ["NOME", "PACIENTE", "DT NASC", "SOLICITANTE"];

struct Regexes {
    nome_1: PyRegex,
    nome_2: PyRegex,
    espacos: PyRegex,
}

static REGEXES: LazyLock<Result<Regexes, ErroPyre>> = LazyLock::new(|| {
    Ok(Regexes {
        nome_1: PyRegex::new(NOME_REGEX_1, Flags::IGNORECASE)?,
        nome_2: PyRegex::new(NOME_REGEX_2, Flags::NENHUMA)?,
        espacos: PyRegex::new(r"\s+", Flags::NENHUMA)?,
    })
});

fn regexes() -> Result<&'static Regexes, ErroPyre> {
    REGEXES.as_ref().map_err(Clone::clone)
}

/// The text of group 1 of the first match of `regex` in `texto`.
fn grupo_1<'t>(regex: &PyRegex, texto: &'t str) -> Result<Option<&'t str>, ErroPyre> {
    match regex.finditer(texto).next() {
        Some(captura) => Ok(captura?.get(1).map(|m| m.as_str())),
        None => Ok(None),
    }
}

/// The `nome` field of TestoDoc's `extract_patient_hints`, or `None`.
///
/// # Errors
///
/// [`ErroMascaramento`] if a regex fails.
pub fn nome_dos_indicios(texto: &str) -> Result<Option<String>, ErroMascaramento> {
    if texto.is_empty() {
        return Ok(None);
    }
    let re = regexes()?;
    let grupo = match grupo_1(&re.nome_1, texto)? {
        Some(g) => Some(g),
        None => grupo_1(&re.nome_2, texto)?,
    };
    let Some(grupo) = grupo else {
        return Ok(None);
    };
    let espacado = re.espacos.sub(grupo, " ")?;
    let nome = strip_chars(&espacado, " .,-");
    if NAO_SAO_NOME.contains(&upper(nome).as_str()) || split(nome).count() < 2 {
        return Ok(None);
    }
    Ok(Some(if str_isupper(nome) {
        title(nome)
    } else {
        nome.to_owned()
    }))
}

/// `_FIM_DO_NOME`: a word that ends the name.
fn fim_do_nome(palavra: &str) -> bool {
    PALAVRAS_DE_ROTULO.contains(&palavra)
        || VOCABULARIO_TIPO.contains(&palavra)
        || TERMOS_CLINICOS.contains(&palavra)
}

/// `_nomes_dos_indicios`: the hints' name, as the folded name parts joined by spaces.
/// The heuristic crosses line breaks and joins neighbouring words; only the line where
/// the name starts stays, cut at the first label word or exam term.
///
/// # Errors
///
/// [`ErroMascaramento`] if a regex fails.
pub fn nomes_dos_indicios(texto: &str) -> Result<Vec<String>, ErroMascaramento> {
    let nome = nome_dos_indicios(texto)?.unwrap_or_default();
    let mut palavras: Vec<String> = split(&nome).map(str::to_owned).collect();
    if palavras.is_empty() {
        return Ok(Vec::new());
    }
    let escapadas: Vec<String> = palavras.iter().map(|p| re_escape(p)).collect();
    let busca = PyRegex::new(&escapadas.join(r"\s+"), Flags::IGNORECASE)?;
    if let Some(captura) = busca.finditer(texto).next()
        && let Some(achado) = captura?.get(0)
    {
        palavras = split(primeira_linha(achado.as_str()))
            .map(str::to_owned)
            .collect();
    }
    let linha: Vec<&str> = palavras
        .iter()
        .map(String::as_str)
        .take_while(|p| !fim_do_nome(&dobrar(p)))
        .collect();
    let partes = partes_de_nome(&linha.join(" "));
    Ok(if partes.is_empty() {
        Vec::new()
    } else {
        vec![partes.join(" ")]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values come from TestoDoc's `extract_patient_hints` on the same input.

    #[test]
    fn an_all_caps_name_is_title_cased() {
        assert_eq!(
            nome_dos_indicios("Paciente: ODETE RAMOS LINS\n")
                .unwrap()
                .as_deref(),
            Some("Odete Ramos Lins")
        );
        assert_eq!(
            nome_dos_indicios("Paciente: Odete RAMOS\n")
                .unwrap()
                .as_deref(),
            Some("Odete RAMOS")
        );
        assert_eq!(nome_dos_indicios("").unwrap(), None);
    }
}
