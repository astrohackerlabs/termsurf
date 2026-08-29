//! JSON↔tensor conversion, ported from v1 (v1/cargo/src/lib.rs
//! value_to_tensor / tensor_to_value) with serde_json::Value in place of
//! Nushell Value, and fallible tch calls (`f_*`) in place of panicking ones —
//! a daemon must return errors, not die.

use tch::{Device, Kind, Tensor};

/// tch errors stringify with a full C++ backtrace; keep the first line only —
/// good error messages are a carried-forward principle, stack frames are not.
pub fn tch_error(e: tch::TchError) -> String {
    e.to_string()
        .lines()
        .next()
        .unwrap_or("internal torch error")
        .to_string()
}

/// v1 fidelity: default dtype is float32 (v1/cargo/src/lib.rs:197), not
/// Python torch.tensor's int64 inference.
/// Display name for a Kind, covering every kind the registry can hold —
/// comparison ops mint Bool, `randn --dtype float16` mints Half. Where a
/// name overlaps `parse_kind`'s inputs it matches them; the rest are
/// display-only.
pub fn kind_name(kind: tch::Kind) -> String {
    match kind {
        tch::Kind::Float => "float32".to_string(),
        tch::Kind::Double => "float64".to_string(),
        tch::Kind::Half => "float16".to_string(),
        tch::Kind::Int => "int32".to_string(),
        tch::Kind::Int64 => "int64".to_string(),
        tch::Kind::Int16 => "int16".to_string(),
        tch::Kind::Int8 => "int8".to_string(),
        tch::Kind::Uint8 => "uint8".to_string(),
        tch::Kind::Bool => "bool".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

pub fn parse_kind(dtype: Option<&str>) -> Result<Kind, String> {
    match dtype {
        None | Some("float32") | Some("float") => Ok(Kind::Float),
        Some("float64") | Some("double") => Ok(Kind::Double),
        Some("int32") | Some("int") => Ok(Kind::Int),
        Some("int64") | Some("long") => Ok(Kind::Int64),
        Some("bool") => Ok(Kind::Bool),
        Some(s) => Err(format!(
            "invalid dtype: {s} (expected float32, float64, int32, int64, or bool)"
        )),
    }
}

/// The non-finite JSON dialect (issue 0006): JSON has no NaN/Infinity, and
/// serde maps them to null — silent corruption. These three string tokens
/// are emitted by `tensor_to_json` and accepted by `json_to_tensor`.
fn token_to_f64(token: &str) -> Option<f64> {
    match token {
        "NaN" => Some(f64::NAN),
        "Infinity" => Some(f64::INFINITY),
        "-Infinity" => Some(f64::NEG_INFINITY),
        _ => None,
    }
}

fn f64_to_json(value: f64) -> serde_json::Value {
    if value.is_finite() {
        serde_json::Value::from(value)
    } else if value.is_nan() {
        serde_json::Value::String("NaN".to_string())
    } else if value > 0.0 {
        serde_json::Value::String("Infinity".to_string())
    } else {
        serde_json::Value::String("-Infinity".to_string())
    }
}

/// Decide the tensor kind for input `data` (issue 0006). With an explicit
/// dtype, PyTorch casting rules apply and any leaf mix is fine — except
/// non-finite tokens cast to integer kinds, which PyTorch itself rejects.
/// Without one: all-bool data infers Bool; bool/number mixes error (no
/// silent cross-kind inference — carried principle 4; PyTorch would infer
/// int64 here, a recorded deviation); otherwise the float32 default.
pub fn resolve_kind(data: &serde_json::Value, explicit: Option<Kind>) -> Result<Kind, String> {
    let mut bools = 0usize;
    let mut numbers = 0usize;
    let mut tokens = 0usize;
    classify_leaves(data, &mut bools, &mut numbers, &mut tokens)?;
    if let Some(kind) = explicit {
        let is_integer = matches!(
            kind,
            Kind::Int | Kind::Int8 | Kind::Int16 | Kind::Int64 | Kind::Uint8
        );
        if is_integer && tokens > 0 {
            return Err("non-finite values cannot be cast to an integer dtype".to_string());
        }
        return Ok(kind);
    }
    if bools > 0 && (numbers > 0 || tokens > 0) {
        return Err("mixed booleans and numbers (pass an explicit --dtype to cast)".to_string());
    }
    if bools > 0 {
        Ok(Kind::Bool)
    } else {
        Ok(Kind::Float)
    }
}

fn classify_leaves(
    value: &serde_json::Value,
    bools: &mut usize,
    numbers: &mut usize,
    tokens: &mut usize,
) -> Result<(), String> {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                classify_leaves(item, bools, numbers, tokens)?;
            }
            Ok(())
        }
        serde_json::Value::Bool(_) => {
            *bools += 1;
            Ok(())
        }
        serde_json::Value::Number(_) => {
            *numbers += 1;
            Ok(())
        }
        serde_json::Value::String(s) if token_to_f64(s).is_some() => {
            *tokens += 1;
            Ok(())
        }
        other => Err(format!(
            "input must be numbers, booleans, or the non-finite tokens \"NaN\"/\"Infinity\"/\"-Infinity\", got {other}"
        )),
    }
}

/// One scalar leaf as f64: numbers, booleans (0/1), or a non-finite token.
fn leaf_to_f64(value: &serde_json::Value) -> Result<f64, String> {
    if let Some(f) = value.as_f64() {
        return Ok(f);
    }
    match value {
        serde_json::Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        serde_json::Value::String(s) => token_to_f64(s)
            .ok_or_else(|| format!("expected a number or a non-finite token, got \"{s}\"")),
        other => Err(format!("expected number, got {other}")),
    }
}

/// Recursively convert a JSON number / (nested) array of numbers to a tensor.
pub fn json_to_tensor(
    value: &serde_json::Value,
    kind: Kind,
    device: Device,
) -> Result<Tensor, String> {
    match value {
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return Err("list cannot be empty".to_string());
            }
            if items[0].is_array() {
                // Nested list: convert each sublist and stack along dim 0.
                // f_stack (not stack) so ragged shapes return an error.
                let subtensors: Result<Vec<Tensor>, String> = items
                    .iter()
                    .map(|v| json_to_tensor(v, kind, device))
                    .collect();
                let subtensors = subtensors?;
                Tensor::f_stack(&subtensors, 0)
                    .map_err(|e| format!("ragged or mismatched nested list: {}", tch_error(e)))
            } else {
                // Flat list. v1 fidelity: all-integers build an i64 buffer,
                // anything else builds f64 (booleans as 0/1, the non-finite
                // tokens as their floats); both then cast to `kind`.
                let all_ints = items.iter().all(|v| v.is_i64() || v.is_u64());
                let tensor = if all_ints {
                    let data: Result<Vec<i64>, String> = items
                        .iter()
                        .map(|v| {
                            v.as_i64()
                                .ok_or_else(|| format!("expected integer, got {v}"))
                        })
                        .collect();
                    Tensor::from_slice(&data?)
                } else {
                    let data: Result<Vec<f64>, String> = items.iter().map(leaf_to_f64).collect();
                    Tensor::from_slice(&data?)
                };
                tensor
                    .f_to_kind(kind)
                    .map_err(tch_error)?
                    .f_to_device(device)
                    .map_err(tch_error)
            }
        }
        serde_json::Value::Number(_)
        | serde_json::Value::Bool(_)
        | serde_json::Value::String(_) => {
            let tensor = if let Some(i) = value.as_i64() {
                Tensor::from(i)
            } else {
                Tensor::from(leaf_to_f64(value)?)
            };
            tensor
                .f_to_kind(kind)
                .map_err(tch_error)?
                .f_to_device(device)
                .map_err(tch_error)
        }
        other => Err(format!(
            "input must be a number, boolean, or a (nested) list of them, got {other}"
        )),
    }
}

/// Recursively convert a tensor to JSON (0-D → number, N-D → nested arrays).
/// The caller is expected to hand in a CPU tensor; `value` requests copy the
/// tensor off-device first.
pub fn tensor_to_json(tensor: &Tensor) -> Result<serde_json::Value, String> {
    let dims = tensor.size();
    let kind = tensor.kind();

    if dims.is_empty() {
        return match kind {
            Kind::Int | Kind::Int8 | Kind::Int16 | Kind::Int64 | Kind::Uint8 => {
                Ok(serde_json::Value::from(tensor.int64_value(&[])))
            }
            Kind::Float | Kind::Double | Kind::Half => Ok(f64_to_json(tensor.double_value(&[]))),
            // Comparison ops return Bool tensors (issue 0005).
            Kind::Bool => Ok(serde_json::Value::Bool(tensor.int64_value(&[]) != 0)),
            _ => Err(format!("cannot convert tensor of type {kind:?}")),
        };
    }

    let first_dim = dims[0];
    let mut items: Vec<serde_json::Value> = Vec::with_capacity(first_dim as usize);
    for i in 0..first_dim {
        let element = tensor.f_get(i).map_err(tch_error)?;
        items.push(tensor_to_json(&element)?);
    }
    Ok(serde_json::Value::Array(items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flat_int_list_round_trips_as_float32_by_default() {
        let kind = parse_kind(None).unwrap();
        let tensor = json_to_tensor(&json!([1, 2, 3]), kind, Device::Cpu).unwrap();
        assert_eq!(tensor.kind(), Kind::Float);
        assert_eq!(tensor_to_json(&tensor).unwrap(), json!([1.0, 2.0, 3.0]));
    }

    #[test]
    fn nested_list_round_trips() {
        let tensor = json_to_tensor(&json!([[1, 2], [3, 4]]), Kind::Float, Device::Cpu).unwrap();
        assert_eq!(tensor.size(), vec![2, 2]);
        assert_eq!(
            tensor_to_json(&tensor).unwrap(),
            json!([[1.0, 2.0], [3.0, 4.0]])
        );
    }

    #[test]
    fn scalar_round_trips() {
        let tensor = json_to_tensor(&json!(7), Kind::Float, Device::Cpu).unwrap();
        assert_eq!(tensor.size(), Vec::<i64>::new());
        assert_eq!(tensor_to_json(&tensor).unwrap(), json!(7.0));
    }

    #[test]
    fn int64_dtype_round_trips_as_integers() {
        let kind = parse_kind(Some("int64")).unwrap();
        let tensor = json_to_tensor(&json!([1, 2, 3]), kind, Device::Cpu).unwrap();
        assert_eq!(tensor.kind(), Kind::Int64);
        assert_eq!(tensor_to_json(&tensor).unwrap(), json!([1, 2, 3]));
    }

    #[test]
    fn ragged_nested_list_is_an_error_not_a_panic() {
        let result = json_to_tensor(&json!([[1, 2], [3]]), Kind::Float, Device::Cpu);
        assert!(result.is_err());
    }

    #[test]
    fn empty_list_is_an_error() {
        assert!(json_to_tensor(&json!([]), Kind::Float, Device::Cpu).is_err());
    }

    #[test]
    fn non_numeric_input_is_an_error() {
        assert!(json_to_tensor(&json!("hello"), Kind::Float, Device::Cpu).is_err());
        assert!(json_to_tensor(&json!([1, "two"]), Kind::Float, Device::Cpu).is_err());
    }

    #[test]
    fn invalid_dtype_is_an_error() {
        assert!(parse_kind(Some("float16")).is_err());
    }
}
