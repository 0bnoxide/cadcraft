//! Label content: format parts (literal, property, number with precision and suffix, counter,
//! newline) and label-expression field derivation. Reference: AutoGIS
//! `label_expression_common.py`; Label Genie behaviour spec (content parts only). Owner: Claude (see
//! `docs/agents/slice-4-labels.md`).
//!
//! Pure functions, no drawing access. Nothing here panics: a missing property, a value that is not
//! a number, a non-finite number or a counter overflow renders that part as empty text.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Properties of one label source (a point's attributes, a segment's length and bearing, ...).
pub type Props = serde_json::Map<String, Value>;

/// Longest rendered label, in bytes; output past it is cut at a char boundary.
pub const MAX_LABEL_LEN: usize = 4096;
/// Most decimals a `Number` part renders; larger requests are clamped.
pub const MAX_DECIMALS: u8 = 12;
/// Widest zero padding a `Counter` part renders; larger requests are clamped.
pub const MAX_COUNTER_WIDTH: u8 = 20;

/// One piece of label content. Parts render in order and concatenate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Part {
    /// Literal text.
    Text { text: String },
    /// A property as text: a string as is, a number in its plain form, a bool as `true`/`false`.
    Prop { key: String },
    /// A numeric property (JSON number or numeric string), optionally rounded to an increment
    /// first, shown with `decimals` places, trailing zeros optionally trimmed, then `suffix`.
    Number {
        key: String,
        decimals: u8,
        #[serde(default)]
        round_to: Option<f64>,
        #[serde(default)]
        suffix: String,
        #[serde(default)]
        trim_zeros: bool,
    },
    /// `start + step * index`, zero-padded to `width` digits.
    Counter {
        #[serde(default)]
        start: i64,
        #[serde(default = "one")]
        step: i64,
        #[serde(default)]
        width: u8,
    },
    /// A line break, `\n`. MText callers convert it to `\P`.
    Newline,
}

fn one() -> i64 {
    1
}

/// Renders `parts` for one source. `index` is the source's 0-based position, used by counters.
pub fn format(parts: &[Part], props: &Props, index: u64) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            Part::Text { text } => out.push_str(text),
            Part::Prop { key } => match props.get(key) {
                Some(Value::String(s)) => out.push_str(s),
                Some(Value::Number(n)) => out.push_str(&n.to_string()),
                Some(Value::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
                _ => {}
            },
            Part::Number { key, decimals, round_to, suffix, trim_zeros } => {
                if let Some(text) = props.get(key).and_then(number_of).and_then(|v| format_number(v, *decimals, *round_to, *trim_zeros)) {
                    out.push_str(&text);
                    out.push_str(suffix);
                }
            }
            Part::Counter { start, step, width } => {
                let n = i64::try_from(index).ok().and_then(|i| step.checked_mul(i)).and_then(|d| start.checked_add(d));
                if let Some(n) = n {
                    out.push_str(&format!("{n:0w$}", w = usize::from((*width).min(MAX_COUNTER_WIDTH))));
                }
            }
            Part::Newline => out.push('\n'),
        }
        if out.len() >= MAX_LABEL_LEN {
            let mut cut = MAX_LABEL_LEN;
            while !out.is_char_boundary(cut) {
                cut -= 1;
            }
            out.truncate(cut);
            break;
        }
    }
    out
}

fn number_of(v: &Value) -> Option<f64> {
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }?;
    n.is_finite().then_some(n)
}

/// Formats `value` with `decimals` places (clamped to [`MAX_DECIMALS`]), after rounding to the
/// `round_to` increment. `None` if `round_to` is given but not finite and positive, or if the value
/// or the rounded value is not finite.
pub fn format_number(value: f64, decimals: u8, round_to: Option<f64>, trim_zeros: bool) -> Option<String> {
    let v = match round_to {
        Some(r) if r.is_finite() && r > 0.0 => (value / r).round() * r,
        Some(_) => return None,
        None => value,
    };
    if !v.is_finite() {
        return None;
    }
    let mut s = format!("{v:.d$}", d = usize::from(decimals.min(MAX_DECIMALS)));
    if trim_zeros && s.contains('.') {
        s.truncate(s.trim_end_matches('0').trim_end_matches('.').len());
    }
    // "-0.00" and "-0" read as a sign error on a drawing.
    if s.starts_with('-') && s.bytes().all(|b| matches!(b, b'-' | b'0' | b'.')) {
        s.remove(0);
    }
    Some(s)
}

/// Property names derived for one analyte. Port of AutoGIS `derive_label_fields`: both its label
/// expression variants must agree on these names, so the convention is kept exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelFields {
    pub layer_base: String,
    pub id_field: String,
    pub value_field: String,
    pub units_field: String,
    pub sl_field: String,
}

/// Label expression kinds named by the reference, kept for templates that select one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LabelExpressionType {
    ResultWithUnits,
    ExceedanceCallout,
    NdCallout,
    WellIdOnly,
}

/// Field names for `analyte` with an optional `prefix`: spaces and slashes become `_`, commas drop.
pub fn derive_label_fields(analyte: &str, prefix: &str) -> LabelFields {
    let safe: String = analyte.chars().filter(|c| *c != ',').map(|c| if c == ' ' || c == '/' { '_' } else { c }).collect();
    LabelFields {
        id_field: format!("{prefix}LocationID"),
        value_field: format!("{prefix}{safe}_Value"),
        units_field: format!("{prefix}{safe}_Units"),
        sl_field: format!("{prefix}{safe}_SL"),
        layer_base: safe,
    }
}
