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

    for step in &route.steps {
        emit_step_dsl(&mut java, step, 3);
    }

    java.push_str("            ;\n");
    java.push_str("    }\n");
    java.push_str("}\n");
    java
}

/// One `onException(...)` clause, the Camel form of a Mule on-error scope.
///
/// The clause is declared on the RouteBuilder ahead of `from(...)`. Camel
/// scopes such a clause to the routes of that RouteBuilder instance, and an
/// emitted RouteBuilder holds exactly one route, so it handles the errors of
/// that flow only, like the flow's `<error-handler>` in Mule.
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

fn expr_to_java(expr: &Expression) -> String {
    match expr {
        Expression::Constant(s) => format!("constant(\"{}\")", escape_java(s)),
        Expression::Simple(s) => format!("simple(\"{}\")", escape_java(s)),
        Expression::Header(s) => format!("header(\"{}\")", s),
        Expression::Property(s) => format!("exchangeProperty(\"{}\")", s),
        Expression::BeanRef(s) => format!("method(\"{}\")", s),
    }
}

fn escape_java(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
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
}
