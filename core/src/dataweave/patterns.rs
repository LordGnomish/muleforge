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

    if let Some(bean) = try_group_by(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_order_by(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_distinct_by(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_flatten(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_reduce(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_size_of(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_is_empty(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_upper_lower(trimmed, class_name) {
        return Some(bean);
    }

    if let Some(bean) = try_pluck(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_split_by(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_join_by(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_contains(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_replace(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_trim(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_now_uuid(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_write_read(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_if_else(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_object_merge(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_map_object(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_type_check(trimmed, class_name) {
        return Some(bean);
    }
    if let Some(bean) = try_multi_field_construct(trimmed, class_name) {
        return Some(bean);
    }

    None // No pattern matched — needs LLM
}

/// payload pluck (val, key) -> { key: val }  —  object to array of key-value pairs
fn try_pluck(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if !body.contains("pluck") {
        return None;
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;

/**
 * Pluck: converts object entries to array of key-value pairs.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Map<String, Object> payload = exchange.getIn().getBody(Map.class);
        List<Map<String, Object>> result = new ArrayList<>();
        if (payload != null) {{
            payload.forEach((key, value) -> {{
                Map<String, Object> entry = new LinkedHashMap<>();
                entry.put("key", key);
                entry.put("value", value);
                result.add(entry);
            }});
        }}
        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload splitBy ","  or  "string" splitBy regex
fn try_split_by(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r#"^payload\s+splitBy\s+"([^"]+)"$"#).ok()?;
    let caps = re.captures(body.trim())?;
    let delimiter = caps.get(1)?.as_str();

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.Arrays;

/**
 * SplitBy: payload splitBy "{delimiter}"
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        String body = exchange.getIn().getBody(String.class);
        exchange.getIn().setBody(Arrays.asList(body.split("{delimiter}")));
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload joinBy ","
fn try_join_by(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r#"^payload\s+joinBy\s+"([^"]*)"$"#).ok()?;
    let caps = re.captures(body.trim())?;
    let delimiter = caps.get(1)?.as_str();

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.List;
import java.util.stream.Collectors;

/**
 * JoinBy: payload joinBy "{delimiter}"
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Object> body = exchange.getIn().getBody(List.class);
        String result = body.stream().map(String::valueOf).collect(Collectors.joining("{delimiter}"));
        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload contains "value"  or  payload contains value
fn try_contains(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r#"^payload\s+contains\s+"?([^"]*)"?$"#).ok()?;
    let caps = re.captures(body.trim())?;
    let value = caps.get(1)?.as_str();

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.Collection;

/**
 * Contains check: payload contains "{value}"
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Object body = exchange.getIn().getBody();
        boolean result;
        if (body instanceof String) {{
            result = ((String) body).contains("{value}");
        }} else if (body instanceof Collection) {{
            result = ((Collection<?>) body).contains("{value}");
        }} else {{
            result = String.valueOf(body).contains("{value}");
        }}
        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload replace "old" with "new"
fn try_replace(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r#"^payload\s+replace\s+"([^"]*)"\s+with\s+"([^"]*)"$"#).ok()?;
    let caps = re.captures(body.trim())?;
    let old_str = caps.get(1)?.as_str();
    let new_str = caps.get(2)?.as_str();

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;

/**
 * Replace: payload replace "{old_str}" with "{new_str}"
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        String body = exchange.getIn().getBody(String.class);
        exchange.getIn().setBody(body != null ? body.replace("{old_str}", "{new_str}") : null);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// trim(payload)
fn try_trim(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if body != "trim(payload)" && body != "payload trim" {
        return None;
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;

/**
 * trim(payload)
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        String body = exchange.getIn().getBody(String.class);
        exchange.getIn().setBody(body != null ? body.trim() : null);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// now(), uuid()
fn try_now_uuid(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if body == "now()" {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.time.Instant;

@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        exchange.getIn().setBody(Instant.now().toString());
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }
    if body == "uuid()" {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.UUID;

@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        exchange.getIn().setBody(UUID.randomUUID().toString());
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }
    None
}

/// write(payload, "application/json") / read(payload, "application/json")
fn try_write_read(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if body.starts_with("write(payload") {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import com.fasterxml.jackson.databind.ObjectMapper;

/**
 * write(payload, ...) — serialize to JSON
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    private final ObjectMapper mapper = new ObjectMapper();

    @Override
    public void process(Exchange exchange) throws Exception {{
        Object body = exchange.getIn().getBody();
        exchange.getIn().setBody(mapper.writeValueAsString(body));
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }
    if body.starts_with("read(payload") {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.util.Map;

/**
 * read(payload, ...) — deserialize from JSON
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    private final ObjectMapper mapper = new ObjectMapper();

    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        String body = exchange.getIn().getBody(String.class);
        exchange.getIn().setBody(mapper.readValue(body, Map.class));
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }
    None
}

/// if (condition) value1 else value2
fn try_if_else(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    let re = Regex::new(r#"^if\s*\((.+?)\)\s+"([^"]+)"\s+else\s+"([^"]+)"$"#).ok()?;
    let caps = re.captures(body)?;
    let condition = caps.get(1)?.as_str();
    let then_val = caps.get(2)?.as_str();
    let else_val = caps.get(3)?.as_str();

    // Convert condition
    let java_cond = condition
        .replace("payload.", "body.get(\"")
        .replace(" ==", "\").equals(")
        .replace(" !=", "\") != null && !body.get(\"");
    // Simplified — works for basic payload.field == "value" patterns

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.Map;

/**
 * Conditional: if ({condition}) "{then_val}" else "{else_val}"
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Map<String, Object> body = exchange.getIn().getBody(Map.class);
        // TODO: verify condition logic — auto-converted from DataWeave
        boolean condition = {java_cond} != null;
        exchange.getIn().setBody(condition ? "{then_val}" : "{else_val}");
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload ++ { newField: "value" }  —  object merge
fn try_object_merge(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r"^payload\s*\+\+\s*\{(.+)\}$").ok()?;
    let caps = re.captures(body.trim())?;
    let fields = caps.get(1)?.as_str();

    let field_re = Regex::new(r#"(\w+)\s*:\s*"([^"]*)""#).ok()?;
    let pairs: Vec<(String, String)> = field_re
        .captures_iter(fields)
        .map(|c| (c[1].to_string(), c[2].to_string()))
        .collect();

    if pairs.is_empty() {
        return None;
    }

    let mut puts = String::new();
    for (k, v) in &pairs {
        puts.push_str(&format!("        result.put(\"{}\", \"{}\");\n", k, v));
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;

/**
 * Object merge: payload ++ {{ ... }}
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Map<String, Object> payload = exchange.getIn().getBody(Map.class);
        Map<String, Object> result = new LinkedHashMap<>(payload != null ? payload : Map.of());
{puts}        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload mapObject { (key): value }
fn try_map_object(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if !body.contains("mapObject") {
        return None;
    }

    // Simple key transform: payload mapObject { (upper($$)): $ }
    if body.contains("upper($$)") {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;

/**
 * mapObject: transform object keys to uppercase.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Map<String, Object> payload = exchange.getIn().getBody(Map.class);
        Map<String, Object> result = new LinkedHashMap<>();
        if (payload != null) {{
            payload.forEach((k, v) -> result.put(k.toUpperCase(), v));
        }}
        exchange.getIn().setBody(result);
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }

    if body.contains("lower($$)") {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;

/**
 * mapObject: transform object keys to lowercase.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Map<String, Object> payload = exchange.getIn().getBody(Map.class);
        Map<String, Object> result = new LinkedHashMap<>();
        if (payload != null) {{
            payload.forEach((k, v) -> result.put(k.toLowerCase(), v));
        }}
        exchange.getIn().setBody(result);
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }

    None
}

/// payload is String / payload is Number / payload is Array
fn try_type_check(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    let re = Regex::new(r"^payload\s+is\s+:?(\w+)$").ok()?;
    let caps = re.captures(body)?;
    let type_name = caps.get(1)?.as_str();

    let java_check = match type_name {
        "String" => "body instanceof String",
        "Number" => "body instanceof Number",
        "Boolean" => "body instanceof Boolean",
        "Array" => "body instanceof java.util.List",
        "Object" => "body instanceof java.util.Map",
        "Null" | "null" => "body == null",
        _ => return None,
    };

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;

/**
 * Type check: payload is {type_name}
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        Object body = exchange.getIn().getBody();
        exchange.getIn().setBody({java_check});
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// { field1: value1, field2: payload.x, field3: now(), ... } — complex object construction
fn try_multi_field_construct(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if !body.starts_with('{') || !body.ends_with('}') {
        return None;
    }
    let inner = &body[1..body.len() - 1];

    // Match patterns: field: "literal", field: payload.x, field: now(), field: uuid()
    let field_re = Regex::new(r#"(\w+)\s*:\s*([^,}]+)"#).ok()?;
    let fields: Vec<(String, String)> = field_re
        .captures_iter(inner)
        .map(|c| (c[1].to_string(), c[2].trim().to_string()))
        .collect();

    if fields.len() < 2 {
        return None; // Single field handled by simpler patterns
    }

    let mut puts = String::new();
    for (key, value) in &fields {
        let java_value = if value.starts_with('"') && value.ends_with('"') {
            value.clone() // literal string
        } else if value.starts_with("payload.") {
            let field = value.strip_prefix("payload.").unwrap_or(value);
            format!("input.get(\"{}\")", field)
        } else if value == "payload" {
            "exchange.getIn().getBody()".into()
        } else if value == "now()" {
            "java.time.Instant.now().toString()".into()
        } else if value == "uuid()" {
            "java.util.UUID.randomUUID().toString()".into()
        } else if value.starts_with("vars.") || value.starts_with("flowVars.") {
            let var = value.split('.').nth(1).unwrap_or("unknown");
            format!("exchange.getProperty(\"{}\", Object.class)", var)
        } else if value.starts_with("attributes.") {
            let attr = value.split('.').nth(1).unwrap_or("unknown");
            format!("exchange.getIn().getHeader(\"{}\")", attr)
        } else {
            format!("\"{}\" /* TODO: verify */", value)
        };
        puts.push_str(&format!(
            "        result.put(\"{}\", {});\n",
            key, java_value
        ));
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;

/**
 * Object construction with multiple fields.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        Map<String, Object> input = exchange.getIn().getBody(Map.class);
        if (input == null) input = Map.of();
        Map<String, Object> result = new LinkedHashMap<>();
{puts}        exchange.getIn().setBody(result);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload groupBy $.field
fn try_group_by(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r"^payload\s+groupBy\s+\$\.(\w+)$").ok()?;
    let caps = re.captures(body.trim())?;
    let field = caps.get(1)?.as_str();

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;
import java.util.stream.Collectors;

/**
 * GroupBy: payload groupBy $.{field}
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Map<String, Object>> payload = exchange.getIn().getBody(List.class);
        Map<Object, List<Map<String, Object>>> grouped = payload.stream()
            .collect(Collectors.groupingBy(item -> item.get("{field}")));
        exchange.getIn().setBody(grouped);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload orderBy $.field
fn try_order_by(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r"^payload\s+orderBy\s+\$\.(\w+)$").ok()?;
    let caps = re.captures(body.trim())?;
    let field = caps.get(1)?.as_str();

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;
import java.util.stream.Collectors;

/**
 * OrderBy: payload orderBy $.{field}
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Map<String, Object>> payload = exchange.getIn().getBody(List.class);
        List<Map<String, Object>> sorted = payload.stream()
            .sorted(Comparator.comparing(item -> String.valueOf(item.get("{field}"))))
            .collect(Collectors.toList());
        exchange.getIn().setBody(sorted);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload distinctBy $.field
fn try_distinct_by(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    let re = Regex::new(r"^payload\s+distinctBy\s+\$\.(\w+)$").ok()?;
    let caps = re.captures(body.trim())?;
    let field = caps.get(1)?.as_str();

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;
import java.util.stream.Collectors;

/**
 * DistinctBy: payload distinctBy $.{field}
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Map<String, Object>> payload = exchange.getIn().getBody(List.class);
        Set<Object> seen = new HashSet<>();
        List<Map<String, Object>> distinct = payload.stream()
            .filter(item -> seen.add(item.get("{field}")))
            .collect(Collectors.toList());
        exchange.getIn().setBody(distinct);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// flatten
fn try_flatten(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if body != "flatten" && body != "payload flatten" && body != "flatten payload" {
        return None;
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.*;
import java.util.stream.Collectors;

/**
 * Flatten: nested arrays into single array.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Object> payload = exchange.getIn().getBody(List.class);
        List<Object> flat = payload.stream()
            .flatMap(item -> {{
                if (item instanceof List) {{
                    return ((List<Object>) item).stream();
                }}
                return java.util.stream.Stream.of(item);
            }})
            .collect(Collectors.toList());
        exchange.getIn().setBody(flat);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// payload reduce ($$, $) -> $$ ++ $ (sum/concat patterns)
fn try_reduce(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw);
    if !body.contains("reduce") {
        return None;
    }

    // Simple sum: payload reduce ($$ + $)
    if body.contains("$$ + $") || body.contains("$$+$") {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.List;

/**
 * Reduce (sum): payload reduce ($$ + $)
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Number> payload = exchange.getIn().getBody(List.class);
        double sum = payload.stream().mapToDouble(Number::doubleValue).sum();
        exchange.getIn().setBody(sum);
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }

    // Concat: payload reduce ($$ ++ $)
    if body.contains("$$ ++ $") || body.contains("$$++$") {
        let java = format!(
            r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.List;
import java.util.stream.Collectors;

/**
 * Reduce (concat): payload reduce ($$ ++ $)
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    @SuppressWarnings("unchecked")
    public void process(Exchange exchange) throws Exception {{
        List<Object> payload = exchange.getIn().getBody(List.class);
        String result = payload.stream().map(String::valueOf).collect(Collectors.joining());
        exchange.getIn().setBody(result);
    }}
}}
"#
        );
        return Some(make_bean(class_name, &java, dw));
    }

    None
}

/// sizeOf(payload)
fn try_size_of(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if body != "sizeOf(payload)" && body != "sizeOf payload" {
        return None;
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.Collection;

/**
 * sizeOf(payload) — returns the size/length of the payload.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        Object body = exchange.getIn().getBody();
        int size;
        if (body instanceof Collection) {{
            size = ((Collection<?>) body).size();
        }} else if (body instanceof String) {{
            size = ((String) body).length();
        }} else if (body instanceof byte[]) {{
            size = ((byte[]) body).length;
        }} else {{
            size = String.valueOf(body).length();
        }}
        exchange.getIn().setBody(size);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// isEmpty(payload)
fn try_is_empty(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    if body != "isEmpty(payload)" && body != "payload is :empty" {
        return None;
    }

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;
import java.util.Collection;

/**
 * isEmpty(payload) — checks if payload is empty/null.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        Object body = exchange.getIn().getBody();
        boolean empty;
        if (body == null) {{
            empty = true;
        }} else if (body instanceof Collection) {{
            empty = ((Collection<?>) body).isEmpty();
        }} else if (body instanceof String) {{
            empty = ((String) body).isEmpty();
        }} else {{
            empty = false;
        }}
        exchange.getIn().setBody(empty);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
}

/// upper(payload) / lower(payload)
fn try_upper_lower(dw: &str, class_name: &str) -> Option<GeneratedBean> {
    let body = extract_dw_body(dw).trim();
    let (func, method) = if body == "upper(payload)" || body == "payload upper" {
        ("upper", "toUpperCase")
    } else if body == "lower(payload)" || body == "payload lower" {
        ("lower", "toLowerCase")
    } else {
        return None;
    };

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;

/**
 * {func}(payload) — converts payload string to {func}case.
 */
@ApplicationScoped
public class {class_name} implements Processor {{
    @Override
    public void process(Exchange exchange) throws Exception {{
        String body = exchange.getIn().getBody(String.class);
        exchange.getIn().setBody(body != null ? body.{method}() : null);
    }}
}}
"#
    );
    Some(make_bean(class_name, &java, dw))
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
