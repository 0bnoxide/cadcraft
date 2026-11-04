//! Redaction of sensitive values. Reference: AIGIS3D `redaction.py`. Owner: Codex (see
//! `docs/agents/slice-3-droneqa.md`).

use serde_json::Value;

fn canonical(key: &str) -> String {
    key.chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}

/// Return a scrubbed copy; never mutate the source evidence.
pub fn redact(value: &Value, sensitive_fields: &[String]) -> Value {
    redact_at(value, sensitive_fields, 0)
}

fn redact_at(value: &Value, sensitive_fields: &[String], depth: usize) -> Value {
    if depth >= 64 {
        return Value::String("[REDACTED]".into());
    }
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| {
                    let normalized = canonical(key);
                    let private = [
                        "token",
                        "accesstoken",
                        "refreshtoken",
                        "authorization",
                        "xesriauthorization",
                        "password",
                        "clientsecret",
                        "codeverifier",
                        "cookie",
                        "setcookie",
                        "apikey",
                        "signature",
                        "sig",
                    ]
                    .contains(&normalized.as_str())
                        || sensitive_fields.iter().any(|field| canonical(field) == normalized);
                    (key.clone(), if private { Value::String("[REDACTED]".into()) } else { redact_at(value, sensitive_fields, depth + 1) })
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(|item| redact_at(item, sensitive_fields, depth + 1)).collect()),
        Value::String(text) => Value::String(scrub(text)),
        _ => value.clone(),
    }
}

fn scrub(text: &str) -> String {
    let mut output = text.to_owned();
    // Static patterns still fail closed if a future edit makes one invalid.
    for (pattern, replacement) in [
        (
            r#"["'](?i-u:access[_-]?token|refresh[_-]?token|authorization|x-esri-authorization|password|client[_-]?secret)["']\s*:\s*(?:"[^"]*"|'[^']*')"#,
            "[REDACTED]",
        ),
        (r#"(?i-u:https?)://[^\s"'<>]*[?#@][^\s"'<>]*"#, "[REDACTED_URL]"),
        (r"\b(?:(?i-u:authorization)\s*[:=]\s*)?(?i-u:bearer|basic)\s+[^\s,;]+", "[REDACTED_AUTH]"),
        (
            r"\b(?i-u:(?:access[_-]?|refresh[_-]?)?token|password|client[_-]?secret|code[_-]?verifier|api[_-]?key|sig(?:nature)?)\s*[:=]\s*[^\s,;]+",
            "[REDACTED]",
        ),
    ] {
        let Ok(regex) = regex::Regex::new(pattern) else {
            return "[REDACTED]".into();
        };
        output = regex.replace_all(&output, replacement).into_owned();
    }
    output
}
