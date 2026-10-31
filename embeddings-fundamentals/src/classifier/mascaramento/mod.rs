//! Removal of personal data before anything leaves the machine (TestoDoc
//! `mascaramento.py`; spec/CLASSIFIER.md §6.5).
//!
//! The layers run in TestoDoc's order: labelled fields, known names (the given ones and
//! the ones found in name labels), titles (Dr., Dra.…), patterns and dates; then runs of
//! markers collapse. The marker never carries part of the value. [`conferir`] looks again
//! for known names, patterns and dates; anything found blocks the sending.
//!
//! Every position is a code-point offset, as in Python (spec/CLASSIFIER.md §6.2). A
//! search on the folded text gives byte offsets in it; they become code points through
//! its map, and code points become bytes of the original text through the original's
//! map.

use std::collections::BTreeMap;

use super::categorias::REMOVIDO;
use super::pyre::ErroPyre;
use super::texto::{MapaOffsets, compor};
use super::tipos::Camada;

pub mod conhecidos;
mod datas;
mod padroes;
mod regexes;
mod rotulos;
mod tratamento;

pub use conhecidos::partes_de_nome;
pub use regexes::{TextoPython, textos_auxiliares, textos_python};
pub use rotulos::nomes_dos_rotulos;

/// A failure while masking. Python's `re` never fails; here the regex engine can, and
/// the caller must then send nothing.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ErroMascaramento {
    /// A regex failed to compile or to run.
    #[error("masking regex failed")]
    Regex(#[from] ErroPyre),
    /// A match did not end on a code-point boundary (a bug).
    #[error("match offset is not at a code-point boundary")]
    Offset,
}

/// The masked text and the removals per layer (TestoDoc `Resultado`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resultado {
    /// The text after the five layers and the collapse of markers.
    pub texto: String,
    /// Removals per layer, only the layers above zero.
    pub contagens: BTreeMap<Camada, u32>,
}

/// A string and its code-point map.
pub(crate) struct Mapeado<'a> {
    pub s: &'a str,
    pub mapa: MapaOffsets,
}

impl<'a> Mapeado<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            s,
            mapa: MapaOffsets::new(s),
        }
    }

    /// Length in code points.
    pub fn len(&self) -> usize {
        self.mapa.len()
    }

    /// Byte offset of a code-point offset, clamped to the end.
    pub fn byte(&self, ch: usize) -> usize {
        self.mapa.byte(ch)
    }

    /// Code-point offset of a byte offset that a match gave.
    pub fn ch(&self, byte: usize) -> Result<usize, ErroMascaramento> {
        self.mapa.char_de_byte(byte).ok_or(ErroMascaramento::Offset)
    }

    /// `s[inicio:fim]` in code points.
    pub fn fatia(&self, inicio: usize, fim: usize) -> &'a str {
        self.mapa.fatia(self.s, inicio, fim)
    }

    /// `s.find("\n", desde)`, or the length when there is none.
    pub fn fim_da_linha(&self, desde: usize) -> Result<usize, ErroMascaramento> {
        let b = self.byte(desde);
        match self.s.get(b..).and_then(|resto| resto.find('\n')) {
            Some(i) => self.ch(b + i),
            None => Ok(self.len()),
        }
    }
}

/// One replacement: code-point range and new text.
pub(crate) type Trecho = (usize, usize, String);

/// `_substituir`: replaces the ranges left to right, in the order of
/// `(inicio, fim, novo)`, skipping a range that starts before the end of the last one
/// kept.
pub(crate) fn substituir(texto: &str, mut trechos: Vec<Trecho>) -> String {
    trechos.sort();
    let mapa = MapaOffsets::new(texto);
    let mut saida = String::with_capacity(texto.len());
    let mut pos = 0;
    for (inicio, fim, novo) in trechos {
        if inicio < pos {
            continue;
        }
        saida.push_str(mapa.fatia(texto, pos, inicio));
        saida.push_str(&novo);
        pos = fim;
    }
    saida.push_str(mapa.fatia(texto, pos, mapa.len()));
    saida
}

fn conta(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn sem_zeros(contagens: impl IntoIterator<Item = (Camada, usize)>) -> BTreeMap<Camada, u32> {
    contagens
        .into_iter()
        .filter(|&(_, n)| n > 0)
        .map(|(camada, n)| (camada, conta(n)))
        .collect()
}

/// `remover_dados_pessoais`: removes the personal data the layers recognise.
/// `conhecidos` are names (or literal identifiers) to remove; `preservar` are exam-type
/// terms, folded, that stay inside a label's value.
///
/// # Errors
///
/// [`ErroMascaramento`] if a regex fails; nothing may then be sent.
pub fn remover_dados_pessoais(
    texto: &str,
    conhecidos: &[String],
    preservar: &[String],
) -> Result<Resultado, ErroMascaramento> {
    let preservar: Vec<&str> = preservar.iter().map(String::as_str).collect();
    let texto = compor(texto);
    // The labelled name comes back without a label in the same text (header, signature).
    let mut todos: Vec<String> = conhecidos.to_vec();
    todos.extend(rotulos::nomes(&texto, &preservar)?);
    let (texto, rotulo) = rotulos::camada(&texto, &preservar)?;
    let (texto, nome_conhecido) = conhecidos::camada(&texto, &todos)?;
    let (texto, tratamento) = tratamento::camada(&texto, &preservar)?;
    let (texto, padrao) = padroes::camada(&texto)?;
    let (texto, data) = datas::camada(&texto)?;
    let texto = regexes::regexes()?
        .sequencia_de_marcadores
        .sub(&texto, REMOVIDO)?;
    Ok(Resultado {
        texto,
        contagens: sem_zeros([
            (Camada::Rotulo, rotulo),
            (Camada::NomeConhecido, nome_conhecido),
            (Camada::Tratamento, tratamento),
            (Camada::Padrao, padrao),
            (Camada::Data, data),
        ]),
    })
}

/// `conferir`: looks again for known names, patterns and dates. An empty map means
/// nothing was found; it never returns the values.
///
/// # Errors
///
/// [`ErroMascaramento`] if a regex fails; nothing may then be sent.
pub fn conferir(
    texto: &str,
    conhecidos: &[String],
) -> Result<BTreeMap<Camada, u32>, ErroMascaramento> {
    let (texto, nome_conhecido) = conhecidos::camada(&compor(texto), conhecidos)?;
    let (texto, padrao) = padroes::camada(&texto)?;
    let (_, data) = datas::camada(&texto)?;
    Ok(sem_zeros([
        (Camada::NomeConhecido, nome_conhecido),
        (Camada::Padrao, padrao),
        (Camada::Data, data),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classifier::texto::dobrar;
    use proptest::prelude::*;

    /// Pieces of exam-like text; every name is invented.
    const PECAS: &[&str] = &[
        "Paciente:",
        "Nome:\n",
        "Médico responsável:",
        "Idade:",
        "CPF:",
        "Telefone:",
        "Dr.",
        "Dra.",
        "Zanoni Prado",
        "ANA LUIZA COSTA",
        "Nilo Teixeira",
        "Carla dos Anjos",
        "e",
        "de",
        "FEVE 62 %",
        "Holter de 24 h",
        "VO2 pico 2500-3500",
        "123.456.789-09",
        "(11) 98765-4321",
        "1234567",
        "CRM/SP 123456",
        "12/03/1990",
        "12 de março de 1990",
        "[REMOVIDO]",
        "[DATA]",
    ];

    /// Pieces separated or glued: a known name glued to a digit or `_` must go too
    /// (DV10; TestoDoc lets it through, spec §0.4 F10).
    fn texto() -> impl Strategy<Value = String> {
        let separador = proptest::sample::select(vec!["", "_", " ", "  ", "\t", "\n", " | ", ": "]);
        proptest::collection::vec((proptest::sample::select(PECAS.to_vec()), separador), 0..24)
            .prop_map(|v| v.into_iter().map(|(p, sep)| format!("{p}{sep}")).collect())
    }

    /// `pdftotext -layout` pads columns with long runs of spaces. Some layer-1 patterns
    /// cost a quadratic number of steps on them (TestoDoc takes 1.8 s on 3 000 spaces;
    /// these halves still pass `fancy-regex`'s default limit of 1 000 000 steps);
    /// Python's `re` has no step limit, so the port must not have one either. Expected
    /// values from TestoDoc (Python 3.14): every text unchanged, nothing counted.
    #[test]
    fn long_whitespace_runs_do_not_hit_a_backtracking_limit() {
        let vocabulario: Vec<String> = crate::classifier::listas::VOCABULARIO_TIPO
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        for texto in [
            " ".repeat(1500),
            format!("{}\n", " ".repeat(100)).repeat(15),
            format!(
                "Ritmo sinusal{}FC media 71 bpm{}Pausas: nenhuma\n",
                " ".repeat(40),
                " ".repeat(40)
            )
            .repeat(20),
        ] {
            let r = remover_dados_pessoais(&texto, &[], &vocabulario).unwrap();
            assert_eq!(r.texto, texto);
            assert!(r.contagens.is_empty());
            assert!(nomes_dos_rotulos(&texto, &vocabulario).unwrap().is_empty());
            assert!(conferir(&texto, &[]).unwrap().is_empty());
        }
    }

    #[test]
    fn masking_is_not_idempotent_as_in_testodoc() {
        // TestoDoc's `mascaramento.py` gives these same outputs (spec §0.4, F9).
        let s = "Paciente:: Médico responsável:\t123.456.789-09: ";
        let uma = remover_dados_pessoais(s, &[], &[]).unwrap().texto;
        assert_eq!(uma, "Paciente:[REMOVIDO]: ");
        let duas = remover_dados_pessoais(&uma, &[], &[]).unwrap().texto;
        assert_eq!(duas, "Paciente:[REMOVIDO] ");
        let uma = remover_dados_pessoais("CRM/SP 123456CRM/SP 123456Paciente:", &[], &[])
            .unwrap()
            .texto;
        assert_eq!(uma, "CRM [REMOVIDO]CRM/SP 123456Paciente:");
        assert_eq!(
            remover_dados_pessoais(&uma, &[], &[]).unwrap().texto,
            "CRM [REMOVIDO]CRM [REMOVIDO]Paciente:"
        );
        // A known name glued to a registry number: TestoDoc kept "Zanoni"; here it goes
        // (DV10).
        let conhecidos = vec!["Zanoni Prado".to_owned()];
        let colado = remover_dados_pessoais(
            "CRM/SP 123456Zanoni Prado Zanoni Prado CRM/SP 123456Zanoni Prado",
            &conhecidos,
            &[],
        )
        .unwrap();
        assert_eq!(
            colado.texto,
            "CRM [REMOVIDO][REMOVIDO] CRM [REMOVIDO][REMOVIDO]"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Invariant 5: after removing a known invented name, no part of it with 3 or
        /// more letters is left in the text.
        #[test]
        fn no_part_of_a_known_name_is_left(
            s in texto(),
            nome in proptest::sample::select(vec!["Zanoni Prado", "Nilo Teixeira", "Carla dos Anjos"]),
        ) {
            let conhecidos = vec![nome.to_owned()];
            let saida = remover_dados_pessoais(&format!("{s}{nome}_{s}{nome} {s}"), &conhecidos, &[]).unwrap();
            let dobrado = dobrar(&saida.texto);
            for parte in partes_de_nome(nome) {
                prop_assert!(
                    !dobrado.split(|c: char| !crate::classifier::texto::re_letra(c)).any(|w| w == parte),
                    "{parte} left in {:?}", saida.texto
                );
            }
        }
    }

    fn t(inicio: usize, fim: usize, novo: &str) -> Trecho {
        (inicio, fim, novo.to_owned())
    }

    #[test]
    fn a_known_name_glued_to_digits_or_underscores_is_removed() {
        // Deliberate divergence from TestoDoc (spec §13, DV10; §0.4 F10): its `\b` treats
        // digits and `_` as word characters, so these names went out untouched and the
        // final check missed them too.
        let conhecidos = vec!["Zanoni Prado".to_owned()];
        for (texto, esperado) in [
            ("Ref 12345Zanoni Prado", "Ref 12345[REMOVIDO]"),
            (
                "arquivo_Zanoni_Prado_eco",
                "arquivo_[REMOVIDO]_[REMOVIDO]_eco",
            ),
            ("Zanoni2 Prado3", "[REMOVIDO]2 [REMOVIDO]3"),
        ] {
            assert_eq!(
                conferir(texto, &conhecidos)
                    .unwrap()
                    .get(&Camada::NomeConhecido),
                Some(&2),
                "the check finds both parts in {texto:?}"
            );
            let saida = remover_dados_pessoais(texto, &conhecidos, &[]).unwrap();
            assert_eq!(saida.texto, esperado);
            assert_eq!(
                conferir(&saida.texto, &conhecidos).unwrap(),
                BTreeMap::new()
            );
        }
        // A part inside a longer word is not a name: letters still bound it.
        let ana = vec!["Ana Lima".to_owned()];
        assert_eq!(
            remover_dados_pessoais("Banana limao", &ana, &[])
                .unwrap()
                .texto,
            "Banana limao"
        );
    }

    #[test]
    fn known_literals_need_three_code_points_and_go_longest_first() {
        // TestoDoc: "12" is too short; "xab12" runs before "ab1", and "ab1" before "x12".
        let conhecidos: Vec<String> = ["X12", "AB1", "xab12", "12"].map(str::to_owned).to_vec();
        let r = remover_dados_pessoais("ref X12 e AB12 e XAB12 e 12", &conhecidos, &[]).unwrap();
        assert_eq!(r.texto, "ref [REMOVIDO]2 e [REMOVIDO] e 12");
        assert_eq!(r.contagens, BTreeMap::from([(Camada::NomeConhecido, 3)]));
    }

    #[test]
    fn substituir_sorts_and_skips_overlaps() {
        assert_eq!(
            substituir("abcdef", vec![t(3, 5, "Y"), t(0, 2, "X"), t(1, 4, "Z")]),
            "XcYf"
        );
        // Same start: the shorter range sorts first and wins.
        assert_eq!(
            substituir("abcdef", vec![t(1, 4, "L"), t(1, 2, "S")]),
            "aScdef"
        );
        assert_eq!(
            substituir("çãé", vec![t(1, 2, "[X]")]),
            "ç[X]é",
            "code points"
        );
        assert_eq!(substituir("ab", vec![]), "ab");
    }
}
