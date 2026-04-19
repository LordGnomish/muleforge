//! Pattern-based DataWeave → Java converter.
//!
//! Handles common DataWeave patterns without requiring an LLM call.
//! Falls through to LLM for complex expressions.

use crate::ast::camel_ir::GeneratedBean;
use regex::Regex;

/// Attempt to convert a DataWeave expression using pattern matching.
/// Returns Some(bean) if a pattern matched, None if LLM is needed.
pub fn try_pattern_convert(
    dw_expression: &str,
    class_name: &str,
    _flow_name: &str,
) -> Option<GeneratedBean> {
    let trimmed = dw_expression.trim();

    // Try each pattern in order of specificity
    if let Some(bean) = try_identity_passthrough(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_simple_field_mapping(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_type_coercion(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_string_concat(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_filter_expression(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_map_expression(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_payload_field_access(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_null_coalescing(trimmed, class_name) {
        return Some(bean);
    }

    None // No pattern matched — needs LLM
}

/// %dw 2.0 output application/json --- payload
fn try_identity_passthrough(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    // Matches: payload, or just passing through with output format
    let body = extract_dw_body(dw);
    if body.trim() == "payload" {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;

/**
 * Identity passthrough — original DataWeave simply returned the payload as-is.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        // Passthrough — payload is already in the correct format.
        // If format conversion is needed, configure marshal/unmarshal in the route.
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }
    None
}

/// payload.fieldName or payload.field1.field2
fn try_payload_field_access(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r"^payload\.([a-zA-Z_][\w.]*)$").ok()?;
    let caps = re.captures(body.trim())?;
    let field_path = caps.get(1)?.as_str();

    let parts: Vec<&str> = field_path.split('.').collect();
    let mut access = "body".to_string();
    for part in &parts {
        access = format!(
            "((java.util.Map<String, Object>) {}).get(\"{}\")",
            access, part
        );
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;

/**
 * Extracts field '{field_path}' from the payload.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        Object body = exchange.getIn().getBody();
        Object result = {access};
        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// { field1: payload.x, field2: payload.y }
fn try_simple_field_mapping(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim().to_string();
    if !body.starts_with('{') || !body.ends_with('}') {
        return None;
    }
    let inner = &body[1..body.len() - 1];
    let re = Regex::new(r#"(\w+)\s*:\s*payload\.(\w[\w.]*)"#).ok()?;

    let mappings: Vec<(String, String)> = re
        .captures_iter(inner)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();

    if mappings.is_empty() {
        return None;
    }

    let mut puts = String::new();
    for (target, source) in &mappings {
        puts.push_str(&format!(
            "        result.put(\"{}\", input.get(\"{}\"));\n",
            target, source
        ));
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.HashMap;
import java.util.Map;

/**
 * Field mapping transform.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Map<String, Object> input = exchange.getIn().getBody(Map.class);
        Map<String, Object> result = new HashMap<>();
{puts}        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload as String, payload as Number, etc.
fn try_type_coercion(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r"^payload\s+as\s+(\w+)$").ok()?;
    let caps = re.captures(body.trim())?;
    let target_type = caps.get(1)?.as_str();

    let java_type = match target_type {
        "String" => "String.class",
        "Number" => "Double.class",
        "Boolean" => "Boolean.class",
        _ => return None,
    };

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;

/**
 * Type coercion: payload as {target_type}.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        Object body = exchange.getIn().getBody();
        exchange.getIn().setBody(exchange.getContext().getTypeConverter().convertTo({java_type}, body));
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// "prefix" ++ payload.field ++ "suffix"  or  string concatenation
fn try_string_concat(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    if !body.contains("++") {
        return None;
    }
    let parts: Vec<&str> = body.split("++").map(|s| s.trim()).collect();
    let mut java_parts = Vec::new();
    for part in &parts {
        let p = part.trim_matches('"').trim_matches('\'');
        if part.starts_with('"') || part.starts_with('\'') {
            java_parts.push(format!("\"{}\"", p));
        } else if part.starts_with("payload.") {
            let field = part.strip_prefix("payload.").unwrap_or(part);
            java_parts.push(format!("String.valueOf(input.get(\"{}\"))", field));
        } else if *part == "payload" {
            java_parts.push("String.valueOf(body)".to_string());
        } else {
            return None; // too complex
        }
    }

    let concat_expr = java_parts.join(" + ");
    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.Map;

/**
 * String concatenation transform.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Object body = exchange.getIn().getBody();
        Map<String, Object> input = (body instanceof Map) ? (Map<String, Object>) body : Map.of();
        String result = {concat_expr};
        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload filter ($.age > 18)
fn try_filter_expression(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r"^payload\s+filter\s+\(\$\.(\w+)\s*(==|!=|>|<|>=|<=)\s*(.+)\)$").ok()?;
    let caps = re.captures(body.trim())?;
    let field = caps.get(1)?.as_str();
    let op = caps.get(2)?.as_str();
    let value = caps.get(3)?.as_str().trim().trim_matches('"');

    let java_op = match op {
        "==" => "equals",
        "!=" => "!equals",
        _ => return None, // numeric comparisons need more complex handling
    };

    let comparison = if java_op.starts_with('!') {
        format!(
            "!String.valueOf(item.get(\"{}\")).equals(\"{}\")",
            field, value
        )
    } else {
        format!(
            "String.valueOf(item.get(\"{}\")).equals(\"{}\")",
            field, value
        )
    };

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.List;
import java.util.Map;
import java.util.stream.Collectors;

/**
 * Filter: payload filter ($.{field} {op} {value})
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Map<String, Object>> payload = exchange.getIn().getBody(List.class);
        List<Map<String, Object>> result = payload.stream()
            .filter(item -> {comparison})
            .collect(Collectors.toList());
        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload map { field1: $.x, field2: $.y }
fn try_map_expression(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r"^payload\s+map\s+\{(.+)\}$").ok()?;
    let caps = re.captures(body.trim())?;
    let inner = caps.get(1)?.as_str();

    let field_re = Regex::new(r#"(\w+)\s*:\s*\$\.(\w+)"#).ok()?;
    let mappings: Vec<(String, String)> = field_re
        .captures_iter(inner)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();

    if mappings.is_empty() {
        return None;
    }

    let mut puts = String::new();
    for (target, source) in &mappings {
        puts.push_str(&format!(
            "                mapped.put(\"{}\", item.get(\"{}\"));\n",
            target, source
        ));
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;
import java.util.stream.Collectors;

/**
 * Map transform: payload map {{ ... }}
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Map<String, Object>> payload = exchange.getIn().getBody(List.class);
        List<Map<String, Object>> result = payload.stream()
            .map(item -> {{
                Map<String, Object> mapped = new HashMap<>();
{puts}                return mapped;
            }})
            .collect(Collectors.toList());
        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload.field default "fallback"  or  payload.field ?? "fallback"
fn try_null_coalescing(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r#"^payload\.(\w+)\s+default\s+"([^"]*)"$"#).ok()?;
    let caps = re.captures(body.trim())?;
    let field = caps.get(1)?.as_str();
    let default_val = caps.get(2)?.as_str();

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.Map;

/**
 * Null coalescing: payload.{field} default "{default_val}"
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Map<String, Object> input = exchange.getIn().getBody(Map.class);
        Object value = input != null ? input.get("{field}") : null;
        exchange.getIn().setBody(value != null ? value : "{default_val}");
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

// --- helpers ---

/// Extract the body after `---` in a DataWeave expression.
fn extract_dw_body(dw: &str) -> &str {
    if let Some(idx) = dw.find("---") {
        dw[idx + 3..].trim()
    } else {
        dw.trim()
    }
}

fn make_bean(class_name: &str, java: &str, dw: &str) -> GeneratedBean {
    GeneratedBean {
        class_name: class_name.to_string(),
        package: "generated.beans".into(),
        java_source: java.to_string(),
        origin_dw: Some(dw.to_string()),
    }
}
