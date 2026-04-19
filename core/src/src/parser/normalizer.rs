//! Normalizer: resolves property placeholders, inlines sub-flows where
//! unambiguous, and resolves connector config references.

use std::collections::HashMap;

use crate::ast::mule_ast::{MuleElement, MuleProject};
use crate::Result;

pub fn normalize(mut project: MuleProject) -> Result<MuleProject> {
    // Phase 1: resolve ${...} property placeholders in attributes.
    for flow in &mut project.flows {
        for proc in &mut flow.processors {
            resolve_placeholders(proc, &project.properties);
        }
        for eh in &mut flow.error_handlers {
            resolve_placeholders(eh, &project.properties);
        }
    }
    for cfg in &mut project.configs {
        resolve_placeholders(cfg, &project.properties);
    }

    // Phase 2: resolve config-ref attributes by linking to the config element.
    let config_map: HashMap<String, MuleElement> = project
        .configs
        .iter()
        .filter_map(|c| c.attr("name").map(|n| (n.to_string(), c.clone())))
        .collect();

    for flow in &mut project.flows {
        for proc in &mut flow.processors {
            resolve_config_refs(proc, &config_map);
        }
    }

    // Phase 3: inline sub-flows referenced only once via flow-ref.
    let sub_flow_map: HashMap<String, Vec<MuleElement>> = project
        .flows
        .iter()
        .filter(|f| f.is_sub_flow)
        .map(|f| (f.name.clone(), f.processors.clone()))
        .collect();

    let mut ref_counts: HashMap<String, usize> = HashMap::new();
    for flow in &project.flows {
        count_flow_refs(&flow.processors, &mut ref_counts);
    }

    for flow in &mut project.flows {
        if !flow.is_sub_flow {
            inline_sub_flows(&mut flow.processors, &sub_flow_map, &ref_counts);
        }
    }

    project.flows.retain(|f| {
        if f.is_sub_flow {
            ref_counts.get(&f.name).copied().unwrap_or(0) != 1
        } else {
            true
        }
    });

    Ok(project)
}

fn resolve_placeholders(elem: &mut MuleElement, props: &HashMap<String, String>) {
    for val in elem.attributes.values_mut() {
        *val = substitute_props(val, props);
    }
    for child in &mut elem.children {
        resolve_placeholders(child, props);
    }
}

fn substitute_props(input: &str, props: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut key = String::new();
            for ch in chars.by_ref() {
                if ch == '}' {
                    break;
                }
                key.push(ch);
            }
            let (lookup_key, default) = if let Some(idx) = key.find(":-") {
                (&key[..idx], Some(&key[idx + 2..]))
            } else {
                (key.as_str(), None)
            };
            if let Some(val) = props.get(lookup_key) {
                out.push_str(val);
            } else if let Some(d) = default {
                out.push_str(d);
            } else {
                out.push_str(&format!("${{{}}}", key));
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn resolve_config_refs(elem: &mut MuleElement, configs: &HashMap<String, MuleElement>) {
    if let Some(ref_name) = elem.attr("config-ref").map(|s| s.to_string()) {
        if let Some(config) = configs.get(&ref_name) {
            elem.resolved_config = Some(Box::new(config.clone()));
        }
    }
    for child in &mut elem.children {
        resolve_config_refs(child, configs);
    }
}

fn count_flow_refs(processors: &[MuleElement], counts: &mut HashMap<String, usize>) {
    for proc in processors {
        if proc.name == "flow-ref" {
            if let Some(name) = proc.attr("name") {
                *counts.entry(name.to_string()).or_insert(0) += 1;
            }
        }
        count_flow_refs(&proc.children, counts);
    }
}

fn inline_sub_flows(
    processors: &mut Vec<MuleElement>,
    sub_flows: &HashMap<String, Vec<MuleElement>>,
    ref_counts: &HashMap<String, usize>,
) {
    let mut i = 0;
    while i < processors.len() {
        if processors[i].name == "flow-ref" {
            if let Some(name) = processors[i].attr("name").map(|s| s.to_string()) {
                if ref_counts.get(&name).copied() == Some(1) {
                    if let Some(body) = sub_flows.get(&name) {
                        processors.splice(i..=i, body.iter().cloned());
                        continue;
                    }
                }
            }
        }
        inline_sub_flows(&mut processors[i].children, sub_flows, ref_counts);
        i += 1;
    }
}
