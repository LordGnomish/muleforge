//! Routes emitter: generates one Java RouteBuilder per Camel route.

use std::path::Path;

use crate::ast::camel_ir::*;
use crate::emitter::{JAVA_RELEASE, QUARKUS_PLATFORM_VERSION};
use crate::Result;

pub fn emit(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    let routes_dir = output_dir.join("src/main/java/generated/routes");
    std::fs::create_dir_all(&routes_dir)?;

    for route in &ir.routes {
        let class_name = to_class_name(&route.id);
        let java = generate_route_builder(&class_name, route);
        let file_path = routes_dir.join(format!("{}.java", class_name));
        std::fs::write(&file_path, java)?;
    }

    // Generate pom.xml
    let pom = generate_pom(ir);
    std::fs::write(output_dir.join("pom.xml"), pom)?;

    // Generate Dockerfile
    let dockerfile = generate_dockerfile(&ir.name);
    std::fs::write(output_dir.join("Dockerfile"), dockerfile)?;

    // Generate .gitignore
    std::fs::write(output_dir.join(".gitignore"), GITIGNORE)?;

    Ok(())
}

fn to_class_name(flow_name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in flow_name.chars() {
        if c == '-' || c == '_' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap_or(c));
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    if !result.ends_with("Route") {
        result.push_str("Route");
    }
    result
}

fn generate_route_builder(class_name: &str, route: &CamelRoute) -> String {
    let mut java = String::new();
    java.push_str("package generated.routes;\n\n");
    java.push_str("import org.apache.camel.builder.RouteBuilder;\n");
    java.push_str("import org.apache.camel.LoggingLevel;\n");
    java.push_str("import jakarta.enterprise.context.ApplicationScoped;\n\n");
    java.push_str("@ApplicationScoped\n");
    java.push_str(&format!(
        "public class {} extends RouteBuilder {{\n\n",
        class_name
    ));
    java.push_str("    @Override\n");
    java.push_str("    public void configure() throws Exception {\n");

    // Exception handlers
    for eh in &route.on_exceptions {
        java.push_str(&on_exception_clause(eh));
        java.push('\n');
    }

    // Route definition
    let from_uri = match &route.source {
        RouteEndpoint::Uri(uri) => format!("\"{}\"", uri),
        RouteEndpoint::Direct(name) => format!("\"direct:{}\"", name),
    };
    java.push_str(&format!("        from({})\n", from_uri));
    java.push_str(&format!("            .routeId(\"{}\")\n", route.id));
    if leaves_failures_to_caller(route) {
        java.push_str("            .errorHandler(noErrorHandler())\n");
    }

    for step in &route.steps {
        emit_step_dsl(&mut java, step, 3);
    }

    java.push_str("            ;\n");
    java.push_str("    }\n");
    if compares_a_number(route) {
        java.push_str(IS_NUMBER_METHOD);
    }
    java.push_str("}\n");
    java
}

/// The method the predicate of a comparison with a number calls (see
/// [`property_predicate`]); written into a route class that has one.
///
/// Mule compares numbers by value, whatever their Java type, and a number
/// never equals a string or a boolean. No conversion is involved: a
/// `BigDecimal` or `BigInteger` is compared as it is, every other `Number`
/// has to give the expected value both as a `double` (no fraction is cut
/// off) and as a `long` (no precision is lost above 2^53).
const IS_NUMBER_METHOD: &str = r#"
    // Whether value is a number equal to expected, as Mule compares numbers:
    // by value, whatever the Java type (Integer 3, Long 3, Double 3.0 and
    // BigDecimal 3.0 all equal 3), and never a String ("3") or a Boolean.
    private static boolean isNumber(Object value, long expected) {
        if (value instanceof java.math.BigDecimal decimal) {
            return decimal.compareTo(java.math.BigDecimal.valueOf(expected)) == 0;
        }
        if (value instanceof java.math.BigInteger integer) {
            return integer.equals(java.math.BigInteger.valueOf(expected));
        }
        return value instanceof Number number
                && number.doubleValue() == expected
                && number.longValue() == expected;
    }
"#;

/// Whether a choice of the route (its exception clauses included) compares
/// a property with a number, so that the class needs [`IS_NUMBER_METHOD`].
fn compares_a_number(route: &CamelRoute) -> bool {
    fn in_steps(steps: &[RouteStep]) -> bool {
        steps.iter().any(|step| match step {
            RouteStep::Choice { whens, otherwise } => {
                whens.iter().any(|(condition, steps)| {
                    matches!(
                        condition,
                        Expression::PropertyEquals {
                            value: ComparedValue::Integer(_),
                            ..
                        }
                    ) || in_steps(steps)
                }) || otherwise.as_deref().is_some_and(in_steps)
            }
            RouteStep::Split { steps, .. } => in_steps(steps),
            _ => false,
        })
    }
    in_steps(&route.steps) || route.on_exceptions.iter().any(|eh| in_steps(&eh.steps))
}

/// Whether the route is written with `.errorHandler(noErrorHandler())`.
///
/// It is for the route of a Mule sub-flow: `RouteEndpoint::Direct` without
/// exception handlers. A sub-flow has no error handling of its own; what
/// fails in it is handled by the error handler of the flow that references
/// it. A Camel route, however, gets the default error handler, and a failure
/// that one has dealt with comes back to the calling route as final: the
/// caller's `onException` clauses do not run for it. Without error handler
/// the failure comes back unhandled and the caller's clauses take it.
/// (`noErrorHandler()` is a method of `RouteBuilder`; no import is needed.)
///
/// Every other route keeps its error handler: a route with exception
/// handlers needs it (`onException` clauses do nothing without one), and a
/// route with a source of its own has no caller to leave a failure to.
fn leaves_failures_to_caller(route: &CamelRoute) -> bool {
    matches!(route.source, RouteEndpoint::Direct(_)) && route.on_exceptions.is_empty()
}

/// One `onException(...)` clause, the Camel form of a Mule on-error scope.
///
/// The clause is declared on the RouteBuilder ahead of `from(...)`. Camel
/// scopes such a clause to the routes of that RouteBuilder instance, and an
/// emitted RouteBuilder holds exactly one route, so no other flow's route
/// uses it. It runs for what fails in that route, and for what fails in a
/// route called from it through `direct:` only if that route has no error
/// handler: the route of a sub-flow (see [`leaves_failures_to_caller`]).
/// That matches the flow's `<error-handler>` in Mule for the flow and the
/// sub-flows it references. It does not for a `<flow>` it references, whose
/// route keeps its own error handler; the mapper flags such a flow-ref.
///
/// * `onException(Class<? extends Throwable>...)` takes class literals:
///   every exception class is written fully qualified with `.class`.
/// * `handled(true)` is Mule's on-error-continue: the failed route stops,
///   the clause's steps run and their result is the reply, as if the route
///   had succeeded. (Not `continued(true)`: that resumes the route after the
///   failed step, which on-error-continue does not do.)
/// * `handled(false)` is Mule's on-error-propagate: the steps run and the
///   caller still gets the failure. It is Camel's default and written out
///   so the generated route says which of the two it is.
///
/// A handler without exception class (the mapper found none for the Mule
/// error type) is emitted commented out: `onException()` compiles but Camel
/// rejects it at startup, and `Exception.class` would handle more errors
/// than the Mule scope did.
fn on_exception_clause(eh: &ExceptionHandler) -> String {
    let classes: Vec<String> = eh
        .exception_classes
        .iter()
        .map(|class| java_class_literal(class))
        .collect();

    let mut clause = String::new();
    if classes.is_empty() {
        clause.push_str("        onException(/* TODO: exception class */)\n");
    } else {
        clause.push_str(&format!("        onException({})\n", classes.join(", ")));
    }
    clause.push_str(&format!("            .handled({})\n", eh.handled));
    for step in &eh.steps {
        emit_step_dsl(&mut clause, step, 3);
    }
    clause.push_str("            .end();\n");

    if classes.is_empty() {
        clause = clause
            .lines()
            .map(|line| match line.strip_prefix("        ") {
                Some(code) => format!("        // {code}\n"),
                None => format!("        // {line}\n"),
            })
            .collect();
    }
    clause
}

/// `java.net.ConnectException` -> `java.net.ConnectException.class`.
fn java_class_literal(class_name: &str) -> String {
    let name = class_name.trim();
    format!("{}.class", name.strip_suffix(".class").unwrap_or(name))
}

fn emit_step_dsl(java: &mut String, step: &RouteStep, indent: usize) {
    let pad = "    ".repeat(indent);

    // A step whose value is a Mule expression that is not converted is not
    // written as that step: the route fails there instead (see
    // `emit_unconverted_step`).
    let needs_value = match step {
        RouteStep::SetBody { expression } => Some(("setBody".to_string(), "value", expression)),
        RouteStep::SetHeader { name, expression } => {
            Some((format!("setHeader(\"{name}\")"), "value", expression))
        }
        RouteStep::SetProperty { name, expression } => {
            Some((format!("setProperty(\"{name}\")"), "value", expression))
        }
        RouteStep::Split { expression, .. } => {
            Some(("split".to_string(), "expression", expression))
        }
        _ => None,
    };
    if let Some((what, part, Expression::Unconverted { original, reason })) = needs_value {
        let mut notes = vec![reason.as_str(), UNCONVERTED_STEP_FAILS];
        if matches!(step, RouteStep::Split { .. }) {
            notes.push("the steps of the split follow it and are not reached");
        }
        emit_unconverted_step(java, &pad, &what, part, original, &notes);
        if let RouteStep::Split { steps, .. } = step {
            for s in steps {
                emit_step_dsl(java, s, indent);
            }
        }
        return;
    }

    match step {
        RouteStep::SetBody { expression } => {
            java.push_str(&format!("{}.setBody({})\n", pad, expr_to_java(expression)));
        }
        RouteStep::SetHeader { name, expression } => {
            java.push_str(&format!(
                "{}.setHeader(\"{}\", {})\n",
                pad,
                name,
                expr_to_java(expression)
            ));
        }
        RouteStep::SetProperty { name, expression } => {
            java.push_str(&format!(
                "{}.setProperty(\"{}\", {})\n",
                pad,
                name,
                expr_to_java(expression)
            ));
        }
        RouteStep::RemoveProperty { name } => {
            java.push_str(&format!("{}.removeProperty(\"{}\")\n", pad, name));
        }
        RouteStep::Log { level, message } => {
            let lvl = match level {
                LogLevel::Trace => "TRACE",
                LogLevel::Debug => "DEBUG",
                LogLevel::Info => "INFO",
                LogLevel::Warn => "WARN",
                LogLevel::Error => "ERROR",
            };
            java.push_str(&format!(
                "{}.log(LoggingLevel.{}, \"{}\")\n",
                pad,
                lvl,
                escape_java(message)
            ));
        }
        RouteStep::ToUri(uri) => {
            java.push_str(&format!("{}.to(\"{}\")\n", pad, uri));
        }
        RouteStep::ProcessBean(bean) => {
            java.push_str(&format!("{}.bean(\"{}\")\n", pad, bean));
        }
        RouteStep::Choice { whens, otherwise } => {
            java.push_str(&format!("{}.choice()\n", pad));
            for (expr, steps) in whens {
                match expr {
                    Expression::Unconverted { original, reason } => push_todo(
                        java,
                        &format!("{pad}    "),
                        &format!("review this condition, it is not converted (Mule: {original})"),
                        &[reason, UNCONVERTED_CONDITION_FAILS],
                    ),
                    Expression::PropertyEquals {
                        original, review, ..
                    } if !review.is_empty() => {
                        let notes: Vec<&str> = review.iter().map(String::as_str).collect();
                        push_todo(
                            java,
                            &format!("{pad}    "),
                            &format!("review this condition (Mule: {original})"),
                            &notes,
                        );
                    }
                    _ => {}
                }
                java.push_str(&format!("{}    .when({})\n", pad, expr_to_java(expr)));
                for s in steps {
                    emit_step_dsl(java, s, indent + 2);
                }
            }
            if let Some(ow) = otherwise {
                java.push_str(&format!("{}    .otherwise()\n", pad));
                for s in ow {
                    emit_step_dsl(java, s, indent + 2);
                }
            }
            java.push_str(&format!("{}.endChoice()\n", pad));
        }
        RouteStep::Split {
            expression,
            parallel,
            steps,
            ..
        } => {
            java.push_str(&format!("{}.split({})", pad, expr_to_java(expression)));
            if *parallel {
                java.push_str(".parallelProcessing()");
            }
            java.push('\n');
            for s in steps {
                emit_step_dsl(java, s, indent + 1);
            }
            java.push_str(&format!("{}.end()\n", pad));
        }
        RouteStep::Marshal(fmt) => {
            java.push_str(&format!("{}.marshal().{}()\n", pad, data_format_java(fmt)));
        }
        RouteStep::Unmarshal(fmt) => {
            java.push_str(&format!(
                "{}.unmarshal().{}()\n",
                pad,
                data_format_java(fmt)
            ));
        }
        RouteStep::Transform { bean_ref } => {
            java.push_str(&format!("{}.bean(\"{}\")\n", pad, bean_ref));
        }
        RouteStep::ThrowException { class, message } => {
            java.push_str(&format!(
                "{}.throwException(new {}(\"{}\"))\n",
                pad,
                class,
                escape_java(message)
            ));
        }
        RouteStep::RawDsl(dsl) => {
            for line in comment_lines(dsl) {
                java.push_str(&format!("{}// {}\n", pad, line));
            }
        }
    }
}

/// Why a step with an unconverted value is written as a step that throws.
const UNCONVERTED_STEP_FAILS: &str = "the route fails at this step instead of going on with a \
     wrong value; replace it with the converted step";
/// Why an unconverted choice condition is written as a predicate that
/// throws.
const UNCONVERTED_CONDITION_FAILS: &str = "a message that reaches this condition fails here \
     instead of taking a branch by guess; replace the predicate";

/// The step `what` (`setBody`, `setProperty("name")`, ...) whose `part`
/// (value, expression) is the Mule expression `original`, which is not
/// converted.
///
/// The step cannot be written: there is nothing right to set. Writing the
/// Mule expression as a string, or leaving the step out, would let the
/// route go on with a wrong or missing value and give a wrong result
/// without a sign of it. So the route says what is missing (`TODO`) and
/// fails when it gets there: `throwException`, which compiles, lets the
/// application start, and is handled like any other failure of the route.
fn emit_unconverted_step(
    java: &mut String,
    pad: &str,
    what: &str,
    part: &str,
    original: &str,
    notes: &[&str],
) {
    push_todo(
        java,
        pad,
        &format!("review this {what}, its {part} is not converted (Mule: {original})"),
        notes,
    );
    java.push_str(&format!(
        "{pad}.throwException(new UnsupportedOperationException(\"{}\"))\n",
        escape_java(&format!(
            "{what} {part} not migrated: {}",
            expression_for_message(original)
        ))
    ));
}

/// How many characters of a Mule expression an exception message quotes.
const MESSAGE_EXPRESSION_LIMIT: usize = 200;

/// The Mule expression `original` as it is quoted in the message of the
/// exception that an unconverted step or condition throws: all of it up to
/// [`MESSAGE_EXPRESSION_LIMIT`] characters, else its beginning (cut between
/// two characters) and a note. A Java string constant holds at most 65535
/// bytes, and a message of that size would not help anybody; the `TODO`
/// in front of the step has the whole expression.
fn expression_for_message(original: &str) -> String {
    match original.char_indices().nth(MESSAGE_EXPRESSION_LIMIT) {
        Some((end, _)) => format!(
            "{}... (truncated, full expression in the TODO above)",
            &original[..end]
        ),
        None => original.to_string(),
    }
}

/// A `// TODO: <title>` comment followed by one `// - <note>` per note.
fn push_todo(java: &mut String, pad: &str, title: &str, notes: &[&str]) {
    for line in comment_lines(&format!("TODO: {title}")) {
        java.push_str(&format!("{pad}// {line}\n"));
    }
    for note in notes {
        for line in comment_lines(&format!("- {note}")) {
            java.push_str(&format!("{pad}// {line}\n"));
        }
    }
}

/// `text` as the lines of a `//` comment, one comment line per text line.
///
/// The text comes from the Mule XML (attribute values quoted in review
/// notes, for example) and must not be able to leave the comment:
///
/// * javac ends a line, and a `//` comment with it, at LF, CR LF and at a
///   lone CR (JLS 3.4). Each of them starts a new comment line.
/// * javac translates Unicode escapes before it recognises comments (JLS
///   3.3): `C:\users` in a comment is an "illegal unicode escape", and
///   `\u000a` would end the comment. A `u` starts an escape when an odd
///   number of backslashes stands before it; such a run gets one more
///   backslash, which makes it a row of plain backslashes for javac.
///
/// Any other text is written as it is.
fn comment_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| {
            let mut out = String::with_capacity(line.len());
            let mut backslashes = 0;
            for c in line.chars() {
                if c == '\\' {
                    backslashes += 1;
                } else {
                    if c == 'u' && backslashes % 2 == 1 {
                        out.push('\\');
                    }
                    backslashes = 0;
                }
                out.push(c);
            }
            out
        })
        .collect()
}

/// `expr` as the Java argument of the step that uses it.
///
/// [`Expression::Unconverted`] comes out as a predicate that throws. That
/// is its form as a choice condition, the one place it is written as an
/// argument: Camel evaluates the conditions of a choice in order, so the
/// branches in front of it behave as in Mule, and a message that gets as
/// far as this condition fails there - neither this branch, nor a later
/// one, nor the otherwise branch is taken on a guess. (A condition that
/// never matches would hand such a message to a later branch without a
/// sign that anything is wrong.) Steps that need a value never get here
/// with it, see `emit_step_dsl`.
fn expr_to_java(expr: &Expression) -> String {
    match expr {
        Expression::Constant(s) => format!("constant(\"{}\")", escape_java(s)),
        Expression::Literal(s) => format!("constant({})", s),
        Expression::Simple(s) => format!("simple(\"{}\")", escape_java(s)),
        Expression::Header(s) => format!("header(\"{}\")", s),
        Expression::Property(s) => format!("exchangeProperty(\"{}\")", s),
        Expression::BeanRef(s) => format!("method(\"{}\")", s),
        Expression::Unconverted { original, .. } => format!(
            "exchange -> {{ throw new UnsupportedOperationException(\"{}\"); }}",
            escape_java(&format!(
                "choice condition not migrated: {}",
                expression_for_message(original)
            ))
        ),
        Expression::PropertyEquals {
            name,
            value,
            negated,
            ..
        } => property_predicate(name, value, *negated),
    }
}

/// The Java predicate for "exchange property `name` equals `value`"
/// (`negated`: does not equal), with Mule's equality: no type is converted.
///
/// It is a lambda on the property's Java value, not a Simple expression:
/// Simple's `==` converts one side to the type of the other, so the string
/// `"true"` would equal `true` and `"3"` would equal 3.
///
/// * a string: `"A".equals(value)` is true only for a `String` with that
///   text (never for null);
/// * a boolean: `Boolean.TRUE.equals(value)` is true only for a `Boolean`;
/// * a number: `isNumber(value, 3)`, see [`IS_NUMBER_METHOD`];
/// * null: the property is not set, or set to null.
///
/// "Does not equal" is the same test with `!` in front (`!= null` for
/// null), so it is the exact opposite for every value.
fn property_predicate(name: &str, value: &ComparedValue, negated: bool) -> String {
    let property = format!("exchange.getProperty(\"{}\")", escape_java(name));
    let not = if negated { "!" } else { "" };
    match value {
        ComparedValue::Text(text) => {
            format!(
                "exchange -> {not}\"{}\".equals({property})",
                escape_java(text)
            )
        }
        ComparedValue::Boolean(value) => {
            let constant = if *value { "TRUE" } else { "FALSE" };
            format!("exchange -> {not}Boolean.{constant}.equals({property})")
        }
        ComparedValue::Integer(number) => {
            format!("exchange -> {not}isNumber({property}, {number})")
        }
        ComparedValue::Null => {
            let operator = if negated { "!=" } else { "==" };
            format!("exchange -> {property} {operator} null")
        }
    }
}

/// `s` as the content of a Java string literal. A line terminator (LF or
/// CR) inside the quotes would end the literal for javac.
fn escape_java(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn data_format_java(fmt: &DataFormat) -> &'static str {
    match fmt {
        DataFormat::Json | DataFormat::JsonJackson => "jackson",
        DataFormat::Xml => "jaxb",
        DataFormat::XmlJackson => "jacksonXml",
        DataFormat::Csv => "csv",
        DataFormat::Avro => "avro",
        DataFormat::Protobuf => "protobuf",
    }
}

/// Extensions every emitted project gets on top of what its routes need
/// (the IR's `maven_dependencies`): `(groupId, artifactId)`, managed by
/// `quarkus-bom`. They back the generated operations docs: Prometheus
/// metrics at `/q/metrics` and OpenTelemetry tracing.
const BASELINE_DEPENDENCIES: &[(&str, &str)] = &[
    ("io.quarkus", "quarkus-micrometer-registry-prometheus"),
    ("io.quarkus", "quarkus-opentelemetry"),
];

/// Test-scoped dependencies, managed by `quarkus-bom`: exactly what the
/// generated tests (`tests_emitter`, `munit_scaffold`) import.
const TEST_DEPENDENCIES: &[(&str, &str)] = &[
    // @QuarkusTest and JUnit Jupiter. Named `quarkus-junit5` before
    // Quarkus 3.31; that artifact is now only a relocation stub.
    ("io.quarkus", "quarkus-junit"),
    // RestAssured and Hamcrest, used by the smoke tests of HTTP routes.
    ("io.rest-assured", "rest-assured"),
];

fn push_dependency(
    pom: &mut String,
    group_id: &str,
    artifact_id: &str,
    version: Option<&str>,
    scope: Option<&str>,
) {
    pom.push_str("        <dependency>\n");
    pom.push_str(&format!("            <groupId>{group_id}</groupId>\n"));
    pom.push_str(&format!(
        "            <artifactId>{artifact_id}</artifactId>\n"
    ));
    if let Some(version) = version {
        pom.push_str(&format!("            <version>{version}</version>\n"));
    }
    if let Some(scope) = scope {
        pom.push_str(&format!("            <scope>{scope}</scope>\n"));
    }
    pom.push_str("        </dependency>\n");
}

/// The project name as a Maven `artifactId`.
///
/// The name is the Mule project's directory name and may contain anything
/// (spaces, for a start), while Maven only reads a pom whose ids match
/// `[A-Za-z0-9_.-]+`. Each run of other characters becomes one `-`.
fn maven_artifact_id(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') {
            id.push(c);
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_matches('-');
    if id.is_empty() {
        "migrated-project".to_string()
    } else {
        id.to_string()
    }
}

/// The `pom.xml` of the emitted project.
///
/// `quarkus-bom`, `quarkus-camel-bom` and `quarkus-maven-plugin` all take
/// their version from the single `quarkus.platform.version` property (see
/// [`QUARKUS_PLATFORM_VERSION`]); every dependency without an explicit
/// version is managed by the two BOMs. The golden files
/// `examples/*/expected-output/pom.xml` pin the exact output.
fn generate_pom(ir: &CamelProject) -> String {
    let mut pom = String::new();
    pom.push_str(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!--
  Generated by MuleForge.
  Regenerate with `muleforge migrate`.
-->
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">

    <modelVersion>4.0.0</modelVersion>
    <groupId>generated</groupId>
"#,
    );
    pom.push_str(&format!(
        "    <artifactId>{}</artifactId>\n",
        maven_artifact_id(&ir.name)
    ));
    pom.push_str(
        r#"    <version>1.0.0-SNAPSHOT</version>
    <packaging>jar</packaging>

    <properties>
"#,
    );
    pom.push_str(&format!(
        "        <maven.compiler.release>{JAVA_RELEASE}</maven.compiler.release>\n"
    ));
    pom.push_str(&format!(
        "        <quarkus.platform.version>{QUARKUS_PLATFORM_VERSION}</quarkus.platform.version>\n"
    ));
    pom.push_str(
        r#"        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    </properties>

    <dependencyManagement>
        <dependencies>
            <dependency>
                <groupId>io.quarkus.platform</groupId>
                <artifactId>quarkus-bom</artifactId>
                <version>${quarkus.platform.version}</version>
                <type>pom</type>
                <scope>import</scope>
            </dependency>
            <dependency>
                <groupId>io.quarkus.platform</groupId>
                <artifactId>quarkus-camel-bom</artifactId>
                <version>${quarkus.platform.version}</version>
                <type>pom</type>
                <scope>import</scope>
            </dependency>
        </dependencies>
    </dependencyManagement>

    <dependencies>
"#,
    );

    // A dependency the IR already asks for is not declared a second time.
    let in_ir = |group_id: &str, artifact_id: &str| {
        ir.maven_dependencies
            .iter()
            .any(|d| d.group_id == group_id && d.artifact_id == artifact_id)
    };
    for dep in &ir.maven_dependencies {
        push_dependency(
            &mut pom,
            &dep.group_id,
            &dep.artifact_id,
            dep.version.as_deref(),
            None,
        );
    }
    for (group_id, artifact_id) in BASELINE_DEPENDENCIES {
        if !in_ir(group_id, artifact_id) {
            push_dependency(&mut pom, group_id, artifact_id, None, None);
        }
    }
    pom.push_str("\n        <!-- Test -->\n");
    for (group_id, artifact_id) in TEST_DEPENDENCIES {
        if !in_ir(group_id, artifact_id) {
            push_dependency(&mut pom, group_id, artifact_id, None, Some("test"));
        }
    }

    pom.push_str(
        r#"    </dependencies>

    <build>
        <plugins>
            <plugin>
                <groupId>io.quarkus.platform</groupId>
                <artifactId>quarkus-maven-plugin</artifactId>
                <version>${quarkus.platform.version}</version>
                <extensions>true</extensions>
                <executions>
                    <execution>
                        <goals>
                            <goal>build</goal>
                            <goal>generate-code</goal>
                            <goal>generate-code-tests</goal>
                        </goals>
                    </execution>
                </executions>
            </plugin>
        </plugins>
    </build>

    <profiles>
        <profile>
            <id>native</id>
            <activation>
                <property>
                    <name>native</name>
                </property>
            </activation>
            <properties>
                <quarkus.native.enabled>true</quarkus.native.enabled>
                <quarkus.package.jar.enabled>false</quarkus.package.jar.enabled>
            </properties>
        </profile>
    </profiles>
</project>
"#,
    );
    pom
}

/// Multi-stage Dockerfile. The JDK of every build and runtime image is the
/// Java release the pom compiles for ([`JAVA_RELEASE`]): an older JDK cannot
/// build it ("release version N not supported").
fn generate_dockerfile(_name: &str) -> String {
    format!(
        r#"# --- JVM build ---
FROM maven:3.9-eclipse-temurin-{java} AS build
WORKDIR /app
COPY pom.xml .
RUN mvn dependency:go-offline -B
COPY src ./src
RUN mvn package -DskipTests -B

# --- JVM runtime ---
FROM eclipse-temurin:{java}-jre-alpine AS jvm
WORKDIR /app
COPY --from=build /app/target/quarkus-app /app
EXPOSE 8080
ENTRYPOINT ["java", "-jar", "quarkus-run.jar"]

# --- Native build (optional) ---
FROM quay.io/quarkus/ubi-quarkus-mandrel-builder-image:jdk-{java} AS native-build
WORKDIR /app
COPY pom.xml .
COPY src ./src
RUN mvn package -Pnative -DskipTests -B

# --- Native runtime ---
FROM quay.io/quarkus/quarkus-micro-image:2.0 AS native
WORKDIR /app
COPY --from=native-build /app/target/*-runner /app/application
EXPOSE 8080
ENTRYPOINT ["./application", "-Dquarkus.http.host=0.0.0.0"]
"#,
        java = JAVA_RELEASE
    )
}

const GITIGNORE: &str = r#"target/
.idea/
*.iml
.settings/
.project
.classpath
.vscode/
*.class
*.jar
*.log
.env
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(group_id: &str, artifact_id: &str, version: Option<&str>) -> MavenDependency {
        MavenDependency {
            group_id: group_id.into(),
            artifact_id: artifact_id.into(),
            version: version.map(Into::into),
        }
    }

    fn project(deps: Vec<MavenDependency>) -> CamelProject {
        CamelProject {
            name: "orders".into(),
            maven_dependencies: deps,
            ..Default::default()
        }
    }

    /// Text of the first `<tag>...</tag>` in `xml`.
    fn element<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
        let open = format!("<{tag}>");
        let start = xml.find(&open)? + open.len();
        let end = start + xml[start..].find(&format!("</{tag}>"))?;
        Some(xml[start..end].trim())
    }

    /// Text of every `<tag>...</tag>` in `xml`, in document order.
    fn elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
        let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
        let mut out = Vec::new();
        let mut rest = xml;
        while let Some(i) = rest.find(&open) {
            let body = &rest[i + open.len()..];
            let Some(j) = body.find(&close) else { break };
            out.push(body[..j].trim());
            rest = &body[j + close.len()..];
        }
        out
    }

    /// `(groupId:artifactId, version, scope)` of the project's own
    /// dependencies (the `<dependencies>` block after `<dependencyManagement>`).
    fn declared_dependencies(pom: &str) -> Vec<(String, Option<String>, Option<String>)> {
        let after_management = pom
            .split("</dependencyManagement>")
            .nth(1)
            .expect("pom has a dependencyManagement block");
        let block = element(after_management, "dependencies").expect("pom has dependencies");
        elements(block, "dependency")
            .into_iter()
            .map(|d| {
                (
                    format!(
                        "{}:{}",
                        element(d, "groupId").unwrap(),
                        element(d, "artifactId").unwrap()
                    ),
                    element(d, "version").map(str::to_string),
                    element(d, "scope").map(str::to_string),
                )
            })
            .collect()
    }

    // ── onException ─────────────────────────────────────────────────────

    fn handler(classes: &[&str], handled: bool, steps: Vec<RouteStep>) -> ExceptionHandler {
        ExceptionHandler {
            exception_classes: classes.iter().map(|c| c.to_string()).collect(),
            handled,
            steps,
        }
    }

    /// Java source of the RouteBuilder for a flow `safeFlow` that has the
    /// given exception handlers and no steps.
    fn route_java(on_exceptions: Vec<ExceptionHandler>) -> String {
        generate_route_builder(
            "SafeFlowRoute",
            &CamelRoute {
                id: "safeFlow".into(),
                source: RouteEndpoint::Uri("platform-http:/safe?httpMethodRestrict=GET".into()),
                steps: vec![],
                on_exceptions,
            },
        )
    }

    /// The lines of `java` that are code: trimmed, without blank lines and
    /// `//` comment lines.
    fn code_lines(java: &str) -> Vec<&str> {
        java.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//"))
            .collect()
    }

    #[test]
    fn on_exception_names_a_single_exception_type_as_a_class_literal() {
        // examples/hello-world, safeFlow: on-error-continue type="HTTP:CONNECTIVITY".
        let java = route_java(vec![handler(
            &["java.net.ConnectException"],
            true,
            vec![
                RouteStep::SetBody {
                    expression: Expression::Constant("Service temporarily unavailable".into()),
                },
                RouteStep::Log {
                    level: LogLevel::Warn,
                    message: "Upstream service unreachable".into(),
                },
            ],
        )]);

        // `onException(Class<? extends Throwable>...)` takes class literals:
        // a bare type name is not a Java expression and does not compile.
        let clause = r#"
        onException(java.net.ConnectException.class)
            .handled(true)
            .setBody(constant("Service temporarily unavailable"))
            .log(LoggingLevel.WARN, "Upstream service unreachable")
            .end();

        from("platform-http:/safe?httpMethodRestrict=GET")
            .routeId("safeFlow")
"#;
        assert!(
            java.contains(clause),
            "expected clause:{clause}\nin:\n{java}"
        );
    }

    #[test]
    fn on_exception_lists_several_exception_types_as_class_literals() {
        let java = route_java(vec![handler(
            &[
                "java.net.ConnectException",
                "java.net.SocketTimeoutException",
            ],
            true,
            vec![],
        )]);

        assert!(
            code_lines(&java).contains(
                &"onException(java.net.ConnectException.class, java.net.SocketTimeoutException.class)"
            ),
            "{java}"
        );
    }

    #[test]
    fn on_exception_keeps_fully_qualified_class_names() {
        // Nothing is imported for exception classes, so the package has to
        // stay on the name; a name that already is a class literal is not
        // suffixed a second time.
        let java = route_java(vec![
            handler(
                &["org.apache.camel.http.base.HttpOperationFailedException"],
                false,
                vec![],
            ),
            handler(&["java.sql.SQLException.class"], false, vec![]),
        ]);

        let code = code_lines(&java);
        assert!(
            code.contains(
                &"onException(org.apache.camel.http.base.HttpOperationFailedException.class)"
            ),
            "{java}"
        );
        assert!(
            code.contains(&"onException(java.sql.SQLException.class)"),
            "{java}"
        );
    }

    #[test]
    fn on_exception_always_states_whether_the_exception_is_handled() {
        // on-error-continue: the handler's result is the route's reply.
        let continued = route_java(vec![handler(&["java.lang.Exception"], true, vec![])]);
        assert!(
            continued.contains(
                "        onException(java.lang.Exception.class)\n            .handled(true)\n            .end();\n"
            ),
            "{continued}"
        );
        // on-error-propagate: the handler runs and the caller still gets
        // the failure. That is Camel's default, spelled out.
        let propagated = route_java(vec![handler(&["java.lang.Exception"], false, vec![])]);
        assert!(
            propagated.contains(
                "        onException(java.lang.Exception.class)\n            .handled(false)\n            .end();\n"
            ),
            "{propagated}"
        );
        // Mule's on-error-continue ends the flow with the handler's result;
        // Camel's continued(true) would resume the route after the failed step.
        assert!(!continued.contains("continued("), "{continued}");
        assert!(!propagated.contains("continued("), "{propagated}");
    }

    #[test]
    fn on_exception_without_an_exception_class_is_emitted_commented_out() {
        // The mapper found no exception class for the handler's Mule error
        // type. `onException()` would compile and then fail at startup, and
        // `Exception.class` would catch far more than the Mule handler did.
        let java = route_java(vec![handler(
            &[],
            true,
            vec![
                RouteStep::RawDsl("TODO: no exception class for FILE:NOT_FOUND".into()),
                RouteStep::SetBody {
                    expression: Expression::Constant("fallback".into()),
                },
            ],
        )]);

        let code = code_lines(&java);
        assert!(
            !code
                .iter()
                .any(|l| l.contains("onException") || l.contains("fallback")),
            "a handler without exception class must not be active code:\n{java}"
        );
        // Still there for whoever completes it by hand.
        let commented = r#"
        // onException(/* TODO: exception class */)
        //     .handled(true)
        //     // TODO: no exception class for FILE:NOT_FOUND
        //     .setBody(constant("fallback"))
        //     .end();

        from("platform-http:/safe?httpMethodRestrict=GET")
"#;
        assert!(
            java.contains(commented),
            "expected:{commented}\nin:\n{java}"
        );
        // The route itself is unaffected.
        assert!(code.contains(&r#".routeId("safeFlow")"#), "{java}");
    }

    // ── Error handling of sub-flow routes ───────────────────────────────

    /// Java source of the RouteBuilder for a route `shared` with one step.
    fn shared_route_java(source: RouteEndpoint, on_exceptions: Vec<ExceptionHandler>) -> String {
        generate_route_builder(
            "SharedRoute",
            &CamelRoute {
                id: "shared".into(),
                source,
                steps: vec![RouteStep::SetProperty {
                    name: "enriched".into(),
                    expression: Expression::Constant("true".into()),
                }],
                on_exceptions,
            },
        )
    }

    #[test]
    fn sub_flow_route_leaves_its_failures_to_the_calling_route() {
        // A Mule sub-flow has no error handling of its own: what fails in it
        // is handled by the error handler of the flow that called it. A
        // Camel route gets the default error handler, and after that one
        // the onException clauses of the calling route no longer run. So
        // the route of a sub-flow is emitted without error handler.
        let java = shared_route_java(RouteEndpoint::Direct("shared".into()), vec![]);

        let route = r#"
    public void configure() throws Exception {
        from("direct:shared")
            .routeId("shared")
            .errorHandler(noErrorHandler())
            .setProperty("enriched", constant("true"))
            ;
    }
"#;
        assert!(java.contains(route), "expected route:{route}\nin:\n{java}");
    }

    #[test]
    fn route_of_a_flow_keeps_its_error_handler() {
        // A flow handles its own errors: with its on-error scopes, and with
        // the default error handler for the rest. That includes a flow the
        // mapper could only give a placeholder direct: source.
        for source in [
            "platform-http:/safe?httpMethodRestrict=GET",
            "kafka:orders?brokers=broker-1:9092",
            "direct:kafka-message-listener",
        ] {
            for on_exceptions in [
                vec![],
                vec![handler(&["java.lang.Exception"], true, vec![])],
            ] {
                let java = shared_route_java(RouteEndpoint::Uri(source.into()), on_exceptions);

                assert!(!java.contains("errorHandler"), "{source}:\n{java}");
                assert!(
                    java.contains(&format!(
                        "        from(\"{source}\")\n            .routeId(\"shared\")\n            .setProperty("
                    )),
                    "{source}:\n{java}"
                );
            }
        }
    }

    #[test]
    fn direct_route_with_exception_handlers_keeps_them_in_force() {
        // onException clauses only work with an error handler: without one
        // they would be ignored.
        let java = shared_route_java(
            RouteEndpoint::Direct("shared".into()),
            vec![handler(&["java.lang.Exception"], false, vec![])],
        );

        assert!(!java.contains("errorHandler"), "{java}");
        assert!(
            code_lines(&java).contains(&"onException(java.lang.Exception.class)"),
            "{java}"
        );
    }

    // ── `//` comments ───────────────────────────────────────────────────

    /// Java source of `steps`, as emitted inside a route.
    fn steps_java(steps: &[RouteStep]) -> String {
        let mut java = String::new();
        for step in steps {
            emit_step_dsl(&mut java, step, 3);
        }
        java
    }

    /// The non-blank lines of `java` as javac splits them: a line ends at
    /// LF, at CR and at CR LF (JLS 3.4), and a `//` comment ends with it.
    fn javac_lines(java: &str) -> Vec<&str> {
        java.split(['\n', '\r'])
            .filter(|l| !l.trim().is_empty())
            .collect()
    }

    /// Whether javac reads a Unicode escape in `java`: a `u` after a
    /// backslash that an even number of backslashes precedes (JLS 3.3).
    /// It does so before it recognises comments, and rejects the source
    /// unless four hex digits follow.
    fn starts_unicode_escape(java: &str) -> bool {
        let mut backslashes = 0;
        for c in java.chars() {
            if c == '\\' {
                backslashes += 1;
                continue;
            }
            if c == 'u' && backslashes % 2 == 1 {
                return true;
            }
            backslashes = 0;
        }
        false
    }

    #[test]
    fn comment_text_is_written_as_it_is() {
        // Quotes, `*/`, backslashes that start no Unicode escape, non-ASCII
        // text, blank lines, LF and CR LF line ends: nothing of it can end a
        // `//` comment or break the source, so nothing of it is rewritten.
        let note = "TODO: review this handler (on-error-continue type=\"HTTP:TIMEOUT\" \
                    when=\"#[error.description contains 'a*/b' or matches '\\d+\\\\unc' or 'C:\\temp']\")\n\
                    - größer als 5 € */ /* \"quoted\" \\\\u0041\r\n\
                    \n\
                    - last line";
        let java = steps_java(&[RouteStep::RawDsl(note.into())]);

        let expected = concat!(
            "            // TODO: review this handler (on-error-continue type=\"HTTP:TIMEOUT\" ",
            "when=\"#[error.description contains 'a*/b' or matches '\\d+\\\\unc' or 'C:\\temp']\")\n",
            "            // - größer als 5 € */ /* \"quoted\" \\\\u0041\n",
            "            // \n",
            "            // - last line\n",
        );
        assert_eq!(java, expected);
        assert!(!starts_unicode_escape(&java), "{java}");
    }

    #[test]
    fn comment_text_with_a_backslash_before_u_does_not_start_a_unicode_escape() {
        // javac translates Unicode escapes before it looks for comments:
        // `C:\users` in a comment is an "illegal unicode escape", and
        // `\u000a` would be a line end in the middle of the comment.
        for (text, comment) in [
            (
                r##"when="#[error.description contains 'C:\users\x']""##,
                r##"when="#[error.description contains 'C:\\users\x']""##,
            ),
            (r"\u000a drop();", r"\\u000a drop();"),
            (r"\uuu0041", r"\\uuu0041"),
            (r"a\\\ub", r"a\\\\ub"),
            (r"\u", r"\\u"),
            // An even number of backslashes before the `u` is no escape.
            (r"a\\ub \\\\u0041", r"a\\ub \\\\u0041"),
        ] {
            let java = steps_java(&[RouteStep::RawDsl(text.into())]);

            assert_eq!(java, format!("            // {comment}\n"), "{text}");
            assert!(!starts_unicode_escape(&java), "{text}: {java}");
        }
    }

    #[test]
    fn comment_text_with_a_lone_carriage_return_stays_a_comment() {
        // For javac a lone CR ends the line like LF does. Text behind it
        // must not become code.
        let java = steps_java(&[RouteStep::RawDsl(
            "when=\"#[error.description contains 'a\rb']\"); drop();\r\nsecond\nthird\r".into(),
        )]);

        assert_eq!(
            java,
            concat!(
                "            // when=\"#[error.description contains 'a\n",
                "            // b']\"); drop();\n",
                "            // second\n",
                "            // third\n",
            )
        );
        for line in javac_lines(&java) {
            assert!(line.trim_start().starts_with("//"), "{line:?} in:\n{java}");
        }
    }

    #[test]
    fn commented_out_handler_stays_a_comment_whatever_its_note_says() {
        // An on-error scope with a `when` condition is emitted commented
        // out, with the condition quoted in its note.
        let java = route_java(vec![handler(
            &[],
            true,
            vec![
                RouteStep::RawDsl(
                    "TODO: review this handler (on-error-continue \
                     when=\"#[error.description contains 'C:\\users\\x\ry']\")"
                        .into(),
                ),
                RouteStep::SetBody {
                    expression: Expression::Constant("fallback".into()),
                },
            ],
        )]);

        assert!(!starts_unicode_escape(&java), "{java}");
        let lines = javac_lines(&java);
        let from = lines
            .iter()
            .position(|l| l.trim_start().starts_with("from("))
            .expect("route has a from()");
        let clause_start = lines
            .iter()
            .position(|l| l.contains("onException("))
            .expect("handler is emitted");
        assert!(clause_start < from, "{java}");
        for line in &lines[clause_start..from] {
            assert!(line.trim_start().starts_with("//"), "{line:?} in:\n{java}");
        }
        assert!(java.contains(r"'C:\\users\x"), "{java}");
    }

    #[test]
    fn pom_takes_every_platform_version_from_one_property() {
        let pom = generate_pom(&project(vec![dep(
            "org.apache.camel.quarkus",
            "camel-quarkus-core",
            None,
        )]));

        // quarkus-bom, quarkus-camel-bom and quarkus-maven-plugin are released
        // together under one platform version; nothing else carries a version.
        assert_eq!(
            elements(&pom, "version"),
            [
                "1.0.0-SNAPSHOT",
                "${quarkus.platform.version}",
                "${quarkus.platform.version}",
                "${quarkus.platform.version}",
            ]
        );
        let platform = element(&pom, "quarkus.platform.version").expect("platform version");
        assert!(
            platform.split('.').count() >= 3 && !platform.contains('$'),
            "quarkus.platform.version must be a literal release, got {platform:?}"
        );
    }

    #[test]
    fn pom_defines_no_property_that_nothing_uses() {
        let pom = generate_pom(&project(vec![]));
        let properties = element(&pom, "properties").expect("pom has properties");

        for line in properties.lines() {
            let name = line
                .trim()
                .strip_prefix('<')
                .and_then(|l| l.split('>').next())
                .expect("one property per line");
            // Read by Maven plugins themselves, not via ${...}.
            if matches!(
                name,
                "maven.compiler.release" | "project.build.sourceEncoding"
            ) {
                continue;
            }
            assert!(
                pom.contains(&format!("${{{name}}}")),
                "property {name} is defined but never referenced"
            );
        }
    }

    #[test]
    fn pom_declares_route_dependencies_then_baseline_then_test_scope() {
        let pom = generate_pom(&project(vec![
            dep("org.apache.camel.quarkus", "camel-quarkus-core", None),
            dep("org.apache.camel.quarkus", "camel-quarkus-kafka", None),
        ]));

        let declared: Vec<(String, Option<String>)> = declared_dependencies(&pom)
            .into_iter()
            .map(|(ga, _, scope)| (ga, scope))
            .collect();
        let test = Some("test".to_string());
        assert_eq!(
            declared,
            [
                // What the routes need (from the IR), in IR order.
                ("org.apache.camel.quarkus:camel-quarkus-core".into(), None),
                ("org.apache.camel.quarkus:camel-quarkus-kafka".into(), None),
                // What every emitted project gets.
                (
                    "io.quarkus:quarkus-micrometer-registry-prometheus".into(),
                    None
                ),
                ("io.quarkus:quarkus-opentelemetry".into(), None),
                // What the generated tests import: @QuarkusTest + JUnit
                // Jupiter, RestAssured + Hamcrest.
                ("io.quarkus:quarkus-junit".into(), test.clone()),
                ("io.rest-assured:rest-assured".into(), test),
            ]
        );
    }

    #[test]
    fn pom_does_not_declare_a_dependency_twice() {
        // A mapping rule may already ask for an extension the emitter adds
        // to every project.
        let pom = generate_pom(&project(vec![
            dep("org.apache.camel.quarkus", "camel-quarkus-core", None),
            dep("io.quarkus", "quarkus-opentelemetry", None),
        ]));

        let mut coordinates: Vec<String> = declared_dependencies(&pom)
            .into_iter()
            .map(|(ga, _, _)| ga)
            .collect();
        let total = coordinates.len();
        coordinates.sort();
        coordinates.dedup();
        assert_eq!(coordinates.len(), total, "duplicate dependency in:\n{pom}");
    }

    #[test]
    fn pom_keeps_an_explicit_dependency_version_and_lets_the_boms_manage_the_rest() {
        let pom = generate_pom(&project(vec![
            dep("org.apache.camel.quarkus", "camel-quarkus-core", None),
            dep("com.acme", "legacy-client", Some("1.2.3")),
        ]));

        let versions: Vec<(String, Option<String>)> = declared_dependencies(&pom)
            .into_iter()
            .map(|(ga, version, _)| (ga, version))
            .collect();
        for (ga, version) in &versions {
            if ga == "com.acme:legacy-client" {
                assert_eq!(version.as_deref(), Some("1.2.3"));
            } else {
                assert_eq!(version, &None, "{ga} must be managed by the platform BOMs");
            }
        }
        assert!(versions
            .iter()
            .any(|(ga, _)| ga == "com.acme:legacy-client"));
    }

    #[test]
    fn pom_identifies_the_project() {
        let pom = generate_pom(&project(vec![]));

        assert_eq!(element(&pom, "groupId"), Some("generated"));
        assert_eq!(element(&pom, "artifactId"), Some("orders"));
        assert_eq!(element(&pom, "packaging"), Some("jar"));
    }

    #[test]
    fn pom_artifact_id_is_a_valid_maven_id_for_any_project_name() {
        // The project name is the Mule project's directory name. Maven
        // rejects the whole pom unless the artifactId is `[A-Za-z0-9_.-]+`.
        for (name, artifact_id) in [
            ("orders", "orders"),
            ("hello-world_mule.v2", "hello-world_mule.v2"),
            ("My Mule App", "My-Mule-App"),
            ("orders (copy) & co", "orders-copy-co"),
            ("  bestellungen-für-köln ", "bestellungen-f-r-k-ln"),
            ("???", "migrated-project"),
            ("", "migrated-project"),
        ] {
            let mut ir = project(vec![]);
            ir.name = name.into();
            let pom = generate_pom(&ir);
            assert_eq!(
                element(&pom, "artifactId"),
                Some(artifact_id),
                "artifactId for project name {name:?}"
            );
        }
    }

    #[test]
    fn pom_builds_tests_and_native_images_with_current_quarkus_settings() {
        let pom = generate_pom(&project(vec![]));

        assert_eq!(
            elements(&pom, "goal"),
            ["build", "generate-code", "generate-code-tests"]
        );
        // `mvn package -Pnative` is what the Makefile, the Dockerfile and the
        // release workflow run. The platform's build reports the former
        // `quarkus.package.type` as deprecated and replaced by these two.
        let native = pom
            .split("<id>native</id>")
            .nth(1)
            .expect("pom has a native profile");
        assert_eq!(element(native, "quarkus.native.enabled"), Some("true"));
        assert_eq!(
            element(native, "quarkus.package.jar.enabled"),
            Some("false")
        );
        assert!(!pom.contains("quarkus.package.type"));
    }

    // ── Expressions that are not converted ──────────────────────────────

    fn http_route(steps: Vec<RouteStep>) -> String {
        generate_route_builder(
            "OrdersRoute",
            &CamelRoute {
                id: "orders".into(),
                source: RouteEndpoint::Uri("platform-http:/orders".into()),
                steps,
                on_exceptions: vec![],
            },
        )
    }

    fn unconverted(original: &str, reason: &str) -> Expression {
        Expression::Unconverted {
            original: original.into(),
            reason: reason.into(),
        }
    }

    fn set_body(text: &str) -> RouteStep {
        RouteStep::SetBody {
            expression: Expression::Constant(text.into()),
        }
    }

    fn property_equals(name: &str, value: ComparedValue, negated: bool) -> Expression {
        Expression::PropertyEquals {
            original: "#[mule]".into(),
            name: name.into(),
            value,
            negated,
            review: vec![],
        }
    }

    /// One `when` per condition, each with the body `x`.
    fn choice_of(conditions: Vec<Expression>) -> RouteStep {
        RouteStep::Choice {
            whens: conditions
                .into_iter()
                .map(|condition| (condition, vec![set_body("x")]))
                .collect(),
            otherwise: None,
        }
    }

    /// The branch stays where it is; its condition is a predicate that
    /// throws, so a message that gets that far fails instead of being
    /// routed by a guess. Branches in front of it work as before.
    #[test]
    fn unconverted_choice_condition_is_a_predicate_that_throws() {
        let java = http_route(vec![RouteStep::Choice {
            whens: vec![
                (
                    property_equals("type", ComparedValue::Text("A".into()), false),
                    vec![set_body("a")],
                ),
                (
                    unconverted("#[payload.type == 'B']", "the payload type is not known"),
                    vec![set_body("b")],
                ),
            ],
            otherwise: Some(vec![set_body("c")]),
        }]);
        assert!(
            java.contains(
                r#"
            .choice()
                .when(exchange -> "A".equals(exchange.getProperty("type")))
                    .setBody(constant("a"))
                // TODO: review this condition, it is not converted (Mule: #[payload.type == 'B'])
                // - the payload type is not known
                // - a message that reaches this condition fails here instead of taking a branch by guess; replace the predicate
                .when(exchange -> { throw new UnsupportedOperationException("choice condition not migrated: #[payload.type == 'B']"); })
                    .setBody(constant("b"))
                .otherwise()
                    .setBody(constant("c"))
            .end()
            ;
"#
            ),
            "{java}"
        );
    }

    /// A value that is not converted: the route fails at that step. It is
    /// neither set to the Mule expression as text nor skipped.
    #[test]
    fn unconverted_value_is_a_step_that_throws() {
        let java = http_route(vec![
            RouteStep::SetProperty {
                name: "id".into(),
                expression: unconverted("#[payload.id]", "the payload type is not known"),
            },
            RouteStep::SetBody {
                expression: unconverted("#[now()]", "now() is a function"),
            },
            RouteStep::SetHeader {
                name: "x-id".into(),
                expression: unconverted("#[uuid()]", "uuid() is a function"),
            },
        ]);
        assert!(
            java.contains(
                r#"
            .routeId("orders")
            // TODO: review this setProperty("id"), its value is not converted (Mule: #[payload.id])
            // - the payload type is not known
            // - the route fails at this step instead of going on with a wrong value; replace it with the converted step
            .throwException(new UnsupportedOperationException("setProperty(\"id\") value not migrated: #[payload.id]"))
            // TODO: review this setBody, its value is not converted (Mule: #[now()])
            // - now() is a function
            // - the route fails at this step instead of going on with a wrong value; replace it with the converted step
            .throwException(new UnsupportedOperationException("setBody value not migrated: #[now()]"))
            // TODO: review this setHeader("x-id"), its value is not converted (Mule: #[uuid()])
            // - uuid() is a function
            // - the route fails at this step instead of going on with a wrong value; replace it with the converted step
            .throwException(new UnsupportedOperationException("setHeader(\"x-id\") value not migrated: #[uuid()]"))
            ;
"#
            ),
            "{java}"
        );
        assert!(!java.contains("simple(\"#["), "{java}");
        assert!(!java.contains("constant(\"#["), "{java}");
    }

    /// The steps of a split whose expression is not converted stay in the
    /// route, behind the step that fails.
    #[test]
    fn unconverted_split_expression_is_a_step_that_throws() {
        let java = http_route(vec![RouteStep::Split {
            expression: unconverted("#[payload.items]", "the payload type is not known"),
            parallel: false,
            aggregation_strategy: None,
            steps: vec![set_body("item")],
        }]);
        assert!(
            java.contains(
                r#"
            .routeId("orders")
            // TODO: review this split, its expression is not converted (Mule: #[payload.items])
            // - the payload type is not known
            // - the route fails at this step instead of going on with a wrong value; replace it with the converted step
            // - the steps of the split follow it and are not reached
            .throwException(new UnsupportedOperationException("split expression not migrated: #[payload.items]"))
            .setBody(constant("item"))
            ;
"#
            ),
            "{java}"
        );
    }

    #[test]
    fn literal_is_written_as_a_java_literal_not_as_a_string() {
        let java = http_route(vec![
            RouteStep::SetProperty {
                name: "count".into(),
                expression: Expression::Literal("42".into()),
            },
            RouteStep::Choice {
                whens: vec![(Expression::Literal("true".into()), vec![set_body("a")])],
                otherwise: None,
            },
        ]);
        assert!(
            java.contains(
                r#"
            .setProperty("count", constant(42))
            .choice()
                .when(constant(true))
                    .setBody(constant("a"))
            .end()
"#
            ),
            "{java}"
        );
    }

    /// The Mule expression is quoted in a comment and in a string literal;
    /// it must not be able to leave either.
    #[test]
    fn unconverted_expression_cannot_break_out_of_comment_or_string() {
        let original = "#[vars.dir == \"C:\\users\"]\r\n.stop()";
        let java = http_route(vec![RouteStep::Choice {
            whens: vec![(unconverted(original, "why"), vec![set_body("a")])],
            otherwise: None,
        }]);
        assert!(
            java.contains(
                r#"
                // TODO: review this condition, it is not converted (Mule: #[vars.dir == "C:\\users"]
                // .stop())
                // - why
"#
            ),
            "{java}"
        );
        assert!(
            java.contains(
                r#"("choice condition not migrated: #[vars.dir == \"C:\\users\"]\r\n.stop()"); })"#
            ),
            "{java}"
        );
        for line in java.lines().map(str::trim) {
            assert!(!line.starts_with(".stop()"), "{java}");
        }
    }

    // ── Exact comparison of an exchange property ────────────────────────

    /// Mule's `==` does not convert types, Simple's does. So the predicate
    /// is Java on the value of the property: `equals` of a `String` or a
    /// `Boolean` constant is true only for a value of that type, and `!=`
    /// is the same test negated.
    #[test]
    fn property_comparison_is_a_java_predicate_that_does_not_convert_types() {
        let java = http_route(vec![choice_of(vec![
            property_equals("type", ComparedValue::Text("A".into()), false),
            property_equals("type", ComparedValue::Text("A".into()), true),
            property_equals("active", ComparedValue::Boolean(true), false),
            property_equals("active", ComparedValue::Boolean(false), true),
            property_equals("count", ComparedValue::Integer(3), false),
            property_equals("count", ComparedValue::Integer(-1), true),
            property_equals("id", ComparedValue::Null, false),
            property_equals("id", ComparedValue::Null, true),
        ])]);
        let whens: Vec<&str> = java
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with(".when("))
            .collect();
        assert_eq!(
            whens,
            [
                r#".when(exchange -> "A".equals(exchange.getProperty("type")))"#,
                r#".when(exchange -> !"A".equals(exchange.getProperty("type")))"#,
                r#".when(exchange -> Boolean.TRUE.equals(exchange.getProperty("active")))"#,
                r#".when(exchange -> !Boolean.FALSE.equals(exchange.getProperty("active")))"#,
                r#".when(exchange -> isNumber(exchange.getProperty("count"), 3))"#,
                r#".when(exchange -> !isNumber(exchange.getProperty("count"), -1))"#,
                r#".when(exchange -> exchange.getProperty("id") == null)"#,
                r#".when(exchange -> exchange.getProperty("id") != null)"#,
            ],
            "{java}"
        );
        assert!(!java.contains("simple("), "{java}");
        assert!(!java.contains("TODO"), "{java}");
    }

    /// A number equals a number of any Java type with the same value, and
    /// nothing else. The method that decides it is part of the class that
    /// calls it, and only of such a class.
    #[test]
    fn class_that_compares_a_number_gets_the_method_that_does_it() {
        let number = || property_equals("count", ComparedValue::Integer(3), false);
        let method = r#"
            ;
    }

    // Whether value is a number equal to expected, as Mule compares numbers:
    // by value, whatever the Java type (Integer 3, Long 3, Double 3.0 and
    // BigDecimal 3.0 all equal 3), and never a String ("3") or a Boolean.
    private static boolean isNumber(Object value, long expected) {
        if (value instanceof java.math.BigDecimal decimal) {
            return decimal.compareTo(java.math.BigDecimal.valueOf(expected)) == 0;
        }
        if (value instanceof java.math.BigInteger integer) {
            return integer.equals(java.math.BigInteger.valueOf(expected));
        }
        return value instanceof Number number
                && number.doubleValue() == expected
                && number.longValue() == expected;
    }
}
"#;

        let direct = http_route(vec![choice_of(vec![number()])]);
        assert!(direct.ends_with(method), "{direct}");

        // Nested in a branch, in a split and in an exception clause.
        let nested = http_route(vec![RouteStep::Choice {
            whens: vec![(
                Expression::Literal("true".into()),
                vec![RouteStep::Split {
                    expression: Expression::Simple("${body}".into()),
                    parallel: false,
                    aggregation_strategy: None,
                    steps: vec![choice_of(vec![number()])],
                }],
            )],
            otherwise: None,
        }]);
        assert!(nested.ends_with(method), "{nested}");

        let otherwise = http_route(vec![RouteStep::Choice {
            whens: vec![],
            otherwise: Some(vec![choice_of(vec![number()])]),
        }]);
        assert!(otherwise.ends_with(method), "{otherwise}");

        let handler = generate_route_builder(
            "OrdersRoute",
            &CamelRoute {
                id: "orders".into(),
                source: RouteEndpoint::Uri("platform-http:/orders".into()),
                steps: vec![],
                on_exceptions: vec![ExceptionHandler {
                    exception_classes: vec!["java.lang.Exception".into()],
                    handled: true,
                    steps: vec![choice_of(vec![number()])],
                }],
            },
        );
        assert!(handler.ends_with(method), "{handler}");

        let without = http_route(vec![choice_of(vec![
            property_equals("type", ComparedValue::Text("A".into()), false),
            property_equals("active", ComparedValue::Boolean(true), false),
            property_equals("id", ComparedValue::Null, true),
        ])]);
        assert!(!without.contains("isNumber"), "{without}");
        assert!(without.ends_with("            ;\n    }\n}\n"), "{without}");
    }

    /// A comparison that rests on an assumption is emitted all the same,
    /// with what has to be checked in a TODO in front of it. The notes and
    /// the Mule expression come from the Mule XML and stay in the comment.
    #[test]
    fn property_comparison_to_review_gets_a_todo_and_is_still_emitted() {
        let java = http_route(vec![choice_of(vec![
            Expression::PropertyEquals {
                original: "#[vars.kind == 'A']".into(),
                name: "kind".into(),
                value: ComparedValue::Text("A".into()),
                negated: false,
                review: vec![
                    "first note".into(),
                    "second note */\r.stop() C:\\users".into(),
                ],
            },
            property_equals("kind", ComparedValue::Text("B".into()), false),
        ])]);
        assert!(
            java.contains(
                r#"
            .choice()
                // TODO: review this condition (Mule: #[vars.kind == 'A'])
                // - first note
                // - second note */
                // .stop() C:\\users
                .when(exchange -> "A".equals(exchange.getProperty("kind")))
                    .setBody(constant("x"))
                .when(exchange -> "B".equals(exchange.getProperty("kind")))
                    .setBody(constant("x"))
            .end()
"#
            ),
            "{java}"
        );
    }

    /// The text and the property name end up in Java string literals.
    #[test]
    fn property_comparison_escapes_what_it_puts_into_string_literals() {
        let java = http_route(vec![choice_of(vec![property_equals(
            "a\"b",
            ComparedValue::Text("say \"hi\" C:\\users\n${x} {{y}}".into()),
            false,
        )])]);
        assert!(
            java.contains(
                r#".when(exchange -> "say \"hi\" C:\\users\n${x} {{y}}".equals(exchange.getProperty("a\"b")))"#
            ),
            "{java}"
        );
    }

    // ── Long expressions ────────────────────────────────────────────────

    /// The Mule expression is quoted in the message of the exception. A
    /// Java string constant holds at most 65535 bytes: with an expression
    /// of 81 KB the route class did not compile ("constant string too
    /// long"). The message gets the beginning of the expression, cut
    /// between two characters, and says so; the TODO keeps all of it.
    #[test]
    fn long_expression_is_cut_in_the_exception_message_and_kept_in_the_todo() {
        // Two bytes per character, so that a cut by bytes would show.
        let original = format!("#[payload.name == '{}']", "\u{e9}".repeat(81_000));
        let java = http_route(vec![
            RouteStep::Choice {
                whens: vec![(unconverted(&original, "why"), vec![set_body("a")])],
                otherwise: None,
            },
            RouteStep::SetBody {
                expression: unconverted(&original, "why"),
            },
        ]);

        let cut: String = original.chars().take(200).collect();
        let marker = "... (truncated, full expression in the TODO above)";
        assert!(
            java.contains(&format!(
                r#".when(exchange -> {{ throw new UnsupportedOperationException("choice condition not migrated: {cut}{marker}"); }})"#
            )),
            "the condition is not cut as expected"
        );
        assert!(
            java.contains(&format!(
                r#".throwException(new UnsupportedOperationException("setBody value not migrated: {cut}{marker}"))"#
            )),
            "the step is not cut as expected"
        );
        // No line of code is anywhere near the limit of a string constant.
        for line in java.lines().filter(|l| !l.trim_start().starts_with("//")) {
            assert!(
                line.len() < 1000,
                "{} bytes of code in one line",
                line.len()
            );
        }
        // Both TODO comments have the whole expression.
        assert_eq!(
            java.matches(&format!("(Mule: {original})")).count(),
            2,
            "the TODO does not keep the full expression"
        );
    }

    /// An expression of up to 200 characters is quoted as it is.
    #[test]
    fn expression_of_200_characters_is_not_cut() {
        let original = format!("#[{}]", "a".repeat(197));
        assert_eq!(original.chars().count(), 200);
        let java = http_route(vec![RouteStep::SetBody {
            expression: unconverted(&original, "why"),
        }]);
        assert!(
            java.contains(&format!(
                r#".throwException(new UnsupportedOperationException("setBody value not migrated: {original}"))"#
            )),
            "{java}"
        );
        assert!(!java.contains("truncated"), "{java}");
    }

    // ── Where a choice ends ─────────────────────────────────────────────

    fn when_is(name: &str, text: &str) -> Expression {
        property_equals(name, ComparedValue::Text(text.into()), false)
    }

    fn log(message: &str) -> RouteStep {
        RouteStep::Log {
            level: LogLevel::Info,
            message: message.into(),
        }
    }

    /// The steps of the route as they are written: the text between the
    /// `routeId` line and the `;`.
    fn route_steps(java: &str) -> &str {
        let start = java.find("            .routeId(").expect("routeId");
        let start = start + java[start..].find('\n').expect("line end") + 1;
        let end = java.find("            ;\n").expect("end of route");
        &java[start..end]
    }

    /// `.end()` closes the choice, so the step after it is a step of the
    /// route and runs whichever branch was taken. (`.endChoice()` on a
    /// choice returns that choice: what follows it goes into the last
    /// branch.)
    #[test]
    fn choice_is_closed_with_end_and_the_next_step_follows_the_choice() {
        let java = http_route(vec![
            RouteStep::Choice {
                whens: vec![
                    (when_is("k", "a"), vec![log("A")]),
                    (when_is("k", "b"), vec![log("B")]),
                ],
                otherwise: Some(vec![log("O")]),
            },
            log("AFTER"),
            set_body("end"),
        ]);
        assert_eq!(
            route_steps(&java),
            r#"            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .log(LoggingLevel.INFO, "A")
                .when(exchange -> "b".equals(exchange.getProperty("k")))
                    .log(LoggingLevel.INFO, "B")
                .otherwise()
                    .log(LoggingLevel.INFO, "O")
            .end()
            .log(LoggingLevel.INFO, "AFTER")
            .setBody(constant("end"))
"#,
            "{java}"
        );
    }

    #[test]
    fn choice_without_otherwise_is_closed_before_the_next_step() {
        let java = http_route(vec![
            RouteStep::Choice {
                whens: vec![(when_is("k", "a"), vec![log("A")])],
                otherwise: None,
            },
            log("AFTER"),
        ]);
        assert_eq!(
            route_steps(&java),
            r#"            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .log(LoggingLevel.INFO, "A")
            .end()
            .log(LoggingLevel.INFO, "AFTER")
"#,
            "{java}"
        );
    }

    #[test]
    fn choice_as_the_last_step_of_the_route_is_closed() {
        let java = http_route(vec![RouteStep::Choice {
            whens: vec![(when_is("k", "a"), vec![set_body("A")])],
            otherwise: Some(vec![set_body("O")]),
        }]);
        assert!(
            java.ends_with(
                r#"
            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .setBody(constant("A"))
                .otherwise()
                    .setBody(constant("O"))
            .end()
            ;
    }
}
"#
            ),
            "{java}"
        );
    }

    #[test]
    fn two_choices_in_a_row_are_closed_one_after_the_other() {
        let java = http_route(vec![
            RouteStep::Choice {
                whens: vec![(when_is("k", "a"), vec![log("1A")])],
                otherwise: Some(vec![log("1O")]),
            },
            RouteStep::Choice {
                whens: vec![(when_is("j", "x"), vec![log("2X")])],
                otherwise: Some(vec![log("2O")]),
            },
            log("AFTER"),
        ]);
        assert_eq!(
            route_steps(&java),
            r#"            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .log(LoggingLevel.INFO, "1A")
                .otherwise()
                    .log(LoggingLevel.INFO, "1O")
            .end()
            .choice()
                .when(exchange -> "x".equals(exchange.getProperty("j")))
                    .log(LoggingLevel.INFO, "2X")
                .otherwise()
                    .log(LoggingLevel.INFO, "2O")
            .end()
            .log(LoggingLevel.INFO, "AFTER")
"#,
            "{java}"
        );
    }

    /// A choice in a branch of another choice: `.end()` closes the inner
    /// one and `.endChoice()` gives the outer one back, so the `otherwise`
    /// after it is the outer choice's. (Without them it would be a second
    /// `otherwise` of the inner choice, and the application does not start.)
    #[test]
    fn choice_nested_in_a_when_goes_back_to_the_outer_choice() {
        let java = http_route(vec![
            RouteStep::Choice {
                whens: vec![(
                    when_is("k", "a"),
                    vec![RouteStep::Choice {
                        whens: vec![(when_is("j", "x"), vec![log("AX")])],
                        otherwise: Some(vec![log("AO")]),
                    }],
                )],
                otherwise: Some(vec![log("O")]),
            },
            log("AFTER"),
        ]);
        assert_eq!(
            route_steps(&java),
            r#"            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .choice()
                        .when(exchange -> "x".equals(exchange.getProperty("j")))
                            .log(LoggingLevel.INFO, "AX")
                        .otherwise()
                            .log(LoggingLevel.INFO, "AO")
                    .end().endChoice()
                .otherwise()
                    .log(LoggingLevel.INFO, "O")
            .end()
            .log(LoggingLevel.INFO, "AFTER")
"#,
            "{java}"
        );
    }

    /// The steps after the inner choice stay in the branch of the outer
    /// choice that the inner one is in, a `when` or the `otherwise`.
    #[test]
    fn steps_after_a_nested_choice_stay_in_the_branch_of_the_outer_choice() {
        let inner = || RouteStep::Choice {
            whens: vec![(when_is("j", "x"), vec![log("X")])],
            otherwise: None,
        };
        let java = http_route(vec![
            RouteStep::Choice {
                whens: vec![
                    (when_is("k", "a"), vec![log("Apre"), inner(), log("Apost")]),
                    (when_is("k", "b"), vec![log("B")]),
                ],
                otherwise: Some(vec![log("Opre"), inner(), log("Opost")]),
            },
            log("AFTER"),
        ]);
        assert_eq!(
            route_steps(&java),
            r#"            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .log(LoggingLevel.INFO, "Apre")
                    .choice()
                        .when(exchange -> "x".equals(exchange.getProperty("j")))
                            .log(LoggingLevel.INFO, "X")
                    .end().endChoice()
                    .log(LoggingLevel.INFO, "Apost")
                .when(exchange -> "b".equals(exchange.getProperty("k")))
                    .log(LoggingLevel.INFO, "B")
                .otherwise()
                    .log(LoggingLevel.INFO, "Opre")
                    .choice()
                        .when(exchange -> "x".equals(exchange.getProperty("j")))
                            .log(LoggingLevel.INFO, "X")
                    .end().endChoice()
                    .log(LoggingLevel.INFO, "Opost")
            .end()
            .log(LoggingLevel.INFO, "AFTER")
"#,
            "{java}"
        );
    }

    #[test]
    fn choices_three_levels_deep_are_each_closed_at_their_own_level() {
        let java = http_route(vec![
            RouteStep::Choice {
                whens: vec![(
                    when_is("k", "a"),
                    vec![
                        RouteStep::Choice {
                            whens: vec![(
                                when_is("j", "x"),
                                vec![
                                    RouteStep::Choice {
                                        whens: vec![(when_is("i", "1"), vec![log("AX1")])],
                                        otherwise: Some(vec![log("AXO")]),
                                    },
                                    log("AXpost"),
                                ],
                            )],
                            otherwise: Some(vec![log("AO")]),
                        },
                        log("Apost"),
                    ],
                )],
                otherwise: Some(vec![log("O")]),
            },
            log("AFTER"),
        ]);
        assert_eq!(
            route_steps(&java),
            r#"            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .choice()
                        .when(exchange -> "x".equals(exchange.getProperty("j")))
                            .choice()
                                .when(exchange -> "1".equals(exchange.getProperty("i")))
                                    .log(LoggingLevel.INFO, "AX1")
                                .otherwise()
                                    .log(LoggingLevel.INFO, "AXO")
                            .end().endChoice()
                            .log(LoggingLevel.INFO, "AXpost")
                        .otherwise()
                            .log(LoggingLevel.INFO, "AO")
                    .end().endChoice()
                    .log(LoggingLevel.INFO, "Apost")
                .otherwise()
                    .log(LoggingLevel.INFO, "O")
            .end()
            .log(LoggingLevel.INFO, "AFTER")
"#,
            "{java}"
        );
    }

    /// In an exception clause the choice is closed with `.end()` as in a
    /// route: the step after it is a step of the clause, whose own
    /// `.end()` follows. A nested choice goes back to its outer choice.
    #[test]
    fn choice_in_an_exception_clause_is_closed_before_the_next_step() {
        let java = route_java(vec![handler(
            &["java.lang.Exception"],
            true,
            vec![
                RouteStep::Choice {
                    whens: vec![(
                        when_is("k", "a"),
                        vec![RouteStep::Choice {
                            whens: vec![(when_is("j", "x"), vec![log("EAX")])],
                            otherwise: Some(vec![log("EAO")]),
                        }],
                    )],
                    otherwise: Some(vec![log("EO")]),
                },
                log("EAFTER"),
            ],
        )]);
        assert!(
            java.contains(
                r#"
        onException(java.lang.Exception.class)
            .handled(true)
            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .choice()
                        .when(exchange -> "x".equals(exchange.getProperty("j")))
                            .log(LoggingLevel.INFO, "EAX")
                        .otherwise()
                            .log(LoggingLevel.INFO, "EAO")
                    .end().endChoice()
                .otherwise()
                    .log(LoggingLevel.INFO, "EO")
            .end()
            .log(LoggingLevel.INFO, "EAFTER")
            .end();
"#
            ),
            "{java}"
        );
    }

    /// A split is a block as well: after its `.end()` in a branch of a
    /// choice, `.endChoice()` gives the choice back (`.when(...)` and
    /// `.otherwise()` are methods of the choice only). A choice in a split
    /// is closed with `.end()` alone: that goes back to the split, and
    /// `.endChoice()` there would close the split to look for a choice.
    #[test]
    fn split_in_a_choice_branch_goes_back_to_the_choice() {
        let java = http_route(vec![RouteStep::Choice {
            whens: vec![(
                when_is("k", "a"),
                vec![RouteStep::Split {
                    expression: Expression::Simple("${body}".into()),
                    parallel: false,
                    aggregation_strategy: None,
                    steps: vec![
                        RouteStep::Choice {
                            whens: vec![(when_is("j", "x"), vec![log("ITEM X")])],
                            otherwise: Some(vec![log("ITEM O")]),
                        },
                        log("ITEM"),
                    ],
                }],
            )],
            otherwise: Some(vec![log("O")]),
        }]);
        assert_eq!(
            route_steps(&java),
            r#"            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("k")))
                    .split(simple("${body}"))
                        .choice()
                            .when(exchange -> "x".equals(exchange.getProperty("j")))
                                .log(LoggingLevel.INFO, "ITEM X")
                            .otherwise()
                                .log(LoggingLevel.INFO, "ITEM O")
                        .end()
                        .log(LoggingLevel.INFO, "ITEM")
                    .end().endChoice()
                .otherwise()
                    .log(LoggingLevel.INFO, "O")
            .end()
"#,
            "{java}"
        );
    }

    /// The steps of a split whose expression is not converted are written
    /// in place of the split, in the branch the split was in: a choice
    /// among them is a choice in that branch.
    #[test]
    fn choice_behind_an_unconverted_split_in_a_branch_goes_back_to_the_outer_choice() {
        let java = http_route(vec![RouteStep::Choice {
            whens: vec![(
                when_is("k", "a"),
                vec![RouteStep::Split {
                    expression: unconverted("#[payload.items]", "why"),
                    parallel: false,
                    aggregation_strategy: None,
                    steps: vec![RouteStep::Choice {
                        whens: vec![(when_is("j", "x"), vec![log("X")])],
                        otherwise: None,
                    }],
                }],
            )],
            otherwise: Some(vec![log("O")]),
        }]);
        assert!(
            java.contains(
                r#"
                    .throwException(new UnsupportedOperationException("split expression not migrated: #[payload.items]"))
                    .choice()
                        .when(exchange -> "x".equals(exchange.getProperty("j")))
                            .log(LoggingLevel.INFO, "X")
                    .end().endChoice()
                .otherwise()
                    .log(LoggingLevel.INFO, "O")
            .end()
            ;
"#
            ),
            "{java}"
        );
    }
}
