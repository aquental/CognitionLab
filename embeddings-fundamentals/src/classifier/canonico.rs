//! Python-compatible canonical JSON and the state digest (spec/CLASSIFIER.md §7.4).
//!
//! Equivalent to `json.dumps(x, sort_keys=True, separators=(",", ":"), ensure_ascii=False)`.
//! Keys are sorted explicitly, so the output does not depend on `serde_json`'s
//! `preserve_order` feature. Strings are not normalized: unlike [`crate::ids`], which
//! applies NFC, Python's digest hashes the strings as they are.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// A value the canonical encoder refuses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CanonicoError {
    /// A number that is not an integer. Python would print its `repr`; the state never
    /// has floats and no vector checks that format, so it is refused rather than guessed.
    #[error("canonical JSON only encodes integers")]
    NaoInteiro,
}

/// The canonical JSON of `valor`.
///
/// # Errors
///
/// [`CanonicoError::NaoInteiro`] if `valor` contains a non-integer number.
pub fn json_canonico(valor: &Value) -> Result<String, CanonicoError> {
    let mut saida = String::new();
    escrever(valor, &mut saida)?;
    Ok(saida)
}

/// sha256 of the UTF-8 bytes of the canonical JSON of `valor`.
///
/// # Errors
///
/// [`CanonicoError::NaoInteiro`] if `valor` contains a non-integer number.
pub fn digest(valor: &Value) -> Result<[u8; 32], CanonicoError> {
    Ok(Sha256::digest(json_canonico(valor)?.as_bytes()).into())
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

fn escrever(valor: &Value, saida: &mut String) -> Result<(), CanonicoError> {
    match valor {
        Value::Null => saida.push_str("null"),
        Value::Bool(b) => saida.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                saida.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                saida.push_str(&u.to_string());
            } else {
                return Err(CanonicoError::NaoInteiro);
            }
        }
        Value::String(s) => escrever_string(s, saida),
        Value::Array(itens) => {
            saida.push('[');
            for (i, item) in itens.iter().enumerate() {
                if i > 0 {
                    saida.push(',');
                }
                escrever(item, saida)?;
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
                escrever(&mapa[chave], saida)?;
            }
            saida.push('}');
        }
    }
    Ok(())
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
            json_canonico(&v).unwrap(),
            "{\"t\":\"\\u0001\\b\\f\\u001f\u{7f}\u{2028}\u{2029}/é\\\"\\\\\"}"
        );
    }

    #[test]
    fn strings_are_not_normalized() {
        // `ids::canonical_json` would turn this into one composed code point.
        let decomposed = json!("e\u{301}");
        assert_eq!(json_canonico(&decomposed).unwrap(), "\"e\u{301}\"");
        assert_ne!(
            digest(&decomposed).unwrap(),
            digest(&json!("\u{e9}")).unwrap()
        );
    }

    #[test]
    fn keys_sort_by_code_point() {
        let v = json!({"é": 1, "z": 2, "Z": 3, "a": 4, "\u{1f600}": 5, "\u{ffff}": 6});
        assert_eq!(
            json_canonico(&v).unwrap(),
            "{\"Z\":3,\"a\":4,\"z\":2,\"é\":1,\"\u{ffff}\":6,\"\u{1f600}\":5}"
        );
    }

    #[test]
    fn floats_are_refused() {
        assert_eq!(
            json_canonico(&json!({"a": [1, 0.5]})),
            Err(CanonicoError::NaoInteiro)
        );
        assert_eq!(
            json_canonico(&json!([-3, 18_446_744_073_709_551_615_u64])).unwrap(),
            "[-3,18446744073709551615]"
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
            prop_assert_eq!(json_canonico(&mapa(&a)).unwrap(), json_canonico(&mapa(&b)).unwrap());
        }
    }
}
