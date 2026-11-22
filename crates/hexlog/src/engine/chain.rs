use std::fmt::Write;

use serde_json::{Number, Value};
use sha2::{Digest, Sha256};

use crate::engine::limits::{JSON_NESTING_MAX, json_nesting_exceeds};
use crate::engine::model::{AppendItem, ItemBody};
use crate::error::{ErrorCode, HexlogError};
use crate::ids::{Hash, TenantId};

/// 2^53: o vetor oficial `4340000000000000` do RFC 8785 está exatamente no limite.
const EXACT_INTEGER_MAX: u64 = 1 << 53;
/// Último `n` do `Number::toString` do ECMA-262 que ainda sai em notação posicional.
const POSITIONAL_MAX: usize = 21;
/// Teto de pilha da recursão do `canonical`, e não regra de produto.
const CANONICAL_DEPTH_MAX: usize = 256;

/// JCS (RFC 8785) de `value`. Inteiro além de ±2^53 é erro: converter para double arredondaria em silêncio.
pub fn canonical(value: &Value) -> Result<String, HexlogError> {
    let mut out = String::new();
    write_value(value, 0, &mut out)?;
    Ok(out)
}

pub fn sha256hex(text: &str) -> Hash {
    hex_digest(&[text])
}

pub fn hash_record(prev_hash: &str, body: &str) -> Hash {
    hex_digest(&[prev_hash, body])
}

pub fn anchor_doc(tenant: &TenantId, created_at: &str) -> String {
    let mut out = String::from("{\"created_at\":");
    write_quoted(created_at, &mut out);
    out.push_str(",\"format\":\"hexlog-chain/1\",\"tenant\":");
    write_quoted(tenant.as_str(), &mut out);
    out.push('}');
    out
}

pub fn anchor(anchor_doc: &str) -> Hash {
    sha256hex(anchor_doc)
}

pub fn fingerprint(items: &[AppendItem]) -> Result<Hash, HexlogError> {
    for (index, item) in items.iter().enumerate() {
        if let ItemBody::Node { data, .. } = &item.body
            && json_nesting_exceeds(data, JSON_NESTING_MAX)
        {
            return Err(HexlogError::refused(
                ErrorCode::InvalidInput,
                "max-depth-exceeded",
                "value nested too deeply",
            )
            .at(&format!("/{index}/body/data")));
        }
    }
    let value = serde_json::to_value(items)
        .map_err(|_| HexlogError::internal("append items are not serializable"))?;
    Ok(sha256hex(&canonical(&value)?))
}

fn hex_digest(parts: &[&str]) -> Hash {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
    }
    let mut hex = String::with_capacity(64);
    for byte in hasher.finalize().iter() {
        // Escrever em `String` não falha.
        let _ = write!(hex, "{byte:02x}");
    }
    Hash::from_lower_hex(hex)
}

fn write_value(value: &Value, depth: usize, out: &mut String) -> Result<(), HexlogError> {
    if matches!(value, Value::Array(_) | Value::Object(_)) && depth >= CANONICAL_DEPTH_MAX {
        return Err(HexlogError::refused(
            ErrorCode::InvalidInput,
            "max-depth-exceeded",
            "value nested too deeply",
        ));
    }
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => write_number(number, out)?,
        Value::String(text) => write_quoted(text, out),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, depth + 1, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            // A ordem do `Map` é por bytes UTF-8; o RFC exige unidades UTF-16.
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_unstable_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            out.push('{');
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_quoted(key, out);
                out.push(':');
                write_value(item, depth + 1, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_quoted(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            // Hexadecimal minúsculo, como o `UnicodeEscape` do ECMA-262.
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_number(number: &Number, out: &mut String) -> Result<(), HexlogError> {
    let magnitude = number
        .as_i64()
        .map(i64::unsigned_abs)
        .or_else(|| number.as_u64());
    if let Some(magnitude) = magnitude {
        if magnitude > EXACT_INTEGER_MAX {
            return Err(HexlogError::refused(
                ErrorCode::InvalidInput,
                "number-out-of-range",
                "integer outside the exactly representable range",
            ));
        }
        let _ = write!(out, "{number}");
        return Ok(());
    }
    // Sem `arbitrary_precision`, `as_f64` de um `Number` que não é inteiro sempre existe e é finito.
    let float = number
        .as_f64()
        .ok_or_else(|| HexlogError::internal("number is not a double"))?;
    write_float(float, out)
}

/// O `{:e}` puro arredonda o empate para cima; o modo exato com `k` dígitos desempata ao par.
fn shortest_even(x: f64) -> String {
    let shortest = format!("{x:e}");
    let digits = shortest.split_once('e').map_or(0, |(mantissa, _)| {
        mantissa.chars().filter(|c| *c != '.').count()
    });
    if digits == 0 {
        return shortest;
    }
    let exact = format!("{x:.*e}", digits - 1);
    if exact != shortest && exact.parse::<f64>() == Ok(x) {
        exact
    } else {
        shortest
    }
}

fn write_float(x: f64, out: &mut String) -> Result<(), HexlogError> {
    if x == 0.0 {
        out.push('0');
        return Ok(());
    }
    if x < 0.0 {
        out.push('-');
    }
    let scientific = shortest_even(x.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .and_then(|(mantissa, exponent)| Some((mantissa, exponent.parse::<i32>().ok()?)))
        .ok_or_else(|| HexlogError::internal("malformed float formatting"))?;
    let digits = mantissa.replace('.', "");
    let count = digits.len();
    // `n` do ECMA-262: o valor é 0.digits × 10^n.
    let n = exponent + 1;
    match usize::try_from(n)
        .ok()
        .filter(|n| (1..=POSITIONAL_MAX).contains(n))
    {
        Some(n) if count <= n => {
            out.push_str(&digits);
            out.extend(std::iter::repeat_n('0', n - count));
        }
        Some(n) => {
            out.push_str(&digits[..n]);
            out.push('.');
            out.push_str(&digits[n..]);
        }
        None if (-5..=0).contains(&n) => {
            out.push_str("0.");
            out.extend(std::iter::repeat_n('0', n.unsigned_abs() as usize));
            out.push_str(&digits);
        }
        None => {
            out.push_str(&digits[..1]);
            if count > 1 {
                out.push('.');
                out.push_str(&digits[1..]);
            }
            let sign = if exponent < 0 { '-' } else { '+' };
            let _ = write!(out, "e{sign}{}", exponent.unsigned_abs());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
