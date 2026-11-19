//! Per-flow documentation pages.

use crate::ast::camel_ir::{CamelProject, Expression, RouteEndpoint, RouteStep};
use crate::report::MigrationReport;
use crate::Result;
use std::path::Path;

pub fn generate(ir: &CamelProject, report: &MigrationReport, docs_dir: &Path) -> Result<()> {
    let flows_dir = docs_dir.join("flows");
    std::fs::create_dir_all(&flows_dir)?;

    for route in &ir.routes {
        let mut md = String::new();
        md.push_str(&format!("# {}\n\n", route.id));

        let source = match &route.source {
            RouteEndpoint::Uri(u) => u.clone(),
            RouteEndpoint::Direct(d) => format!("direct:{}", d),
        };
        md.push_str(&format!("**Source endpoint:** `{}`\n\n", source));
        md.push_str(&format!("**Steps:** {}\n\n", route.steps.len()));

        if !route.on_exceptions.is_empty() {
            md.push_str(&format!(
                "**Exception handlers:** {}\n\n",
                route.on_exceptions.len()
            ));
        }

        md.push_str("## Processing Steps\n\n");
        for (i, step) in route.steps.iter().enumerate() {
            md.push_str(&format!("{}. {}\n", i + 1, describe_step(step)));
        }

        // Link related migration decisions
        let related: Vec<_> = report
            .decisions
            .iter()
            .filter(|d| d.flow_name.as_deref() == Some(&route.id))
            .collect();
        if !related.is_empty() {
            md.push_str("\n## Migration Decisions\n\n");
            for d in related {
                md.push_str(&format!(
                    "- `{}` — {:?}: {}\n",
                    d.mule_element, d.status, d.rationale
                ));
            }
        }

        let filename = route.id.replace(' ', "-").to_lowercase();
        std::fs::write(flows_dir.join(format!("{}.md", filename)), md)?;
    }

    Ok(())
}

/// One line about a step of the route, for the page of its flow.
///
/// A step whose value is a Mule expression that is not converted is
/// emitted as a step that throws, and a choice condition that is not
/// converted as a predicate that throws. The page says that, instead of
/// describing the step the route does not have.
fn describe_step(step: &RouteStep) -> String {
    if step.is_unconverted() {
        let what = match step {
            RouteStep::SetBody { .. } => "set message body".to_string(),
            RouteStep::SetHeader { name, .. } => format!("set header `{name}`"),
            RouteStep::SetProperty { name, .. } => format!("set property `{name}`"),
            _ => "split".to_string(),
        };
        return format!(
            "**Not migrated:** {what} - its value is a Mule expression that is not converted, \
             the route fails at this step (see the TODO in the route class)"
        );
    }
    match step {
        RouteStep::SetBody { .. } => "Set message body".into(),
        RouteStep::SetHeader { name, .. } => format!("Set header `{}`", name),
        RouteStep::SetProperty { name, .. } => format!("Set property `{}`", name),
        RouteStep::RemoveProperty { name } => format!("Remove property `{}`", name),
        RouteStep::Log { message, .. } => {
            format!("Log: {}", message.chars().take(60).collect::<String>())
        }
        RouteStep::ToUri(uri) => format!("Send to `{}`", uri),
        RouteStep::ProcessBean(b) => format!("Process bean `{}`", b),
        RouteStep::Choice { whens, .. } => {
            let count =
                |wanted: fn(&Expression) -> bool| whens.iter().filter(|(c, _)| wanted(c)).count();
            let unconverted = count(|c| matches!(c, Expression::Unconverted { .. }));
            let to_review = count(
                |c| matches!(c, Expression::PropertyEquals { review, .. } if !review.is_empty()),
            );
            let mut notes = String::new();
            if unconverted > 0 {
                notes.push_str(&format!(
                    "; **not migrated: {unconverted} of {} conditions** - a message that \
                     reaches one of them fails there",
                    whens.len()
                ));
            }
            if to_review > 0 {
                notes.push_str(&format!(
                    "; to review: {to_review} of {} conditions, converted on an assumption",
                    whens.len()
                ));
            }
            if !notes.is_empty() {
                notes.push_str("; see the TODO in the route class");
            }
            format!("Choice router ({} branches{notes})", whens.len())
        }
        RouteStep::Split { parallel, .. } => {
            if *parallel {
                "Parallel split".into()
            } else {
                "Split".into()
            }
        }
        RouteStep::Marshal(fmt) => format!("Marshal ({:?})", fmt),
        RouteStep::Unmarshal(fmt) => format!("Unmarshal ({:?})", fmt),
        RouteStep::Transform { bean_ref } => format!("Transform via `{}`", bean_ref),
        RouteStep::ThrowException { class, .. } => format!("Throw `{}`", class),
        RouteStep::RawDsl(dsl) => format!("Custom: {}", dsl.chars().take(60).collect::<String>()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::camel_ir::{ComparedValue, Expression};

    fn unconverted() -> Expression {
        Expression::Unconverted {
            original: "#[now()]".into(),
            reason: "why".into(),
        }
    }

    fn constant() -> Expression {
        Expression::Constant("text".into())
    }

    /// A step whose value is not converted is emitted as a step that
    /// throws. The page must not say that it sets something.
    #[test]
    fn step_with_an_unconverted_value_is_described_as_not_migrated() {
        let fails = "its value is a Mule expression that is not converted, the route fails at \
                     this step (see the TODO in the route class)";
        for (step, what) in [
            (
                RouteStep::SetBody {
                    expression: unconverted(),
                },
                "set message body".to_string(),
            ),
            (
                RouteStep::SetHeader {
                    name: "x-id".into(),
                    expression: unconverted(),
                },
                "set header `x-id`".to_string(),
            ),
            (
                RouteStep::SetProperty {
                    name: "id".into(),
                    expression: unconverted(),
                },
                "set property `id`".to_string(),
            ),
            (
                RouteStep::Split {
                    expression: unconverted(),
                    parallel: false,
                    aggregation_strategy: None,
                    steps: vec![],
                },
                "split".to_string(),
            ),
        ] {
            assert_eq!(
                describe_step(&step),
                format!("**Not migrated:** {what} - {fails}")
            );
        }
    }

    #[test]
    fn step_with_a_converted_value_is_described_as_before() {
        assert_eq!(
            describe_step(&RouteStep::SetBody {
                expression: constant()
            }),
            "Set message body"
        );
        assert_eq!(
            describe_step(&RouteStep::SetProperty {
                name: "id".into(),
                expression: constant()
            }),
            "Set property `id`"
        );
        assert_eq!(
            describe_step(&RouteStep::SetHeader {
                name: "x-id".into(),
                expression: constant()
            }),
            "Set header `x-id`"
        );
    }

    /// A choice says how many of its conditions fail instead of deciding,
    /// and how many are converted on an assumption.
    #[test]
    fn choice_says_which_of_its_conditions_are_not_migrated_or_to_review() {
        let compared = |review: Vec<String>| Expression::PropertyEquals {
            original: "#[vars.a == 'x']".into(),
            name: "a".into(),
            value: ComparedValue::Text("x".into()),
            negated: false,
            review,
        };
        let choice = |conditions: Vec<Expression>| RouteStep::Choice {
            whens: conditions.into_iter().map(|c| (c, vec![])).collect(),
            otherwise: None,
        };

        assert_eq!(
            describe_step(&choice(vec![compared(vec![]), compared(vec![])])),
            "Choice router (2 branches)"
        );
        assert_eq!(
            describe_step(&choice(vec![
                compared(vec![]),
                unconverted(),
                unconverted()
            ])),
            "Choice router (3 branches; **not migrated: 2 of 3 conditions** - a message that \
             reaches one of them fails there; see the TODO in the route class)"
        );
        assert_eq!(
            describe_step(&choice(vec![compared(vec!["check".into()])])),
            "Choice router (1 branches; to review: 1 of 1 conditions, converted on an \
             assumption; see the TODO in the route class)"
        );
        assert_eq!(
            describe_step(&choice(vec![compared(vec!["check".into()]), unconverted()])),
            "Choice router (2 branches; **not migrated: 1 of 2 conditions** - a message that \
             reaches one of them fails there; to review: 1 of 2 conditions, converted on an \
             assumption; see the TODO in the route class)"
        );
    }
}
