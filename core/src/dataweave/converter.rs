//! DataWeave → Java bean converter.
//!
//! Given a DataWeave expression and optional context (flow name, input/output
//! types), produces a Java bean class that performs the equivalent transformation.

use crate::ast::camel_ir::GeneratedBean;
use crate::ast::mule_ast::{MuleElement, MuleFlow};
use crate::llm::{self, LlmProvider, TaskKind, TransformRequest};

/// A DataWeave expression found in the Mule project, with context.
#[derive(Debug, Clone)]
pub struct DataWeaveExpression {
    pub expression: String,
    pub flow_name: String,
    pub source_file: String,
    pub line: Option<u32>,
    /// Any surrounding context (e.g., the set-payload parent, variable name).
    pub context_hint: String,
}

/// Extract all DataWeave expressions from a Mule project's flows.
pub fn extract_dataweave_expressions(flows: &[MuleFlow]) -> Vec<DataWeaveExpression> {
    let mut exprs = Vec::new();
    for flow in flows {
        for proc in &flow.processors {
            extract_from_element(
                proc,
                &flow.name,
                &flow.source_file.display().to_string(),
                &mut exprs,
            );
        }
    }
    exprs
}

fn extract_from_element(
    elem: &MuleElement,
    flow_name: &str,
    source_file: &str,
    out: &mut Vec<DataWeaveExpression>,
) {
    // ee:transform or transform elements contain DataWeave
    if (elem.namespace == "ee" || elem.namespace.is_empty()) && elem.name == "transform" {
        for child in &elem.children {
            if let Some(ref text) = child.text {
                let trimmed = text.trim();
                if !trimmed.is_empty()
                    && (trimmed.contains("%dw")
                        || trimmed.contains("---")
                        || trimmed.contains("payload"))
                {
                    out.push(DataWeaveExpression {
                        expression: trimmed.to_string(),
                        flow_name: flow_name.to_string(),
                        source_file: source_file.to_string(),
                        line: elem.line,
                        context_hint: format!("In flow '{}', transform element", flow_name),
                    });
                }
            }
        }
    }

    // set-payload with DataWeave value
    if elem.name == "set-payload" {
        if let Some(val) = elem.attr("value") {
            if val.contains("#[")
                && (val.contains("payload") || val.contains("vars") || val.contains("attributes"))
            {
                out.push(DataWeaveExpression {
                    expression: val.to_string(),
                    flow_name: flow_name.to_string(),
                    source_file: source_file.to_string(),
                    line: elem.line,
                    context_hint: format!("set-payload in flow '{}'", flow_name),
                });
            }
        }
    }

    // Recurse into children
    for child in &elem.children {
        extract_from_element(child, flow_name, source_file, out);
    }
}

/// Convert DataWeave expressions to Java beans using an LLM provider.
/// Returns generated beans ready for emission.
pub async fn convert_with_llm(
    expressions: &[DataWeaveExpression],
    llm: &dyn LlmProvider,
) -> Vec<GeneratedBean> {
    let mut beans = Vec::new();
    let system = llm::dataweave_system_prompt();

    for (i, dw) in expressions.iter().enumerate() {
        let class_name = sanitize_class_name(&dw.flow_name, i);

        // Try pattern-based conversion first (no LLM call needed)
        if let Some(bean) = crate::dataweave::patterns::try_pattern_convert(
            &dw.expression,
            &class_name,
            &dw.flow_name,
        ) {
            tracing::info!(
                "Pattern-matched DataWeave in {} (line {:?}) -> {} (no LLM needed)",
                dw.flow_name,
                dw.line,
                class_name
            );
            beans.push(bean);
            continue;
        }

        let user = llm::dataweave_user_prompt(&dw.expression, &dw.context_hint);

        let req = TransformRequest {
            task: TaskKind::DataWeaveToCamel,
            system: system.clone(),
            user,
        };

        match llm.transform(req).await {
            Ok(resp) => {
                beans.push(GeneratedBean {
                    class_name: class_name.clone(),
                    package: "generated.beans".into(),
                    java_source: resp.output,
                    origin_dw: Some(dw.expression.clone()),
                });
                tracing::info!(
                    "Converted DataWeave in {} (line {:?}) → {}",
                    dw.flow_name,
                    dw.line,
                    class_name
                );
            }
            Err(e) => {
                tracing::warn!(
                    "LLM conversion failed for DataWeave in {} (line {:?}): {}. Generating stub.",
                    dw.flow_name,
                    dw.line,
                    e
                );
                beans.push(generate_stub_bean(
                    &class_name,
                    &dw.expression,
                    &dw.context_hint,
                ));
            }
        }
    }

    beans
}

/// Generate stub beans when no LLM is available.
pub fn convert_without_llm(expressions: &[DataWeaveExpression]) -> Vec<GeneratedBean> {
    expressions
        .iter()
        .enumerate()
        .map(|(i, dw)| {
            let class_name = sanitize_class_name(&dw.flow_name, i);
            generate_stub_bean(&class_name, &dw.expression, &dw.context_hint)
        })
        .collect()
}

fn generate_stub_bean(class_name: &str, dw_expression: &str, context: &str) -> GeneratedBean {
    let dw_comment = dw_expression
        .lines()
        .map(|l| format!(" * {}", l))
        .collect::<Vec<_>>()
        .join("\n");

    let java = format!(
        r#"import jakarta.enterprise.context.ApplicationScoped;
import org.apache.camel.Exchange;
import org.apache.camel.Processor;

/**
 * TODO: Manual DataWeave conversion required.
 *
 * Context: {context}
 *
 * Original DataWeave:
{dw_comment}
 */
@ApplicationScoped
public class {class_name} implements Processor {{

    @Override
    public void process(Exchange exchange) throws Exception {{
        // TODO: Implement the DataWeave logic below in Java.
        //
        // The original DataWeave expression is preserved in the Javadoc above.
        // Key steps:
        // 1. Read input from exchange.getIn().getBody()
        // 2. Apply the transformation logic
        // 3. Set the result via exchange.getIn().setBody()
        throw new UnsupportedOperationException(
            "{class_name}: DataWeave conversion not yet implemented — see Javadoc for original expression"
        );
    }}
}}
"#,
        context = context,
        dw_comment = dw_comment,
        class_name = class_name
    );

    GeneratedBean {
        class_name: class_name.to_string(),
        package: "generated.beans".into(),
        java_source: java,
        origin_dw: Some(dw_expression.to_string()),
    }
}

fn sanitize_class_name(flow_name: &str, index: usize) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in flow_name.chars() {
        if c == '-' || c == '_' || c == ' ' || c == '.' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap_or(c));
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    if index > 0 {
        format!("{}Transform{}", result, index)
    } else {
        format!("{}Transform", result)
    }
}
