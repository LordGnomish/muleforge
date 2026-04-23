//! Routes emitter: generates one Java RouteBuilder per Camel route.

use std::path::Path;

use crate::ast::camel_ir::*;
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
        let classes = eh.exception_classes.join(", ");
        java.push_str(&format!("        onException({})\n", classes));
        if eh.handled {
            java.push_str("            .handled(true)\n");
        }
        for step in &eh.steps {
            emit_step_dsl(&mut java, step, 3);
        }
        java.push_str("            .end();\n\n");
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
            for line in dsl.lines() {
                java.push_str(&format!("{}// {}\n", pad, line));
            }
        }
    }
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

fn generate_pom(ir: &CamelProject) -> String {
    let mut pom = String::new();
    pom.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>com.example</groupId>
"#);
    pom.push_str(&format!("    <artifactId>{}</artifactId>\n", ir.name));
    pom.push_str(
        r#"    <version>1.0.0-SNAPSHOT</version>

    <properties>
        <maven.compiler.release>17</maven.compiler.release>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
        <quarkus.platform.version>3.8.0</quarkus.platform.version>
        <camel-quarkus.version>3.8.0</camel-quarkus.version>
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
    for dep in &ir.maven_dependencies {
        pom.push_str("        <dependency>\n");
        pom.push_str(&format!(
            "            <groupId>{}</groupId>\n",
            dep.group_id
        ));
        pom.push_str(&format!(
            "            <artifactId>{}</artifactId>\n",
            dep.artifact_id
        ));
        if let Some(ref v) = dep.version {
            pom.push_str(&format!("            <version>{}</version>\n", v));
        }
        pom.push_str("        </dependency>\n");
    }
    pom.push_str(
        r#"
        <!-- Test -->
        <dependency>
            <groupId>io.quarkus</groupId>
            <artifactId>quarkus-junit5</artifactId>
            <scope>test</scope>
        </dependency>
        <dependency>
            <groupId>org.apache.camel.quarkus</groupId>
            <artifactId>camel-quarkus-junit5</artifactId>
            <scope>test</scope>
        </dependency>
    </dependencies>

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
                <quarkus.package.type>native</quarkus.package.type>
            </properties>
        </profile>
    </profiles>
</project>
"#,
    );
    pom
}

fn generate_dockerfile(_name: &str) -> String {
    r#"# --- JVM build ---
FROM maven:3.9-eclipse-temurin-17 AS build
WORKDIR /app
COPY pom.xml .
RUN mvn dependency:go-offline -B
COPY src ./src
RUN mvn package -DskipTests -B

# --- JVM runtime ---
FROM eclipse-temurin:17-jre-alpine AS jvm
WORKDIR /app
COPY --from=build /app/target/quarkus-app /app
EXPOSE 8080
ENTRYPOINT ["java", "-jar", "quarkus-run.jar"]

# --- Native build (optional) ---
FROM quay.io/quarkus/ubi-quarkus-mandrel-builder-image:23.1-java17 AS native-build
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
"#
    .to_string()
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
