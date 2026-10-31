//! Python-compatible canonical JSON and the state digest (spec/CLASSIFIER.md §7.4).
//!
//! Equivalent to `json.dumps(x, sort_keys=True, separators=(",", ":"), ensure_ascii=False)`.
//! Keys are sorted explicitly, so the output does not depend on `serde_json`'s
//! `preserve_order` feature. Strings are not normalized: unlike [`crate::ids`], which
//! applies NFC, Python's digest hashes the strings as they are.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The canonical JSON of `valor`.
#[must_use]
pub fn json_canonico(valor: &Value) -> String {
    let mut saida = String::new();
    escrever(valor, &mut saida);
    saida
}

/// sha256 of the UTF-8 bytes of the canonical JSON of `valor`.
#[must_use]
pub fn digest(valor: &Value) -> [u8; 32] {
    Sha256::digest(json_canonico(valor).as_bytes()).into()
}

/// Python's `repr` of a finite float, which `json.dumps` writes: the shortest digits
/// that round-trip; scientific notation, with a sign and at least two exponent digits,
/// when the decimal exponent is below -4 or above 16 (`format_float_short`, mode `r`);
/// otherwise positional, with `.0` when there is no fractional part.
fn repr_float(x: f64) -> String {
    // Rust's `{:e}` prints the shortest round-trip digits ("1.2345e-7"), but when two
    // such strings are equally near it may take the other one; Python's dtoa takes the
    // even one. Formatting with that many digits rounds the exact value, ties to even.
    let curto = format!("{:e}", x.abs());
    let n_digitos = curto
        .split('e')
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_digit)
        .count();
    let cientifico = format!("{:.*e}", n_digitos.saturating_sub(1), x.abs());
    let (mantissa, expoente) = cientifico.split_once('e').unwrap_or((&cientifico, "0"));
    let digitos: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let expoente: i64 = expoente.parse().unwrap_or_default();
    // Python's `decpt`: the value is 0.<digitos> × 10^decpt.
    let decpt = expoente + 1;
    let n = i64::try_from(digitos.len()).unwrap_or(i64::MAX);
    let sinal = if x.is_sign_negative() { "-" } else { "" };
    if decpt <= -4 || decpt > 16 {
        let (primeiro, resto) = digitos.split_at(1.min(digitos.len()));
        let ponto = if resto.is_empty() {
            String::new()
        } else {
            format!(".{resto}")
        };
        let e = decpt - 1;
        let sinal_e = if e < 0 { '-' } else { '+' };
        format!("{sinal}{primeiro}{ponto}e{sinal_e}{:02}", e.unsigned_abs())
    } else if decpt <= 0 {
        let zeros = "0".repeat(usize::try_from(-decpt).unwrap_or_default());
        format!("{sinal}0.{zeros}{digitos}")
    } else if decpt >= n {
        let zeros = "0".repeat(usize::try_from(decpt - n).unwrap_or_default());
        format!("{sinal}{digitos}{zeros}.0")
    } else {
        let (inteira, fracao) = digitos.split_at(usize::try_from(decpt).unwrap_or_default());
        format!("{sinal}{inteira}.{fracao}")
    }
}

fn escrever_string(s: &str, saida: &mut String) {
    // serde_json's string escapes are Python's with ensure_ascii=False: \" \\ \n \r \t
    // \b \f, \u00xx in lowercase hex for the other controls, everything else literal
    // (including U+007F, U+2028 and U+2029).
    saida.push('"');
    for ch in s.chars() {
        match ch {
            '"' => saida.push_str("\\\""),
            '\\' => saida.push_str("\\\\"),
            '\n' => saida.push_str("\\n"),
            '\r' => saida.push_str("\\r"),
            '\t' => saida.push_str("\\t"),
            '\u{8}' => saida.push_str("\\b"),
            '\u{c}' => saida.push_str("\\f"),
            c if u32::from(c) < 0x20 => {
                saida.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            c => saida.push(c),
        }
    }
    saida.push('"');
}

fn escrever(valor: &Value, saida: &mut String) {
    match valor {
        Value::Null => saida.push_str("null"),
        Value::Bool(b) => saida.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                saida.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                saida.push_str(&u.to_string());
            } else {
                // Without `arbitrary_precision`, every other number is a finite f64.
                saida.push_str(&n.as_f64().map_or_else(|| n.to_string(), repr_float));
            }
        }
        Value::String(s) => escrever_string(s, saida),
        Value::Array(itens) => {
            saida.push('[');
            for (i, item) in itens.iter().enumerate() {
                if i > 0 {
                    saida.push(',');
                }
                escrever(item, saida);
            }
            saida.push(']');
        }
        Value::Object(mapa) => {
            // `str`'s order is the order of UTF-8 bytes, which is code-point order:
            // Python's `sort_keys`.
            let mut chaves: Vec<&String> = mapa.keys().collect();
            chaves.sort_unstable();
            saida.push('{');
            for (i, chave) in chaves.into_iter().enumerate() {
                if i > 0 {
                    saida.push(',');
                }
                escrever_string(chave, saida);
                saida.push(':');
                escrever(&mapa[chave], saida);
            }
            saida.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    #[test]
    fn escapes_like_python_with_ensure_ascii_false() {
        let v = json!({"t": "\u{1}\u{8}\u{c}\u{1f}\u{7f}\u{2028}\u{2029}/é\"\\"});
        assert_eq!(
            json_canonico(&v),
            "{\"t\":\"\\u0001\\b\\f\\u001f\u{7f}\u{2028}\u{2029}/é\\\"\\\\\"}"
        );
    }

    #[test]
    fn strings_are_not_normalized() {
        // `ids::canonical_json` would turn this into one composed code point.
        let decomposed = json!("e\u{301}");
        assert_eq!(json_canonico(&decomposed), "\"e\u{301}\"");
        assert_ne!(digest(&decomposed), digest(&json!("\u{e9}")));
    }

    #[test]
    fn keys_sort_by_code_point() {
        let v = json!({"é": 1, "z": 2, "Z": 3, "a": 4, "\u{1f600}": 5, "\u{ffff}": 6});
        assert_eq!(
            json_canonico(&v),
            "{\"Z\":3,\"a\":4,\"z\":2,\"é\":1,\"\u{ffff}\":6,\"\u{1f600}\":5}"
        );
    }

    #[test]
    fn floats_are_written_like_python_repr() {
        // json.dumps on Python 3.14 gives exactly these.
        assert_eq!(
            json_canonico(
                &json!({"a": [1, 0.5, 1.0, -0.0, 1e-9, 1e16, 1e15, 1e-5, 1e-4, 5e-324, 1e22, 123_456_789_012_345.6]})
            ),
            "{\"a\":[1,0.5,1.0,-0.0,1e-09,1e+16,1000000000000000.0,1e-05,0.0001,5e-324,1e+22,123456789012345.6]}"
        );
        assert_eq!(
            json_canonico(&json!([-3, 18_446_744_073_709_551_615_u64])),
            "[-3,18446744073709551615]"
        );
        assert_eq!(
            hex::encode(digest(&json!(0.5))),
            hex::encode(sha2::Sha256::digest(b"0.5"))
        );
    }

    fn mapa(pares: &[(String, i64)]) -> Value {
        Value::Object(pares.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
    }

    proptest! {
        // The dev-dependency enables `preserve_order`, so these maps keep insertion order.
        #[test]
        fn output_does_not_depend_on_insertion_order(
            unicos in proptest::collection::btree_map("\\PC{0,4}", any::<i64>(), 0..8)
        ) {
            let a: Vec<(String, i64)> = unicos.into_iter().collect();
            let b: Vec<(String, i64)> = a.iter().rev().cloned().collect();
            prop_assert_eq!(json_canonico(&mapa(&a)), json_canonico(&mapa(&b)));
        }
    }
}
