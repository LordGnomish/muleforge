//! MEL (Mule Expression Language) to Camel Simple language converter.
//!
//! MEL expressions use #[...] syntax in Mule 3/4. Most can be directly
//! mapped to Camel's Simple expression language.
//!
//! Two conversions live here:
//!
//! * [`mel_to_simple`] / [`interpolate_mel`]: lenient, for text that is
//!   only displayed (logger messages).
//! * [`convert_value`] / [`convert_condition`]: strict, for what decides
//!   what a route does (set-variable and set-payload values, choice
//!   conditions). They convert the shapes that mean the same in Camel
//!   whatever the values are, and return [`Expression::Unconverted`] with
//!   the reason for everything else. `docs/mapping/MATRIX.md`
//!   ("Expressions") has the table and the reasons.

use crate::ast::camel_ir::{ComparedValue, Expression};

// ── Strict conversion ───────────────────────────────────────────────────

const WHY_PAYLOAD_FIELD: &str = "it reads a field of the payload: Mule reads it from JSON, XML, \
     a Map or an object according to the MIME type of the payload, while Simple's \
     ${body.field} calls a getter on a Java object and does not read a field out of a JSON or \
     XML body; the payload type is not known to the migrator";
const WHY_PAYLOAD_COMPARED: &str = "it compares the payload: Mule compares the value it reads \
     according to the MIME type of the payload (the JSON document \"A\" equals 'A', an empty \
     body is null), Simple compares the body as it arrived; the payload type is not known to \
     the migrator";
const WHY_ATTRIBUTES: &str = "it reads message attributes: what they hold depends on the source \
     and on the last connector operation of the flow, which the conversion does not follow \
     (and Camel keeps HTTP request headers and query parameters in one header map)";
const WHY_VARIABLE_FIELD: &str = "it reads into the value of a variable (a field, or a name \
     that is not a plain identifier), which depends on the type of the value the variable holds";
const WHY_NOT_BOOLEAN: &str = "it uses a value as the condition: Mule expects a Boolean, while \
     Simple takes every value except null, false, \"false\" and \"\" as true";
const WHY_ORDERING: &str = "it compares with <, >, <= or >=: how two values are ordered depends \
     on their types (number or text), which are not known to the migrator";
const WHY_EMBEDDED: &str = "it mixes text and #[...] expressions, or is not one complete \
     #[...] expression";
const WHY_PROPERTY_PLACEHOLDER: &str = "it contains a Mule property placeholder (${key}), \
     which Mule replaces with the value of the property when the application starts, and the \
     property is not in the properties files of the project; as a constant the route would hand \
     on the text ${key} itself. The usual replacement is a Camel property placeholder \
     ({{key}}), resolved from application.properties; it is not written automatically";
const WHY_OTHER_VALUE: &str = "it is not one of the shapes that are converted: payload, \
     vars.name, or a string, integer or boolean literal";
const WHY_OTHER_CONDITION: &str = "it is not one of the shapes that are converted: vars.name \
     == or != a string, integer, boolean or null literal, or true / false";
const WHY_TWO_VARIABLES: &str = "it compares two variables: Mule's == is true only if both \
     values have the same type and are equal, and there is no simple Camel predicate that is \
     exactly that for any two Java values (Simple's == converts one side to the type of the \
     other)";
/// The note on a comparison written with `flowVars`.
const REVIEW_MEL: &str = "it is written with flowVars, the Mule 3 (MEL) syntax: the predicate \
     compares without converting types, as Mule 4 does; what MEL's == does with values of \
     different types was not checked";

/// An operand that means the same in Mule and in Camel.
enum Operand {
    Payload,
    /// A flow variable, by name.
    Variable(String),
    Text(String),
    Integer(i32),
    Boolean(bool),
    Null,
}

/// The value of a `set-variable` / `set-payload` as a Camel expression.
///
/// * no `#[...]`: a string in Mule, a string constant in Camel - unless
///   it still has a `${key}` property placeholder in it (one the normalizer
///   could not resolve), which Mule would replace at startup;
/// * `#[payload]`: the message body, `${body}`;
/// * `#[vars.name]`: the exchange property `name` (`set-variable` is
///   `setProperty`), `${exchangeProperty.name}`;
/// * `#['text']`, `#[42]`, `#[true]`: a constant of the same type.
///
/// Everything else is [`Expression::Unconverted`].
pub fn convert_value(raw: &str) -> Expression {
    if !raw.contains("#[") {
        if raw.contains("${") {
            return unconverted(raw, WHY_PROPERTY_PLACEHOLDER);
        }
        return Expression::Constant(raw.to_string());
    }
    let Some(inner) = whole_expression(raw) else {
        return unconverted(raw, WHY_EMBEDDED);
    };
    match parse_operand(inner) {
        Some(Operand::Payload) => Expression::Simple("${body}".into()),
        Some(Operand::Variable(name)) => Expression::Simple(exchange_property(&name)),
        Some(Operand::Text(text)) => Expression::Constant(text),
        Some(Operand::Integer(number)) => Expression::Literal(number.to_string()),
        Some(Operand::Boolean(value)) => Expression::Literal(value.to_string()),
        Some(Operand::Null) => unconverted(raw, WHY_OTHER_VALUE),
        None => unconverted(raw, why_not_an_operand(inner).unwrap_or(WHY_OTHER_VALUE)),
    }
}

/// The `expression` of a choice `<when>` as a Camel predicate.
///
/// * `vars.name == literal`, `vars.name != literal`, with a string,
///   integer, boolean or `null` literal on the other side:
///   [`Expression::PropertyEquals`], the comparison of the exchange
///   property `name` with that literal. Mule 4's `==` does not convert
///   types (`"true" == true` is false), so it is not a Simple expression,
///   whose `==` does; the emitter writes a predicate on the Java value.
///   Whether the property holds what the Mule variable holds is not known
///   here: the mapper looks at what sets the variable and adds what has to
///   be reviewed. A comparison written with `flowVars` (Mule 3) already
///   comes with a note.
/// * `true` / `false`: a boolean constant.
///
/// Everything else is [`Expression::Unconverted`], two variables compared
/// with each other included.
pub fn convert_condition(raw: &str) -> Expression {
    let inner = whole_expression(raw).unwrap_or_else(|| raw.trim());
    if inner.contains("#[") {
        return unconverted(raw, WHY_EMBEDDED);
    }
    if inner == "true" || inner == "false" {
        return Expression::Literal(inner.to_string());
    }

    let Some((left, operator, right)) = split_comparison(inner) else {
        let why = match parse_operand(inner) {
            Some(Operand::Payload | Operand::Variable(_)) => WHY_NOT_BOOLEAN,
            _ => why_not_an_operand(inner).unwrap_or(WHY_OTHER_CONDITION),
        };
        return unconverted(raw, why);
    };

    let (left, right) = (left.trim(), right.trim());
    let (parsed_left, parsed_right) = match (parse_operand(left), parse_operand(right)) {
        (Some(left), Some(right)) => (left, right),
        (None, _) => {
            return unconverted(raw, why_not_an_operand(left).unwrap_or(WHY_OTHER_CONDITION))
        }
        (_, None) => {
            return unconverted(
                raw,
                why_not_an_operand(right).unwrap_or(WHY_OTHER_CONDITION),
            )
        }
    };
    if matches!(parsed_left, Operand::Payload) || matches!(parsed_right, Operand::Payload) {
        return unconverted(raw, WHY_PAYLOAD_COMPARED);
    }
    if operator != "==" && operator != "!=" {
        return unconverted(raw, WHY_ORDERING);
    }
    // `==` and `!=` do not care which side is which.
    let (name, other) = match (parsed_left, parsed_right) {
        (Operand::Variable(_), Operand::Variable(_)) => return unconverted(raw, WHY_TWO_VARIABLES),
        (Operand::Variable(name), other) | (other, Operand::Variable(name)) => (name, other),
        _ => return unconverted(raw, WHY_OTHER_CONDITION),
    };
    let value = match other {
        Operand::Text(text) => ComparedValue::Text(text),
        Operand::Integer(number) => ComparedValue::Integer(number),
        Operand::Boolean(value) => ComparedValue::Boolean(value),
        Operand::Null => ComparedValue::Null,
        Operand::Payload | Operand::Variable(_) => return unconverted(raw, WHY_OTHER_CONDITION),
    };
    let mel = [left, right]
        .iter()
        .any(|side| side.starts_with("flowVars"));
    Expression::PropertyEquals {
        original: raw.to_string(),
        name,
        value,
        negated: operator == "!=",
        review: if mel {
            vec![REVIEW_MEL.to_string()]
        } else {
            Vec::new()
        },
    }
}

fn unconverted(raw: &str, reason: &str) -> Expression {
    Expression::Unconverted {
        original: raw.to_string(),
        reason: reason.to_string(),
    }
}

fn exchange_property(name: &str) -> String {
    format!("${{exchangeProperty.{name}}}")
}

/// The expression inside `raw` if `raw` is one complete `#[...]`
/// expression: the bracket that closes `#[` is its last character.
fn whole_expression(raw: &str) -> Option<&str> {
    let body = raw.trim().strip_prefix("#[")?;
    let mut depth = 1usize;
    let mut quote: Option<char> = None;
    for (i, c) in body.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => quote = Some(c),
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return (i + 1 == body.len()).then(|| body[..i].trim());
                    }
                }
                _ => {}
            },
        }
    }
    None
}

fn parse_operand(operand: &str) -> Option<Operand> {
    match operand {
        "payload" | "message.payload" => return Some(Operand::Payload),
        "true" => return Some(Operand::Boolean(true)),
        "false" => return Some(Operand::Boolean(false)),
        "null" => return Some(Operand::Null),
        _ => {}
    }
    if let Some(text) = string_literal(operand) {
        return Some(Operand::Text(text.to_string()));
    }
    if is_number(operand) {
        // An integer that is the same Java `int` literal; a decimal or a
        // larger number has no single Java type that is right.
        return operand.parse::<i32>().ok().map(Operand::Integer);
    }
    let selector = operand
        .strip_prefix("vars")
        .or_else(|| operand.strip_prefix("flowVars"))?;
    let name = match selector.strip_prefix('.') {
        Some(name) => name,
        None => string_literal(selector.strip_prefix('[')?.strip_suffix(']')?)?,
    };
    is_identifier(name).then(|| Operand::Variable(name.to_string()))
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Why `operand` (which [`parse_operand`] did not take) is not converted,
/// if it is one of the references that need more than the migrator knows.
fn why_not_an_operand(operand: &str) -> Option<&'static str> {
    let starts_with_any =
        |prefixes: &[&str]| prefixes.iter().any(|prefix| operand.starts_with(prefix));
    if starts_with_any(&[
        "payload.",
        "payload[",
        "message.payload.",
        "message.payload[",
    ]) {
        Some(WHY_PAYLOAD_FIELD)
    } else if starts_with_any(&[
        "attributes",
        "message.attributes",
        "message.inboundProperties",
    ]) {
        Some(WHY_ATTRIBUTES)
    } else if starts_with_any(&["vars.", "vars[", "flowVars.", "flowVars["])
        && !operand.contains(char::is_whitespace)
    {
        Some(WHY_VARIABLE_FIELD)
    } else {
        None
    }
}

// ── Lenient conversion (logger messages) ────────────────────────────────

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

    // ── convert_value: set-variable / set-payload values ─────────────────

    fn simple(s: &str) -> Expression {
        Expression::Simple(s.into())
    }

    fn constant(s: &str) -> Expression {
        Expression::Constant(s.into())
    }

    fn literal(s: &str) -> Expression {
        Expression::Literal(s.into())
    }

    /// The reason of an unconverted expression; panics if it was converted.
    fn reason_of(raw: &str, converted: Expression) -> String {
        match converted {
            Expression::Unconverted { original, reason } => {
                assert_eq!(original, raw, "the Mule expression is kept as written");
                assert!(!reason.is_empty());
                reason
            }
            other => panic!("{raw} must not be converted, got {other:?}"),
        }
    }

    /// `${key}` in a value without `#[...]` is a property placeholder: Mule
    /// puts the value of the property there when the application starts.
    /// (The normalizer has already replaced the placeholders it could
    /// resolve from the properties files of the project.) As a string
    /// constant the flow would hand on the text `${key}` itself.
    #[test]
    fn value_with_a_property_placeholder_is_not_converted() {
        for raw in [
            "${app.greeting}",
            "Hello ${app.name}!",
            "${secure::db.password}",
            "${a}${b}",
        ] {
            let reason = reason_of(raw, convert_value(raw));
            assert!(
                reason.contains("Mule property placeholder"),
                "{raw}: {reason}"
            );
            assert!(
                reason.contains("Camel property placeholder"),
                "{raw}: {reason}"
            );
            assert!(reason.contains("{{key}}"), "{raw}: {reason}");
        }
        // A dollar sign or a brace alone is text.
        for raw in ["costs $5", "{braces}", "$ {not one}", "100$"] {
            assert_eq!(convert_value(raw), constant(raw), "{raw}");
        }
    }

    /// A value without `#[...]` is a string in Mule, `value="true"` included.
    #[test]
    fn value_without_expression_is_a_string_constant() {
        assert_eq!(
            convert_value("Hello from MuleForge!"),
            constant("Hello from MuleForge!")
        );
        assert_eq!(convert_value("true"), constant("true"));
        assert_eq!(convert_value("42"), constant("42"));
        assert_eq!(convert_value(""), constant(""));
    }

    #[test]
    fn value_payload_is_the_message_body() {
        for raw in ["#[payload]", "#[ payload ]", "#[message.payload]"] {
            assert_eq!(convert_value(raw), simple("${body}"), "{raw}");
        }
    }

    #[test]
    fn value_variable_is_an_exchange_property() {
        for raw in [
            "#[vars.orderId]",
            "#[vars['orderId']]",
            r#"#[vars["orderId"]]"#,
            "#[flowVars.orderId]",
        ] {
            assert_eq!(
                convert_value(raw),
                simple("${exchangeProperty.orderId}"),
                "{raw}"
            );
        }
    }

    /// A string stays a string; a number or boolean must not become one.
    #[test]
    fn value_literal_keeps_its_type() {
        assert_eq!(convert_value("#['ok']"), constant("ok"));
        assert_eq!(convert_value(r#"#["two words"]"#), constant("two words"));
        assert_eq!(convert_value("#[42]"), literal("42"));
        assert_eq!(convert_value("#[-1]"), literal("-1"));
        assert_eq!(convert_value("#[true]"), literal("true"));
        assert_eq!(convert_value("#[false]"), literal("false"));
    }

    #[test]
    fn value_that_reads_into_the_payload_is_not_converted() {
        for raw in [
            "#[payload.id]",
            "#[payload.items[0]]",
            "#[payload['id']]",
            "#[message.payload.id]",
        ] {
            let reason = reason_of(raw, convert_value(raw));
            assert!(reason.contains("field of the payload"), "{raw}: {reason}");
        }
    }

    #[test]
    fn value_of_any_other_shape_is_not_converted() {
        for raw in [
            "#[now()]",
            "#[uuid()]",
            "#['a' ++ vars.b]",
            "#[vars.order.id]",
            "#[vars['order-id']]",
            "#[attributes.headers.host]",
            "#[attributes.queryParams.id]",
            "#[null]",
            "#[3.5]",
            "#[99999999999]",
            "#[vars.a == 'x']",
            "#[output application/json --- {a: 1}]",
            "#[if (vars.a == 1) 'x' else 'y']",
            "Hello #[payload]",
            "#[payload] and #[vars.x]",
            "#['it's']",
            "#['$(payload)']",
            "#['{{secret}}']",
            "#[payload",
        ] {
            reason_of(raw, convert_value(raw));
        }
    }

    // ── convert_condition: choice conditions ─────────────────────────────

    /// `vars.name == value` (`negated`: `!=`) as [`convert_condition`]
    /// gives it for the Mule expression `raw`, with nothing to review.
    fn compared(raw: &str, name: &str, value: ComparedValue, negated: bool) -> Expression {
        Expression::PropertyEquals {
            original: raw.into(),
            name: name.into(),
            value,
            negated,
            review: vec![],
        }
    }

    fn text(s: &str) -> ComparedValue {
        ComparedValue::Text(s.into())
    }

    /// Mule's `==` does not convert types (`"true" == true` is false), so
    /// the comparison is not written in Simple, whose `==` does: it keeps
    /// the name, the literal with its type, and whether it is `==` or `!=`.
    #[test]
    fn condition_compares_a_variable_with_a_literal() {
        for (raw, name, value, negated) in [
            ("#[vars.type == 'A']", "type", text("A"), false),
            (r#"#[vars.type != "A"]"#, "type", text("A"), true),
            (
                "#[vars.count == 3]",
                "count",
                ComparedValue::Integer(3),
                false,
            ),
            (
                "#[vars.count != -1]",
                "count",
                ComparedValue::Integer(-1),
                true,
            ),
            ("#[vars.count == '3']", "count", text("3"), false),
            (
                "#[vars.active == true]",
                "active",
                ComparedValue::Boolean(true),
                false,
            ),
            (
                "#[vars.active != false]",
                "active",
                ComparedValue::Boolean(false),
                true,
            ),
            ("#[vars.active == 'true']", "active", text("true"), false),
            ("#[vars.id != null]", "id", ComparedValue::Null, true),
            ("#[vars.id == null]", "id", ComparedValue::Null, false),
            ("#[vars['type']=='A']", "type", text("A"), false),
            ("vars.type == 'A'", "type", text("A"), false),
        ] {
            assert_eq!(
                convert_condition(raw),
                compared(raw, name, value, negated),
                "{raw}"
            );
        }
    }

    /// No Simple expression is produced for a comparison, whatever the
    /// literal is.
    #[test]
    fn condition_is_never_a_simple_comparison() {
        for raw in [
            "#[vars.a == 'x']",
            "#[vars.a != 'x']",
            "#[vars.a == 1]",
            "#[vars.a == true]",
            "#[vars.a == null]",
            "#[vars.a == vars.b]",
            "#[flowVars.a == 'x']",
        ] {
            assert!(
                !matches!(convert_condition(raw), Expression::Simple(_)),
                "{raw}"
            );
        }
    }

    /// Mule's `==` is true for two variables only if their values have the
    /// same type and are equal. There is no cheap Camel predicate that is
    /// exactly that for any two Java values, so it is left to a person.
    #[test]
    fn condition_that_compares_two_variables_is_not_converted() {
        for raw in [
            "#[vars.owner == vars.user]",
            "#[vars.owner != vars.user]",
            "#[flowVars.owner == vars.user]",
        ] {
            let reason = reason_of(raw, convert_condition(raw));
            assert!(reason.contains("two variables"), "{raw}: {reason}");
        }
    }

    /// `==` and `!=` mean the same with their sides swapped.
    #[test]
    fn condition_with_the_literal_on_the_left_is_the_same_comparison() {
        for (raw, name, value, negated) in [
            ("#['A' == vars.type]", "type", text("A"), false),
            ("#[null != vars.id]", "id", ComparedValue::Null, true),
            (
                "#[3 == vars.count]",
                "count",
                ComparedValue::Integer(3),
                false,
            ),
        ] {
            assert_eq!(
                convert_condition(raw),
                compared(raw, name, value, negated),
                "{raw}"
            );
        }
    }

    /// `flowVars` is Mule 3 (MEL). The comparison is converted like the
    /// Mule 4 one, with a note: what MEL's `==` does with values of
    /// different types was not checked.
    #[test]
    fn condition_on_flow_vars_is_converted_with_a_note_to_review() {
        for raw in ["#[flowVars.type == 'A']", "#['A' == flowVars.type]"] {
            match convert_condition(raw) {
                Expression::PropertyEquals {
                    original,
                    name,
                    value,
                    negated,
                    review,
                } => {
                    assert_eq!(original, raw);
                    assert_eq!(name, "type");
                    assert_eq!(value, text("A"));
                    assert!(!negated);
                    assert_eq!(review.len(), 1, "{review:?}");
                    assert!(review[0].contains("flowVars"), "{review:?}");
                    assert!(review[0].contains("Mule 3"), "{review:?}");
                }
                other => panic!("{raw}: {other:?}"),
            }
        }
    }

    #[test]
    fn condition_boolean_literal_is_a_boolean_constant() {
        assert_eq!(convert_condition("#[true]"), literal("true"));
        assert_eq!(convert_condition("#[false]"), literal("false"));
    }

    /// The condition of `examples/hello-world`: `${body.type}` would call
    /// getType() on the body, it does not read a field of a JSON payload.
    #[test]
    fn condition_on_a_payload_field_is_not_converted() {
        let raw = "#[payload.type == 'A']";
        let reason = reason_of(raw, convert_condition(raw));
        assert!(reason.contains("field of the payload"), "{reason}");
    }

    #[test]
    fn condition_that_compares_the_payload_is_not_converted() {
        for raw in [
            "#[payload == 'A']",
            "#[payload != null]",
            "#['A' == payload]",
        ] {
            let reason = reason_of(raw, convert_condition(raw));
            assert!(reason.contains("compares the payload"), "{raw}: {reason}");
        }
    }

    #[test]
    fn condition_of_any_other_shape_is_not_converted() {
        for raw in [
            "#[vars.count > 3]",
            "#[vars.count <= vars.limit]",
            "#[vars.flag]",
            "#[payload]",
            "#[vars.a == 'x' and vars.b == 'y']",
            "#[vars.a == 'x' or vars.b == 'y']",
            "#[not (vars.a == 'x')]",
            "#[isEmpty(payload)]",
            "#[sizeOf(vars.items) == 2]",
            "#[attributes.queryParams.id == '1']",
            "#[attributes.headers['x-tenant'] == 'a']",
            "#[vars.order.id == 1]",
            "#[vars.a ~= 1]",
            "#['a' == 'a']",
            "#[vars.a == 'it's']",
            "#[vars.a == '${x}']",
            "#[vars.a == 1.5]",
            "",
        ] {
            reason_of(raw, convert_condition(raw));
        }
    }

    // ── Collections (foreach) ───────────────────────────────────────────

    /// The payload is the body, a variable is an exchange property. Camel's
    /// split iterates a Java collection only, so both come with the
    /// assumption they rest on.
    #[test]
    fn collection_that_is_the_payload_is_the_body_on_an_assumption() {
        for raw in ["#[payload]", "#[message.payload]", "#[ payload ]"] {
            let (expression, assumption) = convert_collection(raw);
            assert_eq!(expression, Expression::Simple("${body}".into()), "{raw}");
            let assumption = assumption.unwrap_or_else(|| panic!("{raw}: no assumption"));
            assert!(
                assumption.contains("assuming the body is a Java collection"),
                "{raw}: {assumption}"
            );
        }
    }

    #[test]
    fn collection_that_is_a_variable_is_the_exchange_property_on_an_assumption() {
        for raw in ["#[vars.items]", "#[vars['items']]", "#[flowVars.items]"] {
            let (expression, assumption) = convert_collection(raw);
            assert_eq!(
                expression,
                Expression::Simple("${exchangeProperty.items}".into()),
                "{raw}"
            );
            let assumption = assumption.unwrap_or_else(|| panic!("{raw}: no assumption"));
            assert!(
                assumption.contains("assuming the exchange property items holds a Java collection"),
                "{raw}: {assumption}"
            );
        }
    }

    #[test]
    fn collection_of_any_other_shape_is_not_converted() {
        for (raw, why) in [
            ("#[payload.items]", "reads a field of the payload"),
            ("#[payload.lines()]", "reads a field of the payload"),
            ("#[payload['items']]", "reads a field of the payload"),
            ("#[vars.order.lines]", "reads into the value of a variable"),
            ("#[attributes.queryParams.ids]", "reads message attributes"),
            ("#[1 to 3]", "not one of the shapes"),
            ("#[['a', 'b']]", "not one of the shapes"),
            ("#[payload filter ($.active)]", "not one of the shapes"),
            (
                "#[output application/json --- payload.items]",
                "not one of the shapes",
            ),
            ("#[sizeOf(payload)]", "not one of the shapes"),
            ("#['a,b,c']", "not one of the shapes"),
            ("#[42]", "not one of the shapes"),
            ("#[null]", "not one of the shapes"),
            ("payload", "not one complete #[...] expression"),
            ("a,b,c", "not one complete #[...] expression"),
            ("items of #[payload]", "not one complete #[...] expression"),
            ("", "not one complete #[...] expression"),
        ] {
            let (expression, assumption) = convert_collection(raw);
            assert_eq!(assumption, None, "{raw}");
            let reason = reason_of(raw, expression);
            assert!(reason.contains(why), "{raw}: {reason}");
        }
    }
}
