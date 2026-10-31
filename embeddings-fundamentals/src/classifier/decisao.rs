//! Validation of the provider's answer and the decision (spec/CLASSIFIER.md §7.1–§7.3;
//! TestoDoc `rlcd_client.validar_respostas` and `decidir_classificacao`).

use serde_json::Value;

use super::categorias::{
    CRITERIO_ID, Categoria, TOLERANCIA_LIMIAR, TOLERANCIA_SOMA_PROBABILIDADES,
};
use super::pyclasses;
use super::texto::strip;
use super::tipos::{EstadoClassificacao, RespostaEscolha};

/// Python's `sum()` of floats since 3.12: Neumaier's compensated sum, started from the
/// first value (CPython `builtin_sum_impl`).
fn soma_python(valores: &[f64]) -> f64 {
    let Some((&primeiro, resto)) = valores.split_first() else {
        return 0.0;
    };
    let (mut soma, mut compensacao) = (primeiro, 0.0);
    for &x in resto {
        let t = soma + x;
        if soma.abs() >= x.abs() {
            compensacao += (soma - t) + x;
        } else {
            compensacao += (x - t) + soma;
        }
        soma = t;
    }
    if compensacao != 0.0 && compensacao.is_finite() {
        soma += compensacao;
    }
    soma
}

/// The decimal value of a Unicode decimal digit (`str.isdecimal()`): every run of decimal
/// digits starts at a zero, so the value is the offset in its range modulo 10.
fn valor_decimal(ch: char) -> Option<u32> {
    let c = u32::from(ch);
    let i = pyclasses::IS_DECIMAL.partition_point(|&(_, fim)| fim < c);
    let &(inicio, _) = pyclasses::IS_DECIMAL.get(i).filter(|&&(a, _)| a <= c)?;
    Some((c - inicio) % 10)
}

/// Python's `float(s)` of a string: surrounding whitespace dropped, Unicode decimal digits
/// read as ASCII ones, an underscore only between two digits; then the usual decimal,
/// exponent, `inf` and `nan` forms.
fn float_de_texto(s: &str) -> Option<f64> {
    let mut ascii = String::with_capacity(s.len());
    let chars: Vec<char> = strip(s).chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' {
            let digito = |j: Option<usize>| {
                j.and_then(|j| chars.get(j))
                    .is_some_and(|&c| c.is_ascii_digit() || valor_decimal(c).is_some())
            };
            if !(digito(i.checked_sub(1)) && digito(Some(i + 1))) {
                return None;
            }
        } else if ch.is_ascii() {
            ascii.push(ch);
        } else {
            let valor = valor_decimal(ch)?;
            ascii.push(char::from_digit(valor, 10)?);
        }
    }
    ascii.parse().ok()
}

/// TestoDoc's `_finito`: `float(valor)` if it is finite. A JSON number, a numeric string
/// or a boolean (1.0 / 0.0); anything else is refused.
fn finito(valor: &Value) -> Option<f64> {
    let f = match valor {
        Value::Number(n) => n.as_f64()?,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::String(s) => float_de_texto(s)?,
        Value::Null | Value::Array(_) | Value::Object(_) => return None,
    };
    f.is_finite().then_some(f)
}

/// `validar_respostas` for the classifier's single `Choice` criterion, on TestoDoc's
/// neutral format: `{"tipo_documento": {"escolha": …, "probabilidades": …}}` or
/// `{"tipo_documento": "<escolha>"}`. `None` means an invalid answer
/// (`ErroCategoria::RespostaInvalida`).
#[must_use]
pub fn validar_respostas(brutas: &Value) -> Option<RespostaEscolha> {
    let brutas = brutas.as_object()?;
    if brutas.len() != 1 {
        return None;
    }
    let bruto = brutas.get(CRITERIO_ID)?;
    let (escolha, probabilidades) = match bruto {
        Value::Object(o) => (o.get("escolha"), o.get("probabilidades")),
        outro => (Some(outro), None),
    };
    let escolha: Categoria = escolha?.as_str()?.parse().ok()?;
    let mut limpas = [0.0; 9];
    match probabilidades {
        // Missing or null: one-hot on the choice (`rlcd_client.py:224–225`).
        None | Some(Value::Null) => limpas[escolha.indice()] = 1.0,
        Some(Value::Object(mapa)) => {
            if mapa.keys().any(|k| k.parse::<Categoria>().is_err()) {
                return None;
            }
            for categoria in Categoria::TODAS {
                let p = match mapa.get(categoria.valor()) {
                    Some(valor) => finito(valor)?,
                    None => 0.0,
                };
                if !(0.0..=1.0).contains(&p) {
                    return None;
                }
                limpas[categoria.indice()] = p;
            }
        }
        Some(_) => return None,
    }
    // Summed in the order of the options, as TestoDoc does, so the rounding is the same.
    if (soma_python(&limpas) - 1.0).abs() > TOLERANCIA_SOMA_PROBABILIDADES {
        return None;
    }
    Some(RespostaEscolha {
        escolha,
        probabilidades: limpas,
    })
}

/// `decidir_classificacao`: (state, confidence, margin). Only the threshold decides
/// `incerto`; a confidence equal to the threshold is `ok`. The confidence is the
/// probability of the choice, which may not be the highest; the margin is the highest
/// minus the second highest, never below 0.
#[must_use]
pub fn decidir_classificacao(
    escolha: Categoria,
    probabilidades: &[f64; 9],
    limiar: f64,
) -> (EstadoClassificacao, f64, f64) {
    let confianca = probabilidades[escolha.indice()];
    let mut ordenadas = *probabilidades;
    ordenadas.sort_by(|a, b| b.total_cmp(a));
    let diferenca = ordenadas[0] - ordenadas[1];
    // Python's `max(0.0, x)` keeps the first argument unless `x` is greater.
    let margem = if diferenca > 0.0 { diferenca } else { 0.0 };
    let estado = if confianca + TOLERANCIA_LIMIAR >= limiar {
        EstadoClassificacao::Ok
    } else {
        EstadoClassificacao::Incerto
    };
    (estado, confianca, margem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sum_is_pythons_compensated_sum() {
        // Python 3.14: sum([0.1] * 10) == 1.0, and sum([1e100, 1.0, -1e100, 1.0]) == 2.0.
        assert_eq!(soma_python(&[0.1; 10]).to_bits(), 1.0_f64.to_bits());
        assert_eq!(
            soma_python(&[1e100, 1.0, -1e100, 1.0]).to_bits(),
            2.0_f64.to_bits()
        );
        assert_eq!(soma_python(&[]).to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn finite_follows_python_float() {
        // Python 3.14 float(): these parse ...
        for (valor, esperado) in [
            (json!(0.5), 0.5_f64),
            (json!(1), 1.0),
            (json!(true), 1.0),
            (json!(false), 0.0),
            (json!(" 0.25\n"), 0.25),
            (json!("1_0e-1"), 1.0),
            (json!("٠.٥"), 0.5),
            (json!("+.5"), 0.5),
            (json!("1."), 1.0),
        ] {
            assert_eq!(
                finito(&valor).map(f64::to_bits),
                Some(esperado.to_bits()),
                "{valor}"
            );
        }
        // ... and these do not, or are not finite.
        for valor in [
            json!(null),
            json!([0.5]),
            json!({}),
            json!("1__0"),
            json!("_1"),
            json!("1_"),
            json!("abc"),
            json!("inf"),
            json!("-Infinity"),
            json!("nan"),
            json!("0x1p-1"),
            json!(""),
        ] {
            assert_eq!(finito(&valor), None, "{valor}");
        }
    }

    #[test]
    fn margin_is_never_negative_and_ties_are_ok() {
        let mut p = [0.0; 9];
        p[Categoria::Holter.indice()] = 0.6;
        p[Categoria::Mapa.indice()] = 0.4;
        let (estado, confianca, margem) = decidir_classificacao(Categoria::Mapa, &p, 0.4);
        assert_eq!(
            estado,
            EstadoClassificacao::Ok,
            "equal to the threshold is ok"
        );
        assert_eq!(confianca.to_bits(), 0.4_f64.to_bits());
        assert!((margem - 0.2).abs() < 1e-12);
        let (_, _, empate) = decidir_classificacao(
            Categoria::Holter,
            &[0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            0.9,
        );
        assert_eq!(empate.to_bits(), 0.0_f64.to_bits());
    }
}
