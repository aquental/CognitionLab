//! The nine exam types, the question sent to the provider, and the versioned constants
//! of the classifier (spec/CLASSIFIER.md §5.1; TestoDoc `classificador_documento.CATEGORIAS`).
//!
//! The texts are normative: they are copied from the conformance vectors
//! (`tests/fixtures/classifier/vetores.json`, section `criterio`), and
//! `tests/classifier_vectors.rs` checks them against the file. Changing any of them needs
//! a new [`PERGUNTAS_VERSAO`] in TestoDoc first.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Id of the single `Choice` criterion (`tipo_documento`).
pub const CRITERIO_ID: &str = "tipo_documento";
/// The question asked to the provider.
pub const PERGUNTA: &str = "Que tipo de exame é este documento? Considere os termos do nome do arquivo, os termos do título e o conteúdo das páginas. Dados pessoais foram removidos e aparecem como [REMOVIDO] ou [DATA].";
/// Version of the question and option texts; part of a classification's identity.
pub const PERGUNTAS_VERSAO: &str = "classificador-documento-1";
/// Version of TestoDoc's masking rules that the vectors were made with; any change in
/// masking output needs a new one there.
pub const MASCARAMENTO_VERSAO: &str = "mascaramento-5";
/// Version of the port's masking: TestoDoc's [`MASCARAMENTO_VERSAO`] plus the owner's
/// deliberate change DV10 (known names glued to digits or `_`; spec/CLASSIFIER.md §13).
/// It is what a classification and a request carry.
pub const MASCARAMENTO_VERSAO_PORTE: &str = "mascaramento-5+cognitionlab-1";
/// Unicode version every character table and normalization must follow.
pub const UNICODE_VERSAO: &str = "16.0.0";
/// Stage name of the neutral request.
pub const ETAPA: &str = "classificacao_documento";
/// Most code points kept from one page after masking.
pub const LIMITE_PAGINA: u32 = 6000;
/// Most code points sent in total.
pub const LIMITE_TOTAL: u32 = 60000;
/// Fewer useful characters than this, with no exam-type term, means no content.
pub const TEXTO_UTIL_MIN: u32 = 40;
/// Tolerance of the threshold comparison: a confidence equal to the threshold is `ok`.
pub const TOLERANCIA_LIMIAR: f64 = 1e-9;
/// Tolerance of the probabilities' sum around 1.
pub const TOLERANCIA_SOMA_PROBABILIDADES: f64 = 1e-3;
/// Marker that replaces a removed value.
pub const REMOVIDO: &str = "[REMOVIDO]";
/// Marker that replaces a removed date.
pub const DATA: &str = "[DATA]";

/// One of the nine answers, in the order of TestoDoc's `CATEGORIAS` tuple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Categoria {
    /// Pulse wave velocity and arterial stiffness.
    Vop,
    /// Conventional transthoracic echocardiogram.
    EcoConvencional,
    /// Echocardiogram with strain or other advanced techniques.
    EcoAvancado,
    /// Resting indirect calorimetry.
    Calorimetria,
    /// Cardiopulmonary exercise test.
    Ergoespirometria,
    /// Ambulatory ECG monitoring.
    Holter,
    /// Ambulatory blood pressure monitoring.
    Mapa,
    /// Resting electrocardiogram.
    Ecg,
    /// Anything else.
    Outro,
}

impl Categoria {
    /// All nine, in the order of the tuple: the order of the options on the wire
    /// (spec/CLASSIFIER.md §8.2) and of every list of probabilities.
    pub const TODAS: [Self; 9] = [
        Self::Vop,
        Self::EcoConvencional,
        Self::EcoAvancado,
        Self::Calorimetria,
        Self::Ergoespirometria,
        Self::Holter,
        Self::Mapa,
        Self::Ecg,
        Self::Outro,
    ];

    /// The option value sent to the provider.
    #[must_use]
    pub const fn valor(self) -> &'static str {
        match self {
            Self::Vop => "vop",
            Self::EcoConvencional => "eco_convencional",
            Self::EcoAvancado => "eco_avancado",
            Self::Calorimetria => "calorimetria",
            Self::Ergoespirometria => "ergoespirometria",
            Self::Holter => "holter",
            Self::Mapa => "mapa",
            Self::Ecg => "ecg",
            Self::Outro => "outro",
        }
    }

    /// The label shown to people.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Vop => "VOP",
            Self::EcoConvencional => "Eco convencional",
            Self::EcoAvancado => "Eco avançado",
            Self::Calorimetria => "Calorimetria",
            Self::Ergoespirometria => "Ergoespirometria",
            Self::Holter => "Holter",
            Self::Mapa => "MAPA",
            Self::Ecg => "ECG",
            Self::Outro => "Outro",
        }
    }

    /// The description the provider reads to decide.
    #[must_use]
    pub const fn descricao(self) -> &'static str {
        match self {
            Self::Vop => {
                "Velocidade de onda de pulso e rigidez arterial: VOP carótido-femoral, índice de aumento (AIx), pressão aórtica central, aparelhos como SphygmoCor, Complior ou Mobil-O-Graph."
            }
            Self::EcoConvencional => {
                "Ecocardiograma transtorácico convencional: modo M, bidimensional e Doppler (dimensões das câmaras, FEVE por Teichholz ou Simpson, valvas, fluxos), sem análise de deformação miocárdica."
            }
            Self::EcoAvancado => {
                "Ecocardiograma com técnicas avançadas: strain ou speckle tracking (strain longitudinal global, SLG/GLS, strain do VD ou do AE), mapa polar \"bull's-eye\", twist ou torção, eco 3D."
            }
            Self::Calorimetria => {
                "Calorimetria indireta em repouso: taxa metabólica de repouso (TMR), VO2 e VCO2 em repouso, quociente respiratório, gasto energético diário e substrato oxidado. Não há esforço físico nem protocolo de carga."
            }
            Self::Ergoespirometria => {
                "Teste cardiopulmonar de exercício (ergoespirometria, TCPE ou CPET) em esteira ou cicloergômetro: protocolo de carga, VO2 pico ou máximo, limiares ventilatórios (L1/L2, VT1/VT2), VE/VCO2, pulso de oxigênio, frequência cardíaca e pressão arterial no esforço."
            }
            Self::Holter => {
                "Monitorização eletrocardiográfica ambulatorial (Holter de 24 h, 48 h, 7 dias ou mais): ritmo de base, frequência cardíaca mínima, média e máxima, extrassístoles, arritmias, pausas, variabilidade da frequência cardíaca, segmento ST ao longo do registro."
            }
            Self::Mapa => {
                "Monitorização ambulatorial da pressão arterial (MAPA de 24 h): médias de pressão sistólica e diastólica na vigília, no sono e nas 24 h, descenso noturno, cargas pressóricas, número de medidas válidas."
            }
            Self::Ecg => {
                "Eletrocardiograma de repouso, em geral de 12 derivações: ritmo, frequência, eixo, intervalos PR, QRS e QT/QTc, alterações de repolarização, num registro curto."
            }
            Self::Outro => {
                "Qualquer outro documento: exames de sangue ou laboratoriais, teste ergométrico sem análise de gases, exame de imagem, receita, atestado, ou um texto que não é exame."
            }
        }
    }

    /// TestoDoc's `documento_tipos.slug`; `None` for [`Categoria::Outro`].
    #[must_use]
    pub const fn slug(self) -> Option<&'static str> {
        match self {
            Self::Vop => Some("vop"),
            Self::EcoConvencional => Some("eco-convencional"),
            Self::EcoAvancado => Some("eco-strain-avancado"),
            Self::Calorimetria => Some("calorimetria"),
            Self::Ergoespirometria => Some("ergoespirometria"),
            Self::Holter => Some("holter"),
            Self::Mapa => Some("mapa"),
            Self::Ecg => Some("ecg"),
            Self::Outro => None,
        }
    }

    /// Position in [`Categoria::TODAS`].
    #[must_use]
    pub const fn indice(self) -> usize {
        self as usize
    }
}

impl fmt::Display for Categoria {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.valor())
    }
}

/// An option value that is not one of the nine.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown exam type")]
pub struct CategoriaDesconhecida;

impl FromStr for Categoria {
    type Err = CategoriaDesconhecida;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::TODAS
            .into_iter()
            .find(|c| c.valor() == s)
            .ok_or(CategoriaDesconhecida)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_masking_version_extends_the_vectors_one() {
        // DV10 changes the masking output, so the port carries its own version.
        assert_ne!(MASCARAMENTO_VERSAO_PORTE, MASCARAMENTO_VERSAO);
        assert!(MASCARAMENTO_VERSAO_PORTE.starts_with(&format!("{MASCARAMENTO_VERSAO}+")));
    }

    #[test]
    fn indice_is_the_position_in_todas() {
        for (i, c) in Categoria::TODAS.into_iter().enumerate() {
            assert_eq!(c.indice(), i);
        }
    }

    #[test]
    fn valor_round_trips_through_from_str_and_serde() {
        for c in Categoria::TODAS {
            assert_eq!(c.valor().parse::<Categoria>(), Ok(c));
            let serializado = serde_json::to_string(&c).unwrap();
            assert_eq!(serializado, format!("\"{}\"", c.valor()));
            assert_eq!(serde_json::from_str::<Categoria>(&serializado).unwrap(), c);
            assert_eq!(c.to_string(), c.valor());
        }
        assert!(serde_json::from_str::<Categoria>("\"eco-strain-avancado\"").is_err());
        assert_eq!(
            "eco-strain-avancado".parse::<Categoria>(),
            Err(CategoriaDesconhecida)
        );
    }
}
