//! Where an application sets a variable, and whether the generated routes
//! leave the same value in the exchange property of that name.
//!
//! A choice condition `vars.x == literal` is emitted as a predicate on the
//! Java value of the exchange property `x` (see
//! `Expression::PropertyEquals`). The predicate is exact for that value;
//! it gives Mule's result only if the property holds what the Mule variable
//! holds. That is known in one case: the variable is set by a
//! `<set-variable>` with a literal value (text, or a string, integer or
//! boolean literal in `#[...]`), which is emitted as `setProperty` with a
//! constant of the same type, at a place the generated route runs like Mule
//! does. Everything else that can set the variable is a reason to review
//! the comparison:
//!
//! * a `set-variable` with any other value: `#[payload]` leaves the body
//!   as it arrived in the property, where Mule compares the value it reads
//!   from it; `#[vars.y]` is only as good as `y`; an unconverted value is a
//!   step that throws;
//! * a `set-variable` with a `mimeType` (ignored by the migration, it
//!   changes how Mule reads the value) or whose `variableName` is not a
//!   plain name (an unresolved `${...}` placeholder may be any name);
//! * anything else that names the variable as its target: `ee:set-variable`
//!   in an `ee:transform`, `target` / `targetVariable` of a connector
//!   operation or a scope, the counter of a `foreach`, a Mule 3 enricher.
//!   None of them is migrated as a step that sets the property;
//! * `remove-variable`, which is not migrated: the property keeps its value;
//! * a `set-variable` inside a scope that is not migrated as steps of the
//!   same exchange. Only a flow, a sub-flow, the branches of a `choice` and
//!   an on-error scope for every error are. A `foreach` is a Camel `split`,
//!   whose steps work on a copy of the exchange; the steps of any other
//!   scope (`try`, `async`, `until-successful`, ...) are not emitted; an
//!   on-error scope with a `type` or a `when` is emitted as an
//!   approximation or commented out; a global error handler is not migrated;
//! * a `flow-ref` to something that is not among the flows of the
//!   application: what it sets is unknown.
//!
//! The scan covers the whole application, because a variable set in one
//! flow is visible in every flow and sub-flow the same event passes
//! through.

use crate::ast::camel_ir::Expression;
use crate::ast::mule_ast::{MuleElement, MuleFlow};

/// One place of the application that sets (or removes) a variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Assignment {
    /// Whether it is a `set-variable` with a literal value at a place the
    /// generated route runs as Mule does: the one kind of assignment after
    /// which the exchange property is known to hold Mule's value.
    pub literal: bool,
    /// What and where, for the review note: `set-variable in flow orders
    /// from #[payload]`.
    pub place: String,
}

/// Every place in `flows` and `globals` that sets variable `name`, or may
/// set it, in document order.
pub(super) fn assignments(
    flows: &[MuleFlow],
    globals: &[MuleElement],
    name: &str,
) -> Vec<Assignment> {
    let mut scan = Scan {
        name,
        flows,
        found: Vec::new(),
    };
    for flow in flows {
        let kind = if flow.is_sub_flow { "sub-flow" } else { "flow" };
        let place = format!("{kind} {}", flow.name);
        for element in flow.processors.iter().chain(&flow.error_handlers) {
            scan.element(element, &place, None);
        }
    }
    for global in globals {
        let place = match global.attr("name") {
            Some(name) => format!("global {} {name}", global.qualified_name()),
            None => format!("global {}", global.qualified_name()),
        };
        // Nothing of a global element is migrated.
        scan.element(global, &place, Some(&global.qualified_name()));
    }
    scan.found
}

struct Scan<'a> {
    name: &'a str,
    flows: &'a [MuleFlow],
    found: Vec<Assignment>,
}

impl Scan<'_> {
    /// Record what `element` does to the variable and go on with its
    /// children. `place` is the flow it is in; `scope` is the innermost
    /// enclosing scope whose steps are not migrated as steps of the same
    /// exchange, if there is one.
    fn element(&mut self, element: &MuleElement, place: &str, scope: Option<&str>) {
        if let Some((literal, what, detail)) = self.sets(element, scope) {
            let inside = match scope {
                Some(scope) => format!(" inside {scope}"),
                None => String::new(),
            };
            self.found.push(Assignment {
                literal,
                place: format!("{what} in {place}{inside}{detail}"),
            });
        }
        let qualified_name = element.qualified_name();
        let scope =
            scope.or_else(|| (!keeps_the_exchange(element)).then_some(qualified_name.as_str()));
        for child in &element.children {
            self.element(child, place, scope);
        }
    }

    /// Whether `element` sets the variable: `(is it a migrated literal,
    /// what it is, what it sets the variable from)`.
    fn sets(&self, element: &MuleElement, scope: Option<&str>) -> Option<(bool, String, String)> {
        let found = |what: String| Some((false, what, String::new()));
        let name = self.name;
        let core = element.namespace.is_empty();
        let qualified_name = element.qualified_name();

        if core && element.name == "set-variable" && element.attr("variableName") == Some(name) {
            let value = element.attr("value").unwrap_or("");
            let literal = matches!(
                crate::mel::convert_value(value),
                Expression::Constant(_) | Expression::Literal(_)
            );
            let (literal, detail) = if !literal {
                (false, format!(" from {value}"))
            } else if let Some(mime_type) = element.attr("mimeType") {
                (false, format!(" with mimeType=\"{mime_type}\""))
            } else {
                (scope.is_none(), String::new())
            };
            return Some((literal, "set-variable".to_string(), detail));
        }
        if core && element.name == "remove-variable" && element.attr("variableName") == Some(name) {
            return found("remove-variable".to_string());
        }

        // Any other attribute that names a variable to write to. Sorted,
        // so that the description does not depend on the order of a map.
        let mut attributes: Vec<(&String, &String)> = element.attributes.iter().collect();
        attributes.sort();
        for (key, value) in attributes {
            if names_a_variable(key) && (mentions(value, name) || value.contains("${")) {
                return found(format!("{qualified_name} {key}=\"{value}\""));
            }
        }

        // The variables a foreach sets without being told a name.
        if core && element.name == "foreach" {
            for (variable, attribute) in [
                ("counter", "counterVariableName"),
                ("rootMessage", "rootMessageVariableName"),
            ] {
                if name == variable && element.attr(attribute).is_none() {
                    return found("foreach".to_string());
                }
            }
        }

        // A flow that is not part of what is migrated may set anything.
        if core && element.name == "flow-ref" {
            let target = element.attr("name").unwrap_or("");
            if !self.flows.iter().any(|flow| flow.name == target) {
                return found(format!("flow-ref to {target}"));
            }
        }
        None
    }
}

/// The variables the steps inside `scope` set or remove (`set-variable`,
/// `remove-variable`, a `target`, ...), by name, each once, in document
/// order. `scope` itself is not looked at.
pub(super) fn set_inside(scope: &MuleElement) -> Vec<String> {
    fn collect(element: &MuleElement, names: &mut Vec<String>) {
        let mut attributes: Vec<(&String, &String)> = element.attributes.iter().collect();
        attributes.sort();
        for (key, value) in attributes {
            if names_a_variable(key) && !names.contains(value) {
                names.push(value.clone());
            }
        }
        for child in &element.children {
            collect(child, names);
        }
    }
    let mut names = Vec::new();
    for child in &scope.children {
        collect(child, &mut names);
    }
    names
}

/// Whether a step inside `scope` reads variable `name` in an attribute or
/// in its text: `vars.name`, `vars['name']`, `flowVars.name`.
pub(super) fn read_inside(scope: &MuleElement, name: &str) -> bool {
    fn reads(value: &str, name: &str) -> bool {
        const SELECTORS: [&str; 6] = ["vars.", "Vars.", "vars['", "Vars['", "vars[\"", "Vars[\""];
        let is_identifier_char = |c: char| c.is_alphanumeric() || c == '_';
        !name.is_empty()
            && value.match_indices(name).any(|(start, _)| {
                let after = value[start + name.len()..].chars().next();
                SELECTORS
                    .iter()
                    .any(|selector| value[..start].ends_with(selector))
                    && !after.is_some_and(is_identifier_char)
            })
    }
    fn element_reads(element: &MuleElement, name: &str) -> bool {
        element.attributes.values().any(|value| reads(value, name))
            || element
                .text
                .as_deref()
                .is_some_and(|text| reads(text, name))
            || element
                .children
                .iter()
                .any(|child| element_reads(child, name))
    }
    scope
        .children
        .iter()
        .any(|child| element_reads(child, name))
}

/// The flows the steps inside `scope` call with a `flow-ref`, by name, each
/// once, in document order.
pub(super) fn flow_refs_inside(scope: &MuleElement) -> Vec<String> {
    fn collect(element: &MuleElement, names: &mut Vec<String>) {
        if element.namespace.is_empty() && element.name == "flow-ref" {
            let name = element.attr("name").unwrap_or("").to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for child in &element.children {
            collect(child, names);
        }
    }
    let mut names = Vec::new();
    for child in &scope.children {
        collect(child, &mut names);
    }
    names
}

/// Whether the children of `element` are emitted as steps that run on the
/// exchange of the route, where Mule runs them on the event of the flow.
fn keeps_the_exchange(element: &MuleElement) -> bool {
    if !element.namespace.is_empty() {
        return false;
    }
    match element.name.as_str() {
        "choice" | "when" | "otherwise" => true,
        // A reference to a global handler has no children here.
        "error-handler" => true,
        // Exact only for a scope that takes every error; see
        // `map_error_handler`.
        "on-error-continue" | "on-error-propagate" => {
            let types: Vec<&str> = element
                .attr("type")
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect();
            let every_error =
                types.is_empty() || types.iter().any(|t| matches!(*t, "ANY" | "MULE:ANY"));
            let conditional = element.attr("when").is_some_and(|w| !w.trim().is_empty());
            every_error && !conditional
        }
        _ => false,
    }
}

/// Whether an attribute with this name says which variable an element
/// writes to: `target`, `targetVariable`, `variableName`,
/// `counterVariableName`, ...
fn names_a_variable(attribute: &str) -> bool {
    let attribute = attribute.to_ascii_lowercase();
    attribute == "target" || attribute.contains("variable")
}

/// Whether `value` contains `name` as a whole identifier: `kind`, or
/// `#[flowVars.kind]`, but not `kinds`.
fn mentions(value: &str, name: &str) -> bool {
    let is_identifier_char = |c: char| c.is_alphanumeric() || c == '_';
    value.match_indices(name).any(|(start, _)| {
        let before = value[..start].chars().next_back();
        let after = value[start + name.len()..].chars().next();
        !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elem(
        ns: &str,
        name: &str,
        attrs: &[(&str, &str)],
        children: Vec<MuleElement>,
    ) -> MuleElement {
        MuleElement {
            namespace: ns.into(),
            name: name.into(),
            attributes: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            children,
            text: None,
            line: None,
            resolved_config: None,
        }
    }

    fn set(name: &str, value: &str) -> MuleElement {
        elem(
            "",
            "set-variable",
            &[("variableName", name), ("value", value)],
            vec![],
        )
    }

    fn flow(name: &str, is_sub_flow: bool, processors: Vec<MuleElement>) -> MuleFlow {
        MuleFlow {
            name: name.into(),
            is_sub_flow,
            source_file: "flows.xml".into(),
            processors,
            error_handlers: vec![],
        }
    }

    /// `(literal, place)` of every assignment to `x`.
    fn found(flows: &[MuleFlow], globals: &[MuleElement]) -> Vec<(bool, String)> {
        assignments(flows, globals, "x")
            .into_iter()
            .map(|a| (a.literal, a.place))
            .collect()
    }

    #[test]
    fn literal_set_variables_are_found_in_flows_sub_flows_branches_and_error_handlers() {
        let mut main = flow(
            "main",
            false,
            vec![
                set("x", "text"),
                set("y", "#[payload]"),
                elem(
                    "",
                    "choice",
                    &[],
                    vec![
                        elem("", "when", &[], vec![set("x", "#[1]")]),
                        elem("", "otherwise", &[], vec![set("x", "#[true]")]),
                    ],
                ),
            ],
        );
        main.error_handlers = vec![elem(
            "",
            "error-handler",
            &[],
            vec![
                elem(
                    "",
                    "on-error-continue",
                    &[("type", "ANY")],
                    vec![set("x", "a")],
                ),
                elem("", "on-error-propagate", &[], vec![set("x", "b")]),
            ],
        )];
        let shared = flow("shared", true, vec![set("x", "#['c']")]);

        assert_eq!(
            found(&[main, shared], &[]),
            [
                (true, "set-variable in flow main".to_string()),
                (true, "set-variable in flow main".to_string()),
                (true, "set-variable in flow main".to_string()),
                (true, "set-variable in flow main".to_string()),
                (true, "set-variable in flow main".to_string()),
                (true, "set-variable in sub-flow shared".to_string()),
            ]
        );
    }

    #[test]
    fn nothing_is_found_for_a_variable_nobody_sets() {
        let flows = [flow(
            "main",
            false,
            vec![
                set("xs", "a"),
                set("y", "#[vars.x]"),
                elem("", "logger", &[("message", "#[vars.x]")], vec![]),
                elem("http", "request", &[("target", "xy")], vec![]),
            ],
        )];
        assert_eq!(found(&flows, &[]), []);
    }

    #[test]
    fn a_value_that_is_not_a_literal_is_named() {
        for value in [
            "#[payload]",
            "#[vars.y]",
            "#[payload.id]",
            "#[now()]",
            "a #[1]",
        ] {
            let flows = [flow("main", false, vec![set("x", value)])];
            assert_eq!(
                found(&flows, &[]),
                [(false, format!("set-variable in flow main from {value}"))]
            );
        }
    }

    #[test]
    fn a_literal_inside_a_scope_that_is_not_migrated_as_steps_is_not_a_literal() {
        for (ns, name, described) in [
            ("", "foreach", "foreach"),
            ("", "try", "try"),
            ("", "async", "async"),
            ("", "until-successful", "until-successful"),
            ("batch", "job", "batch:job"),
        ] {
            let flows = [flow(
                "main",
                false,
                vec![elem(ns, name, &[], vec![set("x", "a")])],
            )];
            assert_eq!(
                found(&flows, &[]),
                [(
                    false,
                    format!("set-variable in flow main inside {described}")
                )],
            );
        }

        // The outermost such scope is the one that is named, also when a
        // choice sits in between.
        let nested = elem(
            "",
            "try",
            &[],
            vec![elem(
                "",
                "choice",
                &[],
                vec![elem("", "when", &[], vec![set("x", "a")])],
            )],
        );
        assert_eq!(
            found(&[flow("main", false, vec![nested])], &[]),
            [(false, "set-variable in flow main inside try".to_string())]
        );
    }

    #[test]
    fn an_on_error_scope_for_some_errors_only_is_not_migrated_exactly() {
        for attrs in [
            vec![("type", "HTTP:TIMEOUT")],
            vec![("type", "HTTP:TIMEOUT, DB:CONNECTIVITY")],
            vec![("when", "#[true]")],
        ] {
            let mut main = flow("main", false, vec![]);
            main.error_handlers = vec![elem(
                "",
                "error-handler",
                &[],
                vec![elem("", "on-error-continue", &attrs, vec![set("x", "a")])],
            )];
            assert_eq!(
                found(&[main], &[]),
                [(
                    false,
                    "set-variable in flow main inside on-error-continue".to_string()
                )]
            );
        }
    }

    #[test]
    fn everything_else_that_names_the_variable_as_its_target_is_found() {
        let cases = [
            (
                elem(
                    "ee",
                    "transform",
                    &[],
                    vec![elem(
                        "ee",
                        "variables",
                        &[],
                        vec![elem("ee", "set-variable", &[("variableName", "x")], vec![])],
                    )],
                ),
                "ee:set-variable variableName=\"x\" in flow main inside ee:transform",
            ),
            (
                elem("http", "request", &[("target", "x")], vec![]),
                "http:request target=\"x\" in flow main",
            ),
            (
                elem("", "flow-ref", &[("name", "main"), ("target", "x")], vec![]),
                "flow-ref target=\"x\" in flow main",
            ),
            (
                elem("x", "op", &[("targetVariable", "x")], vec![]),
                "x:op targetVariable=\"x\" in flow main",
            ),
            (
                elem("", "enricher", &[("target", "#[flowVars.x]")], vec![]),
                "enricher target=\"#[flowVars.x]\" in flow main",
            ),
            (
                elem("", "foreach", &[("counterVariableName", "x")], vec![]),
                "foreach counterVariableName=\"x\" in flow main",
            ),
            (
                elem("", "remove-variable", &[("variableName", "x")], vec![]),
                "remove-variable in flow main",
            ),
            (
                elem(
                    "",
                    "set-variable",
                    &[
                        ("variableName", "x"),
                        ("value", "3"),
                        ("mimeType", "application/json"),
                    ],
                    vec![],
                ),
                "set-variable in flow main with mimeType=\"application/json\"",
            ),
            (
                elem(
                    "",
                    "set-variable",
                    &[("variableName", "${name}"), ("value", "3")],
                    vec![],
                ),
                "set-variable variableName=\"${name}\" in flow main",
            ),
            (
                elem("", "flow-ref", &[("name", "elsewhere")], vec![]),
                "flow-ref to elsewhere in flow main",
            ),
        ];
        for (element, place) in cases {
            assert_eq!(
                found(&[flow("main", false, vec![element])], &[]),
                [(false, place.to_string())]
            );
        }
    }

    #[test]
    fn a_foreach_sets_counter_and_root_message_unless_they_are_renamed() {
        let plain = [flow("main", false, vec![elem("", "foreach", &[], vec![])])];
        let renamed = [flow(
            "main",
            false,
            vec![elem(
                "",
                "foreach",
                &[
                    ("counterVariableName", "i"),
                    ("rootMessageVariableName", "root"),
                ],
                vec![],
            )],
        )];
        for name in ["counter", "rootMessage"] {
            assert_eq!(
                assignments(&plain, &[], name),
                [Assignment {
                    literal: false,
                    place: "foreach in flow main".into()
                }],
                "{name}"
            );
            assert_eq!(assignments(&renamed, &[], name), [], "{name}");
        }
        assert_eq!(assignments(&plain, &[], "x"), []);
    }

    /// A flow-ref to a flow or sub-flow of the application is not an
    /// assignment: what that flow sets is found in the flow itself.
    #[test]
    fn a_flow_ref_to_a_flow_of_the_application_is_not_an_assignment() {
        let flows = [
            flow(
                "main",
                false,
                vec![elem("", "flow-ref", &[("name", "shared")], vec![])],
            ),
            flow("shared", true, vec![set("x", "a")]),
        ];
        assert_eq!(
            found(&flows, &[]),
            [(true, "set-variable in sub-flow shared".to_string())]
        );
    }

    /// A global error handler is not migrated: whatever it sets, the
    /// generated routes do not.
    #[test]
    fn nothing_in_a_global_element_is_a_migrated_literal() {
        let handler = elem(
            "",
            "error-handler",
            &[("name", "globalHandler")],
            vec![elem(
                "",
                "on-error-continue",
                &[],
                vec![set("x", "a"), set("x", "#[payload]")],
            )],
        );
        assert_eq!(
            found(&[], &[handler]),
            [
                (
                    false,
                    "set-variable in global error-handler globalHandler inside error-handler"
                        .to_string()
                ),
                (
                    false,
                    "set-variable in global error-handler globalHandler inside error-handler \
                     from #[payload]"
                        .to_string()
                ),
            ]
        );
    }
}
