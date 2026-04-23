//! MEL (Mule Expression Language) to Camel Simple language converter.
//!
//! MEL expressions use #[...] syntax in Mule 3/4. Most can be directly
//! mapped to Camel's Simple expression language.

/// Convert a MEL expression to Camel Simple expression.
/// Returns the Simple expression string, or None if too complex.
pub fn mel_to_simple(mel: &str) -> Option<String> {
    let trimmed = mel.trim();

    // Strip #[...] wrapper
    let inner = if trimmed.starts_with("#[") && trimmed.ends_with(']') {
        &trimmed[2..trimmed.len() - 1]
    } else {
        trimmed
    };

    let inner = inner.trim();

    // Direct payload references
    if inner == "payload" {
        return Some("${body}".into());
    }
    if inner == "message.payload" {
        return Some("${body}".into());
    }

    // payload.field -> ${body.field}
    if inner.starts_with("payload.") {
        let field = inner.strip_prefix("payload.")?;
        return Some(format!("${{body.{}}}", field));
    }

    // flowVars.name or vars.name -> ${exchangeProperty.name}
    if let Some(var) = inner.strip_prefix("flowVars.").or(inner
        .strip_prefix("flowVars['")
        .and_then(|s| s.strip_suffix("']")))
    {
        return Some(format!("${{exchangeProperty.{}}}", var));
    }
    if let Some(var) = inner.strip_prefix("vars.") {
        return Some(format!("${{exchangeProperty.{}}}", var));
    }

    // message.inboundProperties.name -> ${header.name}
    if let Some(prop) = inner.strip_prefix("message.inboundProperties.") {
        return Some(format!("${{header.{}}}", prop));
    }

    // attributes.headers.name -> ${header.name}
    if let Some(header) = inner.strip_prefix("attributes.headers.").or(inner
        .strip_prefix("attributes.headers['")
        .and_then(|s| s.strip_suffix("']")))
    {
        return Some(format!("${{header.{}}}", header));
    }

    // attributes.queryParams.name -> ${header.CamelHttpQuery}
    if let Some(param) = inner.strip_prefix("attributes.queryParams.") {
        return Some(format!("${{header.{}}}", param));
    }

    // String concatenation: "hello" ++ " " ++ payload.name
    if inner.contains("++") {
        let parts: Vec<&str> = inner.split("++").map(|s| s.trim()).collect();
        let mut simple_parts = Vec::new();
        for part in parts {
            let p = part.trim().trim_matches('"').trim_matches('\'');
            if part.trim().starts_with('"') || part.trim().starts_with('\'') {
                simple_parts.push(p.to_string());
            } else if let Some(converted) = mel_to_simple(part.trim()) {
                simple_parts.push(converted);
            } else {
                return None;
            }
        }
        return Some(simple_parts.join(""));
    }

    // Boolean comparisons: payload.type == "A"
    if inner.contains("==") || inner.contains("!=") || inner.contains(">=") || inner.contains("<=")
    {
        let simple = inner
            .replace("payload.", "${body.")
            .replace("flowVars.", "${exchangeProperty.")
            .replace("vars.", "${exchangeProperty.");
        // Close any unclosed ${
        // This is a best-effort conversion
        return Some(simple);
    }

    // now() -> ${date:now}
    if inner == "now()" || inner == "server.dateTime" {
        return Some("${date:now}".into());
    }

    // UUID -> ${exchangeId}
    if inner == "java.util.UUID.randomUUID().toString()" || inner.contains("uuid()") {
        return Some("${exchangeId}".into());
    }

    // null check: payload.field != null
    if inner.contains("!= null") {
        let field = inner.replace(" != null", "").trim().to_string();
        if let Some(simple) = mel_to_simple(&field) {
            return Some(format!("{} != null", simple));
        }
    }

    // Ternary: condition ? a : b -> not directly in Simple, skip
    if inner.contains('?') && inner.contains(':') {
        return None; // Too complex for Simple
    }

    None // Unrecognized — needs manual mapping
}

/// Check if a string contains a MEL expression.
pub fn contains_mel(s: &str) -> bool {
    s.contains("#[") && s.contains(']')
}

/// Extract the MEL expression from a string, or return the string as-is.
pub fn extract_mel(s: &str) -> &str {
    if let Some(start) = s.find("#[") {
        if let Some(end) = s[start..].rfind(']') {
            return &s[start..start + end + 1];
        }
    }
    s
}
