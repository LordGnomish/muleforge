//! MEL (Mule Expression Language) to Camel Simple language converter.
//!
//! MEL expressions use #[...] syntax in Mule 3/4. Most can be directly
//! mapped to Camel's Simple expression language.

/// Convert a MEL expression to Camel Simple expression.
/// Returns the Simple expression string, or None if too complex.
///
/// This is the lenient conversion used for text that is only displayed
/// (logger messages): `payload.field` becomes `${body.field}` without
/// knowing whether the body is an object with that getter. The mapper uses
/// [`convert_value`] and [`convert_condition`] where the result decides
/// what a route does.
pub fn mel_to_simple(mel: &str) -> Option<String> {
    let inner = strip_wrapper(mel);

    // Comparison: payload.type == "A"  ->  ${body.type} == 'A'
    //
    // Simple wants `${function} OP value`: the left side has to be a
    // function, the right side a function or a literal. Each side is
    // converted on its own, so every `${` is closed, and a side that is not
    // converted makes the whole comparison unconverted.
    if let Some((left, op, right)) = split_comparison(inner) {
        let left = reference_to_simple(left.trim())?;
        let right = right.trim();
        let right = literal_to_simple(right).or_else(|| reference_to_simple(right))?;
        return Some(format!("{left} {op} {right}"));
    }

    if let Some(simple) = reference_to_simple(inner) {
        return Some(simple);
    }

    // String concatenation: "hello" ++ " " ++ payload.name
    if inner.contains("++") {
        let parts: Vec<&str> = inner.split("++").map(|s| s.trim()).collect();
        let mut simple_parts = Vec::new();
        for part in parts {
            let p = part.trim().trim_matches('"').trim_matches('\'');
            if part.trim().starts_with('"') || part.trim().starts_with('\'') {
                simple_parts.push(p.to_string());
            } else {
                simple_parts.push(mel_to_simple(part.trim())?);
            }
        }
        return Some(simple_parts.join(""));
    }

    // now() -> ${date:now}
    if inner == "now()" || inner == "server.dateTime" {
        return Some("${date:now}".into());
    }

    // UUID -> ${exchangeId}
    if inner == "java.util.UUID.randomUUID().toString()" || inner.contains("uuid()") {
        return Some("${exchangeId}".into());
    }

    None // Unrecognized (ternaries included) — needs manual mapping
}

/// The expression inside `#[...]`, trimmed; `mel` itself (trimmed) when it
/// has no wrapper.
fn strip_wrapper(mel: &str) -> &str {
    let trimmed = mel.trim();
    trimmed
        .strip_prefix("#[")
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(trimmed)
        .trim()
}

/// A reference to the payload, a variable or an attribute as a Simple
/// function (`${...}`). `None` for anything else.
fn reference_to_simple(inner: &str) -> Option<String> {
    // Direct payload references
    if inner == "payload" || inner == "message.payload" {
        return Some("${body}".into());
    }

    // payload.field -> ${body.field}
    if let Some(field) = inner.strip_prefix("payload.") {
        return is_reference_path(field).then(|| format!("${{body.{}}}", field));
    }

    // flowVars.name or vars.name -> ${exchangeProperty.name}
    if let Some(var) = inner.strip_prefix("flowVars.").or(inner
        .strip_prefix("flowVars['")
        .and_then(|s| s.strip_suffix("']")))
    {
        return is_reference_path(var).then(|| format!("${{exchangeProperty.{}}}", var));
    }
    if let Some(var) = inner.strip_prefix("vars.") {
        return is_reference_path(var).then(|| format!("${{exchangeProperty.{}}}", var));
    }

    // message.inboundProperties.name -> ${header.name}
    if let Some(prop) = inner.strip_prefix("message.inboundProperties.") {
        return is_reference_path(prop).then(|| format!("${{header.{}}}", prop));
    }

    // attributes.headers.name -> ${header.name}
    if let Some(header) = inner.strip_prefix("attributes.headers.").or(inner
        .strip_prefix("attributes.headers['")
        .and_then(|s| s.strip_suffix("']")))
    {
        return is_reference_path(header).then(|| format!("${{header.{}}}", header));
    }

    // attributes.queryParams.name -> ${header.name}
    if let Some(param) = inner.strip_prefix("attributes.queryParams.") {
        return is_reference_path(param).then(|| format!("${{header.{}}}", param));
    }

    None
}

/// Whether `path` (what follows `payload.`, `vars.`, ...) is a plain path
/// and not the rest of a larger expression: no whitespace, no operator.
fn is_reference_path(path: &str) -> bool {
    !path.is_empty()
        && !path
            .chars()
            .any(|c| c.is_whitespace() || "=!<>+*/~".contains(c))
}

/// A string, number, boolean or `null` literal as Simple writes it. A
/// string literal comes out in single quotes; one that could not be
/// written like that as it is (a quote, an escape, `$` or a brace in it)
/// is not converted.
fn literal_to_simple(operand: &str) -> Option<String> {
    if matches!(operand, "true" | "false" | "null") {
        return Some(operand.to_string());
    }
    if is_number(operand) {
        return Some(operand.to_string());
    }
    string_literal(operand).map(|text| format!("'{text}'"))
}

/// `-12`, `3.5`.
fn is_number(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    let all_digits = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit());
    match digits.split_once('.') {
        Some((whole, fraction)) => all_digits(whole) && all_digits(fraction),
        None => all_digits(digits),
    }
}

/// The text of a single- or double-quoted string literal that means the
/// same in Mule and in Camel as it stands: no quote, backslash, `$`
/// (interpolation in DataWeave and in Simple) or brace (`{{...}}` is a
/// property placeholder in Camel) inside.
fn string_literal(operand: &str) -> Option<&str> {
    let quote = operand.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let text = operand
        .strip_prefix(quote)
        .and_then(|rest| rest.strip_suffix(quote))?;
    let plain = !text
        .chars()
        .any(|c| matches!(c, '\'' | '"' | '\\' | '$' | '{' | '}') || c.is_control());
    plain.then_some(text)
}

/// Split `left OP right` at the first comparison operator (`==`, `!=`,
/// `>=`, `<=`, `>`, `<`) that is not inside a string literal.
fn split_comparison(inner: &str) -> Option<(&str, &'static str, &str)> {
    const OPERATORS: [&str; 6] = ["==", "!=", ">=", "<=", ">", "<"];
    let mut quote: Option<char> = None;
    for (i, c) in inner.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None if c == '\'' || c == '"' => quote = Some(c),
            None => {
                if let Some(op) = OPERATORS.iter().find(|op| inner[i..].starts_with(**op)) {
                    return Some((&inner[..i], op, &inner[i + op.len()..]));
                }
            }
        }
    }
    None
}

/// Replace every `#[...]` expression embedded in a string (e.g. a logger
/// message) with its Camel Simple equivalent. Returns None if any embedded
/// expression cannot be converted (a comparison included, see below), so
/// the caller can flag it for review.
pub fn interpolate_mel(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find("#[") {
        out.push_str(&rest[..start]);
        let expr = &rest[start..];
        // Find the matching `]`, allowing nested brackets such as payload['a'].
        let mut depth = 0usize;
        let mut end = None;
        for (i, c) in expr.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end?;
        // A message is a Simple expression, not a predicate: a comparison
        // would be printed operand by operand instead of as true or false.
        if split_comparison(strip_wrapper(&expr[..=end])).is_some() {
            return None;
        }
        out.push_str(&mel_to_simple(&expr[..=end])?);
        rest = &expr[end + 1..];
    }
    out.push_str(rest);
    Some(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolate_replaces_embedded_payload() {
        assert_eq!(
            interpolate_mel("Served /hello: #[payload]").as_deref(),
            Some("Served /hello: ${body}")
        );
    }

    #[test]
    fn interpolate_handles_several_expressions() {
        assert_eq!(
            interpolate_mel("#[payload] and #[message.payload]").as_deref(),
            Some("${body} and ${body}")
        );
    }

    #[test]
    fn interpolate_leaves_plain_text_alone() {
        assert_eq!(
            interpolate_mel("Received request").as_deref(),
            Some("Received request")
        );
    }

    #[test]
    fn interpolate_rejects_unconvertible_expression() {
        assert_eq!(interpolate_mel("count: #[sizeOf(payload)]"), None);
    }

    // ── mel_to_simple: comparisons ───────────────────────────────────────

    /// The comparison branch used to replace `payload.` with `${body.` and
    /// never close the brace: `${body.type == 'A'`, which is not Simple.
    #[test]
    fn comparison_closes_the_function_on_its_left_side() {
        assert_eq!(
            mel_to_simple("#[payload.type == 'A']").as_deref(),
            Some("${body.type} == 'A'")
        );
    }

    #[test]
    fn comparison_converts_both_sides() {
        assert_eq!(
            mel_to_simple("#[payload.owner != vars.user]").as_deref(),
            Some("${body.owner} != ${exchangeProperty.user}")
        );
        assert_eq!(
            mel_to_simple("#[flowVars.count >= 10]").as_deref(),
            Some("${exchangeProperty.count} >= 10")
        );
        assert_eq!(
            mel_to_simple("#[vars.count < vars.limit]").as_deref(),
            Some("${exchangeProperty.count} < ${exchangeProperty.limit}")
        );
    }

    #[test]
    fn comparison_keeps_literals_as_simple_literals() {
        // Simple takes a string literal in single quotes.
        assert_eq!(
            mel_to_simple(r#"#[payload.type == "A"]"#).as_deref(),
            Some("${body.type} == 'A'")
        );
        assert_eq!(
            mel_to_simple("#[payload.name != null]").as_deref(),
            Some("${body.name} != null")
        );
        assert_eq!(
            mel_to_simple("#[vars.active == true]").as_deref(),
            Some("${exchangeProperty.active} == true")
        );
        // An operator inside a string literal is text, not an operator.
        assert_eq!(
            mel_to_simple("#[vars.op == '>=']").as_deref(),
            Some("${exchangeProperty.op} == '>='")
        );
    }

    /// Simple needs a `${...}` function on the left of an operator, and a
    /// side that is not converted must not be passed through as it is.
    #[test]
    fn comparison_with_an_unconvertible_side_is_not_converted() {
        assert_eq!(mel_to_simple("#[sizeOf(payload) == 2]"), None);
        assert_eq!(mel_to_simple("#[payload.type == upper(vars.t)]"), None);
        assert_eq!(mel_to_simple("#['A' == payload.type]"), None);
        assert_eq!(mel_to_simple("#[payload.a == 1 and payload.b == 2]"), None);
        assert_eq!(mel_to_simple("#[payload.type == 'it's']"), None);
    }

    /// No brace may stay open, whatever the expression is.
    #[test]
    fn converted_expressions_have_balanced_braces() {
        for mel in [
            "#[payload.type == 'A']",
            "#[payload.a.b != vars.c]",
            "#[vars.x <= 3]",
            "#[payload]",
            "#['Hello ' ++ payload.name]",
        ] {
            let simple = mel_to_simple(mel).unwrap_or_else(|| panic!("{mel} not converted"));
            assert_eq!(
                simple.matches("${").count(),
                simple.matches('}').count(),
                "{mel} -> {simple}"
            );
        }
    }

    /// A log message is a Simple expression, not a predicate: Camel would
    /// print both operands and the operator where Mule prints true or false.
    /// So a comparison embedded in a message is left for review.
    #[test]
    fn interpolate_rejects_a_comparison() {
        assert_eq!(interpolate_mel("is A: #[payload.type == 'A']"), None);
    }
}
