use muleforge_core::{docgen, emitter, mapper, parser, report};
use std::io::Write;
use tempfile::TempDir;

fn make_project(xml: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    let mule_dir = dir.path().join("src/main/mule");
    std::fs::create_dir_all(&mule_dir).unwrap();
    let mut f = std::fs::File::create(mule_dir.join("flows.xml")).unwrap();
    write!(f, "{}", xml).unwrap();
    dir
}

const COMPLEX_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns:db="http://www.mulesoft.org/schema/mule/db"
      xmlns:kafka="http://www.mulesoft.org/schema/mule/kafka"
      xmlns:jms="http://www.mulesoft.org/schema/mule/jms"
      xmlns:file="http://www.mulesoft.org/schema/mule/file"
      xmlns="http://www.mulesoft.org/schema/mule/core">

    <http:listener-config name="HTTP_config">
        <http:listener-connection host="0.0.0.0" port="8081" />
    </http:listener-config>

    <db:config name="DB_config">
        <db:generic-connection url="jdbc:postgresql://localhost:5432/mydb" user="admin" password="secret" />
    </db:config>

    <flow name="api-main-flow">
        <http:listener config-ref="HTTP_config" path="/api" allowedMethods="GET,POST" />
        <logger message="Received request" level="INFO" />
        <choice>
            <when expression="#[payload.type == 'order']">
                <set-payload value="Processing order" />
                <db:select config-ref="DB_config">SELECT * FROM orders WHERE id = :id</db:select>
            </when>
            <otherwise>
                <set-payload value="Unknown type" />
            </otherwise>
        </choice>
        <error-handler>
            <on-error-propagate type="HTTP:CONNECTIVITY">
                <logger message="HTTP connectivity error" level="ERROR" />
            </on-error-propagate>
            <on-error-continue type="VALIDATION:INVALID_PAYLOAD">
                <set-payload value="Invalid payload" />
            </on-error-continue>
        </error-handler>
    </flow>

    <flow name="kafka-consumer-flow">
        <kafka:consumer topic="orders-topic" />
        <set-variable variableName="orderId" value="#[payload.id]" />
        <logger message="Consumed kafka message" level="DEBUG" />
    </flow>

    <flow name="file-reader-flow">
        <file:read directory="/input" />
        <foreach collection="#[payload.lines()]">
            <logger message="Processing line" level="TRACE" />
        </foreach>
    </flow>

    <sub-flow name="common-transform">
        <set-payload value="transformed" />
        <logger message="Transform applied" level="INFO" />
    </sub-flow>

    <flow name="transform-flow">
        <http:listener config-ref="HTTP_config" path="/transform" allowedMethods="POST" />
        <flow-ref name="common-transform" />
    </flow>

    <flow name="scatter-flow">
        <jms:listener destination="input-queue" />
        <scatter-gather>
            <route>
                <jms:publish destination="queue-a" />
            </route>
            <route>
                <jms:publish destination="queue-b" />
            </route>
        </scatter-gather>
    </flow>
</mule>"##;

// ── Parse ──────────────────────────────────────────────────────────────────

#[test]
fn parse_detects_all_flows() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();

    let names: Vec<&str> = project.flows.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"api-main-flow"));
    assert!(names.contains(&"kafka-consumer-flow"));
    assert!(names.contains(&"file-reader-flow"));
    assert!(names.contains(&"transform-flow"));
    assert!(names.contains(&"scatter-flow"));
    assert!(names.contains(&"common-transform"));
}

#[test]
fn parse_identifies_sub_flows() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();

    let sub = project
        .flows
        .iter()
        .find(|f| f.name == "common-transform")
        .unwrap();
    assert!(sub.is_sub_flow);
}

#[test]
fn parse_extracts_config_elements() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();

    assert!(
        !project.configs.is_empty(),
        "expected at least one config element"
    );
    let names: Vec<&str> = project
        .configs
        .iter()
        .filter_map(|c| c.attr("name"))
        .collect();
    assert!(names.contains(&"HTTP_config"));
}

#[test]
fn parse_extracts_error_handlers() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();

    let api_flow = project
        .flows
        .iter()
        .find(|f| f.name == "api-main-flow")
        .unwrap();
    assert_eq!(
        api_flow.error_handlers.len(),
        1,
        "expected one error-handler element"
    );
    assert_eq!(api_flow.error_handlers[0].name, "error-handler");
}

#[test]
fn parse_choice_children() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();

    let api = project
        .flows
        .iter()
        .find(|f| f.name == "api-main-flow")
        .unwrap();
    let choice = api.processors.iter().find(|p| p.name == "choice").unwrap();
    assert!(
        choice.children.len() >= 2,
        "choice should have when + otherwise"
    );
}

// ── Normalize ─────────────────────────────────────────────────────────────

#[test]
fn normalize_inlines_single_use_subflow() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();

    // transform-flow references common-transform exactly once → inlined
    let transform = normalized
        .flows
        .iter()
        .find(|f| f.name == "transform-flow")
        .unwrap();
    let has_flow_ref = transform.processors.iter().any(|p| p.name == "flow-ref");
    assert!(
        !has_flow_ref,
        "flow-ref should be replaced by inlined processors"
    );

    // sub-flow should be removed from the list
    let still_present = normalized
        .flows
        .iter()
        .any(|f| f.name == "common-transform");
    assert!(
        !still_present,
        "inlined sub-flow should be removed from flow list"
    );
}

#[test]
fn normalize_resolves_property_placeholders() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns="http://www.mulesoft.org/schema/mule/core">
    <flow name="props-flow">
        <set-payload value="${greeting}" />
    </flow>
</mule>"##;

    let dir = make_project(xml);
    // Write a properties file
    let resources = dir.path().join("src/main/resources");
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::write(resources.join("application.properties"), "greeting=Hello\n").unwrap();

    let project = parser::parse_project(dir.path()).unwrap();
    assert_eq!(
        project.properties.get("greeting").map(|s| s.as_str()),
        Some("Hello")
    );

    let normalized = parser::normalize(project).unwrap();
    let flow = &normalized.flows[0];
    let payload = &flow.processors[0];
    assert_eq!(
        payload.attr("value"),
        Some("Hello"),
        "property should be resolved"
    );
}

// ── Map ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn map_produces_routes_for_all_flows() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, decisions) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    assert!(!camel_ir.routes.is_empty());
    assert!(!decisions.is_empty());
    assert!(camel_ir
        .maven_dependencies
        .iter()
        .any(|d| d.artifact_id == "camel-quarkus-core"));
}

#[tokio::test]
async fn map_http_listener_becomes_platform_http() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    let api = camel_ir
        .routes
        .iter()
        .find(|r| r.id == "api-main-flow")
        .unwrap();
    match &api.source {
        muleforge_core::ast::camel_ir::RouteEndpoint::Uri(u) => {
            assert!(
                u.starts_with("platform-http:"),
                "expected platform-http source, got {}",
                u
            );
        }
        other => panic!("expected Uri source, got {:?}", other),
    }
}

#[tokio::test]
async fn map_kafka_consumer_includes_kafka_dep() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    assert!(
        camel_ir
            .maven_dependencies
            .iter()
            .any(|d| d.artifact_id == "camel-quarkus-kafka"),
        "kafka route should add camel-quarkus-kafka dependency"
    );
}

#[tokio::test]
async fn map_error_handlers_become_on_exceptions() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    let api = camel_ir
        .routes
        .iter()
        .find(|r| r.id == "api-main-flow")
        .unwrap();
    assert!(
        !api.on_exceptions.is_empty(),
        "error-handler should produce onException entries"
    );
    let propagate = api.on_exceptions.iter().find(|e| !e.handled);
    assert!(
        propagate.is_some(),
        "on-error-propagate should produce handled=false"
    );
    let cont = api.on_exceptions.iter().find(|e| e.handled);
    assert!(
        cont.is_some(),
        "on-error-continue should produce handled=true"
    );
}

// ── Emit: routes ──────────────────────────────────────────────────────────

#[tokio::test]
async fn emit_routes_generates_java_files() {
    let input_dir = make_project(COMPLEX_XML);
    let output_dir = TempDir::new().unwrap();

    let project = parser::parse_project(input_dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    emitter::emit_project(&camel_ir, output_dir.path()).unwrap();

    let routes_dir = output_dir.path().join("src/main/java/generated/routes");
    assert!(routes_dir.is_dir());

    let java_files: Vec<_> = std::fs::read_dir(&routes_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("java"))
        .collect();
    assert!(
        !java_files.is_empty(),
        "should generate at least one Java file"
    );

    for entry in &java_files {
        let content = std::fs::read_to_string(entry.path()).unwrap();
        assert!(
            content.contains("extends RouteBuilder"),
            "{:?} must extend RouteBuilder",
            entry.path()
        );
        assert!(
            content.contains("@ApplicationScoped"),
            "{:?} must be @ApplicationScoped",
            entry.path()
        );
        assert!(
            content.contains("public void configure()"),
            "{:?} must have configure()",
            entry.path()
        );
        assert!(
            content.contains("from("),
            "{:?} must call from()",
            entry.path()
        );
    }
}

#[tokio::test]
async fn emit_routes_generates_pom_and_dockerfile() {
    let input_dir = make_project(COMPLEX_XML);
    let output_dir = TempDir::new().unwrap();

    let project = parser::parse_project(input_dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    emitter::emit_project(&camel_ir, output_dir.path()).unwrap();

    let pom = std::fs::read_to_string(output_dir.path().join("pom.xml")).unwrap();
    assert!(
        pom.contains("quarkus-bom"),
        "pom should reference quarkus-bom"
    );
    assert!(
        pom.contains("camel-quarkus-core"),
        "pom should include camel-quarkus-core"
    );
    assert!(
        pom.contains("quarkus-maven-plugin"),
        "pom should include quarkus-maven-plugin"
    );

    assert!(output_dir.path().join("Dockerfile").exists());
    let dockerfile = std::fs::read_to_string(output_dir.path().join("Dockerfile")).unwrap();
    assert!(dockerfile.contains("eclipse-temurin"));
    assert!(dockerfile.contains("EXPOSE 8080"));
}

// ── Emit: config ──────────────────────────────────────────────────────────

#[tokio::test]
async fn emit_config_generates_application_properties() {
    let input_dir = make_project(COMPLEX_XML);
    let output_dir = TempDir::new().unwrap();

    let project = parser::parse_project(input_dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    emitter::emit_project(&camel_ir, output_dir.path()).unwrap();

    let props_path = output_dir
        .path()
        .join("src/main/resources/application.properties");
    assert!(props_path.exists());
    let props = std::fs::read_to_string(&props_path).unwrap();
    assert!(props.contains("quarkus.http.port=8080"));
    assert!(props.contains("camel.context.name"));

    // COMPLEX_XML has a kafka route → should emit kafka config
    assert!(
        props.contains("camel.component.kafka.brokers"),
        "kafka routes should add kafka broker config"
    );
}

// ── Emit: K8s ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn emit_k8s_generates_all_manifests() {
    let input_dir = make_project(COMPLEX_XML);
    let output_dir = TempDir::new().unwrap();

    let project = parser::parse_project(input_dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    emitter::emit_k8s_manifests(&camel_ir, output_dir.path()).unwrap();

    let base = output_dir.path().join("k8s/base");
    assert!(base.join("deployment.yaml").exists());
    assert!(base.join("service.yaml").exists());
    assert!(base.join("configmap.yaml").exists());
    assert!(base.join("kustomization.yaml").exists());

    let deploy = std::fs::read_to_string(base.join("deployment.yaml")).unwrap();
    assert!(deploy.contains("livenessProbe"));
    assert!(deploy.contains("readinessProbe"));
    assert!(deploy.contains("/q/health/live"));
    assert!(deploy.contains("/q/health/ready"));

    let prod = output_dir.path().join("k8s/overlays/prod");
    assert!(prod.join("hpa.yaml").exists());
    let hpa = std::fs::read_to_string(prod.join("hpa.yaml")).unwrap();
    assert!(hpa.contains("HorizontalPodAutoscaler"));
}

// ── Report ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn report_counts_match_decisions() {
    let dir = make_project(COMPLEX_XML);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (_, decisions) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    let acquired = muleforge_core::git::AcquiredInput {
        working_dir: dir.path().to_path_buf(),
        source_description: "test project".to_string(),
        source_commit: None,
        is_temporary: false,
    };

    let r = report::build(&decisions, &acquired);
    assert_eq!(r.summary.total_elements, decisions.len());
    assert_eq!(
        r.summary.done + r.summary.manual_review + r.summary.skipped,
        r.summary.total_elements
    );
    assert!(r.summary.total_elements > 0);
}

#[tokio::test]
async fn report_writes_markdown_file() {
    let input_dir = make_project(COMPLEX_XML);
    let output_dir = TempDir::new().unwrap();

    let project = parser::parse_project(input_dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (_, decisions) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    let acquired = muleforge_core::git::AcquiredInput {
        working_dir: input_dir.path().to_path_buf(),
        source_description: "test project".to_string(),
        source_commit: Some("abc1234".to_string()),
        is_temporary: false,
    };

    let r = report::build(&decisions, &acquired);
    let report_path = output_dir.path().join("MIGRATION_REPORT.md");
    report::write(&r, &report_path).unwrap();

    assert!(report_path.exists());
    let md = std::fs::read_to_string(&report_path).unwrap();
    assert!(md.contains("# Migration Report"));
    assert!(md.contains("## Summary"));
    assert!(md.contains("## Decisions"));
    assert!(
        md.contains("abc1234"),
        "report should include source commit SHA"
    );
}

// ── Smoke: simple HTTP flow end-to-end ────────────────────────────────────

#[tokio::test]
async fn simple_http_flow_full_pipeline() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns="http://www.mulesoft.org/schema/mule/core">
    <flow name="hello-api">
        <http:listener path="/hello" allowedMethods="GET" />
        <set-payload value="Hello, Camel!" />
        <logger message="Sending response" level="INFO" />
    </flow>
</mule>"##;

    let input_dir = make_project(xml);
    let output_dir = TempDir::new().unwrap();

    let project = parser::parse_project(input_dir.path()).unwrap();
    assert_eq!(project.flows.len(), 1);

    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    assert_eq!(camel_ir.routes.len(), 1);
    assert_eq!(camel_ir.routes[0].id, "hello-api");

    emitter::emit_project(&camel_ir, output_dir.path()).unwrap();

    let route_file = output_dir
        .path()
        .join("src/main/java/generated/routes/HelloApiRoute.java");
    assert!(
        route_file.exists(),
        "HelloApiRoute.java should be generated"
    );

    let java = std::fs::read_to_string(&route_file).unwrap();
    assert!(java.contains("from(\"platform-http:/hello"));
    assert!(java.contains(".routeId(\"hello-api\")"));
    assert!(java.contains(".setBody(constant(\"Hello, Camel!\"))"));
    assert!(java.contains(".log(LoggingLevel.INFO"));
}

// ── Kafka connector 4.x: subscription on the consumer connection ──────────

/// What Kafka connector 4.x actually writes: `<kafka:message-listener>` has
/// only `config-ref`; topic patterns, group ID and bootstrap servers are on
/// the connection inside `<kafka:consumer-config>`.
const KAFKA_4X_XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:kafka="http://www.mulesoft.org/schema/mule/kafka"
      xmlns="http://www.mulesoft.org/schema/mule/core">

    <kafka:consumer-config name="Kafka_Consumer_config">
        <kafka:consumer-plaintext-connection groupId="order-ingest-group">
            <kafka:bootstrap-servers>
                <kafka:bootstrap-server value="${kafka.brokers}" />
            </kafka:bootstrap-servers>
            <kafka:topic-patterns>
                <kafka:topic-pattern value="orders.raw" />
            </kafka:topic-patterns>
        </kafka:consumer-plaintext-connection>
    </kafka:consumer-config>

    <kafka:producer-config name="Kafka_Producer_config">
        <kafka:producer-plaintext-connection>
            <kafka:bootstrap-servers>
                <kafka:bootstrap-server value="${kafka.brokers}" />
            </kafka:bootstrap-servers>
        </kafka:producer-plaintext-connection>
    </kafka:producer-config>

    <flow name="orderIngest">
        <kafka:message-listener config-ref="Kafka_Consumer_config" />
        <logger level="INFO" message="Received order: #[payload]" />
        <kafka:publish config-ref="Kafka_Producer_config" topic="orders.validated" />
    </flow>
</mule>"##;

/// The mapper does not read the connection, so it has no topic for this
/// listener. It must say so (manual review) rather than report the listener
/// as done on a made-up topic — with the shipped rules and without any.
#[tokio::test]
async fn kafka_4x_listener_without_topic_is_flagged_for_manual_review() {
    let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mappings");
    let no_rules = std::path::Path::new("/nonexistent");

    for mappings_dir in [shipped.as_path(), no_rules] {
        let input_dir = make_project(KAFKA_4X_XML);
        let output_dir = TempDir::new().unwrap();

        let project = parser::parse_project(input_dir.path()).unwrap();
        let normalized = parser::normalize(project).unwrap();
        let rules = mapper::load_rules(mappings_dir).unwrap();
        assert_eq!(
            rules.iter().any(|r| r.mule_ns == "kafka"),
            mappings_dir == shipped,
            "shipped mappings should be the only source of kafka rules"
        );
        let (camel_ir, decisions) = mapper::map_project(&normalized, &rules, None)
            .await
            .unwrap();

        assert_eq!(camel_ir.routes.len(), 1);
        match &camel_ir.routes[0].source {
            muleforge_core::ast::camel_ir::RouteEndpoint::Uri(u) => {
                assert_eq!(u, "direct:kafka-message-listener")
            }
            other => panic!("expected Uri source, got {:?}", other),
        }

        let listener: Vec<_> = decisions
            .iter()
            .filter(|d| d.mule_element == "kafka:message-listener")
            .collect();
        assert_eq!(listener.len(), 1);
        assert_eq!(listener[0].status, report::DecisionStatus::ManualReview);
        assert_eq!(listener[0].rule_id, None);

        let acquired = muleforge_core::git::AcquiredInput {
            working_dir: input_dir.path().to_path_buf(),
            source_description: "test project".to_string(),
            source_commit: None,
            is_temporary: false,
        };
        let r = report::build(&decisions, &acquired);
        assert_eq!(r.summary.total_elements, 3);
        assert_eq!(r.summary.manual_review, 1);
        assert_eq!(r.summary.done, 2);

        emitter::emit_project(&camel_ir, output_dir.path()).unwrap();
        let java = std::fs::read_to_string(
            output_dir
                .path()
                .join("src/main/java/generated/routes/OrderIngestRoute.java"),
        )
        .unwrap();
        assert!(java.contains("from(\"direct:kafka-message-listener\")"));
        assert!(
            !java.contains("from(\"kafka:"),
            "no kafka: source may be emitted without a topic:\n{java}"
        );
        // The publish names its topic itself and is still mapped.
        assert!(java.contains(".to(\"kafka:orders.validated"));
    }
}

// ---------------------------------------------------------------------------
// The emitted project compiles for one Java release (`maven.compiler.release`
// in pom.xml). Every JDK the emitted build files pick must be that release:
// an older JDK stops at "release version N not supported".
// ---------------------------------------------------------------------------

#[tokio::test]
async fn emitted_build_files_use_the_java_release_of_the_pom() {
    let input_dir = make_project(COMPLEX_XML);
    let output_dir = TempDir::new().unwrap();
    let out = output_dir.path();

    let project = parser::parse_project(input_dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules = mapper::load_rules(std::path::Path::new("/nonexistent")).unwrap();
    let (camel_ir, _) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();

    emitter::emit_project(&camel_ir, out).unwrap();
    docgen::local_setup::generate(&camel_ir, &out.join("docs")).unwrap();

    let pom = std::fs::read_to_string(out.join("pom.xml")).unwrap();
    let release = pom
        .split("<maven.compiler.release>")
        .nth(1)
        .and_then(|rest| rest.split("</maven.compiler.release>").next())
        .expect("pom sets maven.compiler.release");
    assert!(
        release.parse::<u32>().is_ok(),
        "maven.compiler.release should be a Java release number, got {release:?}"
    );

    let dockerfile = std::fs::read_to_string(out.join("Dockerfile")).unwrap();
    let from_lines: Vec<&str> = dockerfile
        .lines()
        .filter(|l| l.starts_with("FROM "))
        .collect();
    for expected in [
        format!("FROM maven:3.9-eclipse-temurin-{release} AS build"),
        format!("FROM eclipse-temurin:{release}-jre-alpine AS jvm"),
        format!(
            "FROM quay.io/quarkus/ubi-quarkus-mandrel-builder-image:jdk-{release} AS native-build"
        ),
    ] {
        assert!(
            from_lines.contains(&expected.as_str()),
            "Dockerfile should contain `{expected}`, has {from_lines:?}"
        );
    }

    for workflow in ["ci.yaml", "release.yaml"] {
        let yaml = std::fs::read_to_string(out.join(".github/workflows").join(workflow)).unwrap();
        let java_versions: Vec<&str> = yaml
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("java-version:"))
            .collect();
        assert_eq!(
            java_versions,
            [format!("java-version: '{release}'")],
            "{workflow} should set up JDK {release}"
        );
        assert!(
            yaml.contains(&format!("- name: Set up JDK {release}\n")),
            "{workflow} should name the JDK it sets up"
        );
    }

    let local_setup = std::fs::read_to_string(out.join("docs/development/local-setup.md")).unwrap();
    assert!(
        local_setup.contains(&format!("- JDK {release}+ ")),
        "local-setup.md should ask for JDK {release}+:\n{local_setup}"
    );
}

// ── Error handlers: examples/hello-world ──────────────────────────────────

/// `safeFlow` of `examples/hello-world` has
/// `<on-error-continue type="HTTP:CONNECTIVITY">`. The emitted clause used to
/// be `onException(java.net.ConnectException)`, which is not Java. It has to
/// name the class as a literal, state `handled(true)` (on-error-continue),
/// and stay flagged: `ConnectException` is only the nearest class to
/// `HTTP:CONNECTIVITY`.
///
/// The whole `SafeFlowRoute.java` is not a golden file yet, because its
/// `http:` endpoint URI is still wrong; this pins the error handler in it.
#[tokio::test]
async fn hello_world_example_error_handler_is_a_handled_on_exception_clause() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rules = mapper::load_rules(&manifest_dir.join("../mappings")).unwrap();
    let project = parser::parse_project(&manifest_dir.join("../examples/hello-world")).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let (camel_ir, decisions) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();
    let out = TempDir::new().unwrap();
    emitter::emit_project(&camel_ir, out.path()).unwrap();

    let java = std::fs::read_to_string(
        out.path()
            .join("src/main/java/generated/routes/SafeFlowRoute.java"),
    )
    .unwrap();
    let code: Vec<&str> = java
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    let clause_start = code
        .iter()
        .position(|l| l.starts_with("onException("))
        .unwrap_or_else(|| panic!("no onException clause in:\n{java}"));
    assert_eq!(
        code[clause_start..clause_start + 6],
        [
            "onException(java.net.ConnectException.class)",
            ".handled(true)",
            r#".setBody(constant("Service temporarily unavailable"))"#,
            r#".log(LoggingLevel.WARN, "Upstream service unreachable")"#,
            ".end();",
            r#"from("platform-http:/safe?httpMethodRestrict=GET")"#,
        ],
        "{java}"
    );
    assert!(
        java.contains(
            "// TODO: review this handler (on-error-continue type=\"HTTP:CONNECTIVITY\")"
        ),
        "{java}"
    );

    let scope: Vec<_> = decisions
        .iter()
        .filter(|d| d.mule_element == "on-error-continue")
        .collect();
    assert_eq!(scope.len(), 1);
    assert_eq!(scope[0].status, report::DecisionStatus::ManualReview);
}

// ── Error handlers: failures inside a sub-flow ────────────────────────────

/// In Mule a sub-flow has no error handling of its own: what fails in it is
/// handled by the error handler of the flow that references it. The route of
/// a sub-flow therefore has to be without error handler
/// (`.errorHandler(noErrorHandler())`); with Camel's default error handler
/// the `onException` clauses of the calling route would never see the
/// failure. The routes of the flows keep theirs.
#[tokio::test]
async fn sub_flow_route_leaves_its_failures_to_the_calling_flows_error_handler() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns="http://www.mulesoft.org/schema/mule/core">
    <flow name="callerFlow">
        <http:listener config-ref="HTTP_config" path="/caller" allowedMethods="GET" />
        <flow-ref name="shared" />
        <error-handler>
            <on-error-continue>
                <set-payload value="handled by callerFlow" />
            </on-error-continue>
        </error-handler>
    </flow>
    <flow name="otherFlow">
        <http:listener config-ref="HTTP_config" path="/other" allowedMethods="GET" />
        <flow-ref name="shared" />
    </flow>
    <sub-flow name="shared">
        <set-variable variableName="enriched" value="true" />
        <logger message="in shared" level="INFO" />
    </sub-flow>
</mule>"##;
    let dir = make_project(xml);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let (camel_ir, decisions) = mapper::map_project(&normalized, &[], None).await.unwrap();
    let out = TempDir::new().unwrap();
    emitter::emit_project(&camel_ir, out.path()).unwrap();
    let route = |class: &str| {
        std::fs::read_to_string(
            out.path()
                .join(format!("src/main/java/generated/routes/{class}.java")),
        )
        .unwrap()
    };

    let shared = route("SharedRoute");
    assert!(
        shared.contains(
            r#"
        from("direct:shared")
            .routeId("shared")
            .errorHandler(noErrorHandler())
            .setProperty("enriched", constant("true"))
"#
        ),
        "{shared}"
    );

    let caller = route("CallerFlowRoute");
    assert!(
        caller.contains(
            r#"
        onException(java.lang.Exception.class)
            .handled(true)
            .setBody(constant("handled by callerFlow"))
            .end();

        from("platform-http:/caller?httpMethodRestrict=GET")
            .routeId("callerFlow")
            .to("direct:shared")
"#
        ),
        "{caller}"
    );
    for flow_route in [caller, route("OtherFlowRoute")] {
        assert!(!flow_route.contains("errorHandler"), "{flow_route}");
    }

    for element in ["flow-ref", "on-error-continue"] {
        let found: Vec<_> = decisions
            .iter()
            .filter(|d| d.mule_element == element)
            .collect();
        assert!(!found.is_empty(), "{element}");
        for decision in found {
            assert_eq!(
                decision.status,
                report::DecisionStatus::Done,
                "{element}: {}",
                decision.rationale
            );
        }
    }
}

// ── Expressions: choice conditions, set-variable and set-payload values ───

/// Migrate `examples/hello-world` with the shipped rules. Returns the
/// emitted route classes (name -> Java source) and the mapping decisions.
async fn migrate_hello_world_example() -> (
    std::collections::BTreeMap<String, String>,
    Vec<report::MappingDecision>,
) {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rules = mapper::load_rules(&manifest_dir.join("../mappings")).unwrap();
    let project = parser::parse_project(&manifest_dir.join("../examples/hello-world")).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let (camel_ir, decisions) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();
    let out = TempDir::new().unwrap();
    emitter::emit_project(&camel_ir, out.path()).unwrap();

    let mut routes = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(out.path().join("src/main/java/generated/routes")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        routes.insert(name, std::fs::read_to_string(&path).unwrap());
    }
    (routes, decisions)
}

/// The lines of `java` that are code: trimmed, without blank and comment
/// lines.
fn code_lines(java: &str) -> Vec<&str> {
    java.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect()
}

/// `routerFlow` of `examples/hello-world` routes on `#[payload.type == 'A']`.
/// The route used to get `.when(simple("#[payload.type == 'A']"))`, which
/// Camel rejects at startup ("Unexpected token # at location 0"), and the
/// choice was reported as done.
///
/// `payload.type` reads a field of a payload whose type the migrator does
/// not know, so the condition is not converted: the branch stays, with the
/// Mule expression in a TODO and a predicate that fails when it is
/// evaluated (never a guess), and the choice is flagged for review. The
/// otherwise branch is kept as it is.
#[tokio::test]
async fn hello_world_example_choice_conditions_are_flagged_and_fail_loudly() {
    let (routes, decisions) = migrate_hello_world_example().await;
    let java = &routes["RouterFlowRoute"];

    let code = code_lines(java);
    let from = code
        .iter()
        .position(|l| l.starts_with("from("))
        .unwrap_or_else(|| panic!("no from() in:\n{java}"));
    assert_eq!(
        code[from..from + 11],
        [
            r#"from("platform-http:/route?httpMethodRestrict=POST")"#,
            r#".routeId("routerFlow")"#,
            ".choice()",
            r#".when(exchange -> { throw new UnsupportedOperationException("choice condition not migrated: #[payload.type == 'A']"); })"#,
            r#".setBody(constant("Routed to handler A"))"#,
            r#".when(exchange -> { throw new UnsupportedOperationException("choice condition not migrated: #[payload.type == 'B']"); })"#,
            r#".setBody(constant("Routed to handler B"))"#,
            ".otherwise()",
            r#".setBody(constant("Default handler"))"#,
            ".end()",
            ";",
        ],
        "{java}"
    );
    for condition in ["#[payload.type == 'A']", "#[payload.type == 'B']"] {
        assert!(
            java.contains(&format!(
                "// TODO: review this condition, it is not converted (Mule: {condition})"
            )),
            "{java}"
        );
    }

    // One decision for the choice, and it is not "done".
    let choice: Vec<_> = decisions
        .iter()
        .filter(|d| d.mule_element == "choice")
        .collect();
    assert_eq!(choice.len(), 1, "{choice:?}");
    assert_eq!(choice[0].status, report::DecisionStatus::ManualReview);
    assert_eq!(choice[0].rule_id.as_deref(), Some("choice-router"));
    for condition in ["#[payload.type == 'A']", "#[payload.type == 'B']"] {
        assert!(
            choice[0].rationale.contains(condition),
            "{}",
            choice[0].rationale
        );
    }
}

/// `echoFlow` of `examples/hello-world` has
/// `<set-variable variableName="originalPayload" value="#[payload]" />`,
/// which used to be emitted as the literal string `"#[payload]"`. The whole
/// payload is the Camel message body, whatever its type.
#[tokio::test]
async fn hello_world_example_set_variable_takes_the_payload_from_the_body() {
    let (routes, decisions) = migrate_hello_world_example().await;
    let java = &routes["EchoFlowRoute"];

    let code = code_lines(java);
    let from = code
        .iter()
        .position(|l| l.starts_with("from("))
        .unwrap_or_else(|| panic!("no from() in:\n{java}"));
    assert_eq!(
        code[from..from + 3],
        [
            r#"from("platform-http:/echo?httpMethodRestrict=POST")"#,
            r#".routeId("echoFlow")"#,
            r#".setProperty("originalPayload", simple("${body}"))"#,
        ],
        "{java}"
    );

    // No route of the example passes a Mule expression on to Camel.
    for (name, java) in &routes {
        for line in code_lines(java) {
            assert!(
                !line.contains(r##"simple("#["##) && !line.contains(r##"constant("#["##),
                "{name}: {line}"
            );
        }
    }

    // Every set-variable and set-payload of the example is a literal or the
    // whole payload: done, and reported once per element.
    for (element, count) in [("set-variable", 2), ("set-payload", 5)] {
        let found: Vec<_> = decisions
            .iter()
            .filter(|d| d.mule_element == element)
            .collect();
        assert_eq!(found.len(), count, "{element}: {found:?}");
        for decision in found {
            assert_eq!(
                decision.status,
                report::DecisionStatus::Done,
                "{element}: {}",
                decision.rationale
            );
        }
    }
}

/// A comparison of a variable with a literal. Mule 4 (DataWeave) does not
/// convert types for `==`: a variable set with `value="true"` holds the
/// string `"true"`, which is not the boolean `true`. Camel's Simple does
/// convert (`${exchangeProperty.enriched} == true` is true for the string),
/// so the route used to take the `when` branch where Mule takes
/// `otherwise`, and the choice was reported as done.
///
/// The condition is emitted as a Java predicate on the value of the
/// exchange property that does not convert types. It is reported as done
/// only if the application sets the variable from literals and nothing
/// else; a variable that is (also) set from the payload may hold something
/// else than Mule's value, so that choice is flagged, with the predicate
/// still emitted and a TODO in front of it.
#[tokio::test]
async fn variable_comparison_does_not_convert_types_and_says_when_it_rests_on_an_assumption() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns="http://www.mulesoft.org/schema/mule/core">
    <flow name="strictFlow">
        <http:listener config-ref="HTTP_config" path="/strict" allowedMethods="GET" />
        <set-variable variableName="enriched" value="true" />
        <choice>
            <when expression="#[vars.enriched == true]">
                <set-payload value="when" />
            </when>
            <otherwise>
                <set-payload value="otherwise" />
            </otherwise>
        </choice>
    </flow>
    <flow name="countFlow">
        <http:listener config-ref="HTTP_config" path="/count" allowedMethods="GET" />
        <set-variable variableName="retries" value="#[3]" />
        <choice>
            <when expression="#[vars.retries == 3]">
                <set-payload value="three" />
            </when>
            <when expression="#[vars.retries != '3']">
                <set-payload value="not the text" />
            </when>
            <when expression="#[vars.retries == null]">
                <set-payload value="none" />
            </when>
        </choice>
    </flow>
    <flow name="aliasFlow">
        <http:listener config-ref="HTTP_config" path="/alias" allowedMethods="POST" />
        <set-variable variableName="originalPayload" value="#[payload]" />
        <choice>
            <when expression="#[vars.originalPayload == 'A']">
                <set-payload value="when" />
            </when>
            <otherwise>
                <set-payload value="otherwise" />
            </otherwise>
        </choice>
    </flow>
</mule>"##;
    let dir = make_project(xml);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules =
        mapper::load_rules(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mappings"))
            .unwrap();
    let (camel_ir, decisions) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();
    let out = TempDir::new().unwrap();
    emitter::emit_project(&camel_ir, out.path()).unwrap();
    let route = |class: &str| {
        std::fs::read_to_string(
            out.path()
                .join(format!("src/main/java/generated/routes/{class}.java")),
        )
        .unwrap()
    };

    let strict = route("StrictFlowRoute");
    assert!(
        strict.contains(
            r#"
            .setProperty("enriched", constant("true"))
            .choice()
                .when(exchange -> Boolean.TRUE.equals(exchange.getProperty("enriched")))
                    .setBody(constant("when"))
                .otherwise()
                    .setBody(constant("otherwise"))
            .end()
            ;
    }
}
"#
        ),
        "{strict}"
    );

    let count = route("CountFlowRoute");
    assert!(
        count.contains(
            r#"
            .setProperty("retries", constant(3))
            .choice()
                .when(exchange -> isNumber(exchange.getProperty("retries"), 3))
                    .setBody(constant("three"))
                .when(exchange -> !"3".equals(exchange.getProperty("retries")))
                    .setBody(constant("not the text"))
                .when(exchange -> exchange.getProperty("retries") == null)
                    .setBody(constant("none"))
            .end()
            ;
    }

    // Whether value is a number equal to expected, as Mule compares numbers:
"#
        ),
        "{count}"
    );
    assert!(
        count.contains("    private static boolean isNumber(Object value, long expected) {\n"),
        "{count}"
    );

    let alias = route("AliasFlowRoute");
    assert!(
        alias.contains(
            r#"
            .setProperty("originalPayload", simple("${body}"))
            .choice()
                // TODO: review this condition (Mule: #[vars.originalPayload == 'A'])
                // - converted assuming variable originalPayload holds a value this application set from a literal; it is also set at: set-variable in flow aliasFlow from #[payload]. Check the comparison
                .when(exchange -> "A".equals(exchange.getProperty("originalPayload")))
                    .setBody(constant("when"))
                .otherwise()
                    .setBody(constant("otherwise"))
            .end()
"#
        ),
        "{alias}"
    );

    // No comparison is written in Simple, and no class without a number
    // comparison gets the method for it.
    for java in [&strict, &count, &alias] {
        assert!(!java.contains("simple(\"${exchangeProperty"), "{java}");
    }
    for java in [&strict, &alias] {
        assert!(!java.contains("isNumber"), "{java}");
    }

    // One row per choice, in the order of the flows.
    let choices: Vec<_> = decisions
        .iter()
        .filter(|d| d.mule_element == "choice")
        .collect();
    assert_eq!(choices.len(), 3, "{choices:?}");
    for done in &choices[..2] {
        assert_eq!(done.status, report::DecisionStatus::Done, "{done:?}");
        assert_eq!(done.rationale, "choice -> choice");
    }
    let flagged = choices[2];
    assert_eq!(flagged.status, report::DecisionStatus::ManualReview);
    assert_eq!(flagged.rule_id.as_deref(), Some("choice-router"));
    assert_eq!(
        flagged.rationale,
        "choice -> choice; review: 1 of 1 conditions are converted to an exact comparison of the \
         exchange property, which is Mule's result only on an assumption that has to be \
         checked: #[vars.originalPayload == 'A'] (converted assuming variable originalPayload \
         holds a value this application set from a literal; it is also set at: set-variable in \
         flow aliasFlow from #[payload]. Check the comparison)"
    );
}

/// `value="${app.greeting}"` is a property placeholder, which Mule resolves
/// when the application starts. One the migration could not resolve from
/// the properties of the project used to be emitted as the string constant
/// `"${app.greeting}"` and reported as done. It is not converted: the step
/// fails, the report says why and names the Camel replacement. A
/// placeholder that is resolved from the project's properties is still the
/// constant it stands for.
#[tokio::test]
async fn unresolved_property_placeholder_in_a_value_is_not_emitted_as_a_constant() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns="http://www.mulesoft.org/schema/mule/core">
    <flow name="greetFlow">
        <http:listener config-ref="HTTP_config" path="/greet" allowedMethods="GET" />
        <set-variable variableName="greeting" value="${app.greeting}" />
        <set-variable variableName="name" value="${app.name}" />
        <set-payload value="Hello ${app.missing}" />
    </flow>
</mule>"##;
    let dir = make_project(xml);
    let resources = dir.path().join("src/main/resources");
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::write(
        resources.join("application.properties"),
        "app.name=orders\n",
    )
    .unwrap();

    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let (camel_ir, decisions) = mapper::map_project(&normalized, &[], None).await.unwrap();
    let out = TempDir::new().unwrap();
    emitter::emit_project(&camel_ir, out.path()).unwrap();
    let java = std::fs::read_to_string(
        out.path()
            .join("src/main/java/generated/routes/GreetFlowRoute.java"),
    )
    .unwrap();

    assert_eq!(
        code_lines(&java)
            .into_iter()
            .skip_while(|l| !l.starts_with("from("))
            .collect::<Vec<_>>(),
        [
            r#"from("platform-http:/greet?httpMethodRestrict=GET")"#,
            r#".routeId("greetFlow")"#,
            r#".throwException(new UnsupportedOperationException("setProperty(\"greeting\") value not migrated: ${app.greeting}"))"#,
            r#".setProperty("name", constant("orders"))"#,
            r#".throwException(new UnsupportedOperationException("setBody value not migrated: Hello ${app.missing}"))"#,
            ";",
            "}",
            "}",
        ],
        "{java}"
    );
    assert!(
        java.contains(
            r#"// TODO: review this setProperty("greeting"), its value is not converted (Mule: ${app.greeting})"#
        ),
        "{java}"
    );

    let statuses: Vec<_> = decisions
        .iter()
        .filter(|d| d.mule_element.starts_with("set-"))
        .map(|d| (d.mule_element.as_str(), d.status))
        .collect();
    assert_eq!(
        statuses,
        [
            ("set-variable", report::DecisionStatus::ManualReview),
            ("set-variable", report::DecisionStatus::Done),
            ("set-payload", report::DecisionStatus::ManualReview),
        ]
    );
    let greeting = decisions
        .iter()
        .find(|d| d.mule_element == "set-variable")
        .unwrap();
    assert!(
        greeting.rationale.contains("${app.greeting}")
            && greeting.rationale.contains("Mule property placeholder")
            && greeting.rationale.contains("{{key}}"),
        "{}",
        greeting.rationale
    );
}

/// A Mule `<choice>` ends at `</choice>`: what follows it runs whichever
/// branch was taken, also when the choice is in a branch of another choice
/// or in an on-error scope.
///
/// In the Camel Java DSL `.end()` closes a choice. A choice (or a split) in
/// a branch of another choice is followed by `.endChoice()` as well, which
/// gives the outer choice back for the steps, the `.when(...)` and the
/// `.otherwise()` that come after it. The route used to get `.endChoice()`
/// alone, which leaves the choice open: the steps after `</choice>` went
/// into its last branch, and the `otherwise` of an outer choice became a
/// second `otherwise` of the inner one ("Cannot add a 2nd otherwise to this
/// choice", the application does not start).
#[tokio::test]
async fn choice_ends_where_the_mule_choice_ends_at_every_level_of_nesting() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns="http://www.mulesoft.org/schema/mule/core">
    <flow name="nestedFlow">
        <http:listener config-ref="HTTP_config" path="/nested" allowedMethods="GET" />
        <set-variable variableName="kind" value="a" />
        <set-variable variableName="size" value="x" />
        <choice>
            <when expression="#[vars.kind == 'a']">
                <choice>
                    <when expression="#[vars.size == 'x']">
                        <logger level="INFO" message="a, x" />
                    </when>
                    <otherwise>
                        <logger level="INFO" message="a, another size" />
                    </otherwise>
                </choice>
                <logger level="INFO" message="after the choice in when" />
            </when>
            <when expression="#[vars.kind == 'b']">
                <logger level="INFO" message="b" />
            </when>
            <otherwise>
                <choice>
                    <when expression="#[vars.size == 'x']">
                        <choice>
                            <when expression="#[vars.kind == 'c']">
                                <logger level="INFO" message="c, x" />
                            </when>
                            <otherwise>
                                <logger level="INFO" message="another kind, x" />
                            </otherwise>
                        </choice>
                    </when>
                </choice>
                <logger level="INFO" message="after the choice in otherwise" />
            </otherwise>
        </choice>
        <logger level="INFO" message="after the first choice" />
        <choice>
            <when expression="#[vars.size == 'x']">
                <logger level="INFO" message="x" />
            </when>
        </choice>
        <set-payload value="done" />
    </flow>
    <flow name="lastFlow">
        <http:listener config-ref="HTTP_config" path="/last" allowedMethods="GET" />
        <set-variable variableName="kind" value="a" />
        <choice>
            <when expression="#[vars.kind == 'a']">
                <set-payload value="a" />
            </when>
            <otherwise>
                <set-payload value="other" />
            </otherwise>
        </choice>
        <error-handler>
            <on-error-continue>
                <choice>
                    <when expression="#[vars.kind == 'a']">
                        <logger level="INFO" message="a failed" />
                    </when>
                    <otherwise>
                        <logger level="INFO" message="another kind failed" />
                    </otherwise>
                </choice>
                <set-payload value="failed" />
            </on-error-continue>
        </error-handler>
    </flow>
</mule>"##;
    let dir = make_project(xml);
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let rules =
        mapper::load_rules(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../mappings"))
            .unwrap();
    let (camel_ir, decisions) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();
    let out = TempDir::new().unwrap();
    emitter::emit_project(&camel_ir, out.path()).unwrap();
    let route = |class: &str| {
        std::fs::read_to_string(
            out.path()
                .join(format!("src/main/java/generated/routes/{class}.java")),
        )
        .unwrap()
    };

    let nested = route("NestedFlowRoute");
    assert!(
        nested.contains(
            r#"
            .routeId("nestedFlow")
            .setProperty("kind", constant("a"))
            .setProperty("size", constant("x"))
            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("kind")))
                    .choice()
                        .when(exchange -> "x".equals(exchange.getProperty("size")))
                            .log(LoggingLevel.INFO, "a, x")
                        .otherwise()
                            .log(LoggingLevel.INFO, "a, another size")
                    .end().endChoice()
                    .log(LoggingLevel.INFO, "after the choice in when")
                .when(exchange -> "b".equals(exchange.getProperty("kind")))
                    .log(LoggingLevel.INFO, "b")
                .otherwise()
                    .choice()
                        .when(exchange -> "x".equals(exchange.getProperty("size")))
                            .choice()
                                .when(exchange -> "c".equals(exchange.getProperty("kind")))
                                    .log(LoggingLevel.INFO, "c, x")
                                .otherwise()
                                    .log(LoggingLevel.INFO, "another kind, x")
                            .end().endChoice()
                    .end().endChoice()
                    .log(LoggingLevel.INFO, "after the choice in otherwise")
            .end()
            .log(LoggingLevel.INFO, "after the first choice")
            .choice()
                .when(exchange -> "x".equals(exchange.getProperty("size")))
                    .log(LoggingLevel.INFO, "x")
            .end()
            .setBody(constant("done"))
            ;
    }
}
"#
        ),
        "{nested}"
    );

    // A choice as the last step of the route, and one in an on-error scope
    // with a step after it.
    let last = route("LastFlowRoute");
    assert!(
        last.contains(
            r#"
        onException(java.lang.Exception.class)
            .handled(true)
            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("kind")))
                    .log(LoggingLevel.INFO, "a failed")
                .otherwise()
                    .log(LoggingLevel.INFO, "another kind failed")
            .end()
            .setBody(constant("failed"))
            .end();
"#
        ),
        "{last}"
    );
    assert!(
        last.contains(
            r#"
            .setProperty("kind", constant("a"))
            .choice()
                .when(exchange -> "a".equals(exchange.getProperty("kind")))
                    .setBody(constant("a"))
                .otherwise()
                    .setBody(constant("other"))
            .end()
            ;
    }
}
"#
        ),
        "{last}"
    );

    // `.endChoice()` is only written behind the `.end()` of a block in a
    // branch of a choice; no choice is closed with it.
    for java in [&nested, &last] {
        for line in java.lines().filter(|l| l.contains(".endChoice()")) {
            assert_eq!(line.trim(), ".end().endChoice()", "{java}");
        }
    }

    // The closing calls change nothing in what is reported: every choice of
    // these flows compares variables that are only set from literals.
    let choices: Vec<_> = decisions
        .iter()
        .filter(|d| d.mule_element == "choice")
        .collect();
    assert_eq!(choices.len(), 7, "{choices:?}");
    for choice in choices {
        assert_eq!(choice.status, report::DecisionStatus::Done, "{choice:?}");
    }
}

/// A Mule `foreach` iterates its `collection` (the payload if there is
/// none). Its expression used to be handed to Camel as written,
/// `.split(simple("#[payload.items]"))`: Simple takes that as plain text, so
/// the steps ran once, for the text of the Mule expression, and the foreach
/// was reported as done. A scatter-gather was emitted the same way.
///
/// The payload and a variable are converted to the body and the exchange
/// property, which is the same collection only if the value is a Java
/// collection: the split is emitted with a TODO that says so, and the
/// foreach is flagged. Anything else is not converted: the route fails at
/// that step, the steps of the foreach follow it unreached, and the pages
/// about the flow say that it is not migrated.
#[tokio::test]
async fn foreach_collection_is_converted_on_an_assumption_or_fails_loudly() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns="http://www.mulesoft.org/schema/mule/core">
    <flow name="loopFlow">
        <http:listener config-ref="HTTP_config" path="/loop" allowedMethods="POST" />
        <set-variable variableName="items" value="#[payload]" />
        <foreach>
            <logger level="INFO" message="each of the payload" />
        </foreach>
        <foreach collection="#[vars.items]">
            <set-variable variableName="seen" value="yes" />
            <logger level="INFO" message="item #[vars.counter]" />
        </foreach>
        <logger level="INFO" message="after the loops" />
    </flow>
    <flow name="fieldFlow">
        <http:listener config-ref="HTTP_config" path="/field" allowedMethods="POST" />
        <foreach collection="#[payload.items]">
            <logger level="INFO" message="each item" />
        </foreach>
        <foreach batchSize="2">
            <logger level="INFO" message="each batch" />
        </foreach>
        <logger level="INFO" message="after the field loop" />
    </flow>
    <flow name="scatterFlow">
        <http:listener config-ref="HTTP_config" path="/scatter" allowedMethods="POST" />
        <scatter-gather>
            <route>
                <logger level="INFO" message="one" />
            </route>
            <route>
                <logger level="INFO" message="two" />
            </route>
        </scatter-gather>
    </flow>
</mule>"##;
    let dir = make_project(xml);
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rules = mapper::load_rules(&manifest_dir.join("../mappings")).unwrap();
    let project = parser::parse_project(dir.path()).unwrap();
    let normalized = parser::normalize(project).unwrap();
    let (camel_ir, decisions) = mapper::map_project(&normalized, &rules, None)
        .await
        .unwrap();
    let out = TempDir::new().unwrap();
    emitter::emit_project(&camel_ir, out.path()).unwrap();
    let route = |class: &str| {
        std::fs::read_to_string(
            out.path()
                .join(format!("src/main/java/generated/routes/{class}.java")),
        )
        .unwrap()
    };
    // The code of the route: from `from(...)` to the `;` that ends it.
    fn route_code(java: &str) -> Vec<&str> {
        code_lines(java)
            .into_iter()
            .skip_while(|line| !line.starts_with("from("))
            .take_while(|line| *line != "}")
            .collect()
    }

    let (loop_java, field_java, scatter_java) = (
        route("LoopFlowRoute"),
        route("FieldFlowRoute"),
        route("ScatterFlowRoute"),
    );
    for java in [&loop_java, &field_java, &scatter_java] {
        assert!(!java.contains("simple(\"#["), "{java}");
        assert!(!java.contains("#[payload]\")"), "{java}");
    }

    assert_eq!(
        route_code(&loop_java),
        [
            r#"from("platform-http:/loop?httpMethodRestrict=POST")"#,
            r#".routeId("loopFlow")"#,
            r#".setProperty("items", simple("${body}"))"#,
            r#".split(simple("${body}")).stopOnException()"#,
            r#".log(LoggingLevel.INFO, "each of the payload")"#,
            ".end()",
            r#".split(simple("${exchangeProperty.items}")).stopOnException()"#,
            r#".setProperty("seen", constant("yes"))"#,
            r#".log(LoggingLevel.INFO, "item ${exchangeProperty.counter}")"#,
            ".end()",
            r#".log(LoggingLevel.INFO, "after the loops")"#,
            ";",
        ],
        "{loop_java}"
    );
    for todo in [
        "// TODO: review this split (Mule: foreach without collection)",
        "// - converted assuming the body is a Java collection",
        r##"// TODO: review this split (Mule: foreach collection="#[vars.items]")"##,
        "// - converted assuming the exchange property items holds a Java collection",
        "// - the steps set the variable seen: ",
        "// - the steps read vars.counter",
    ] {
        assert!(loop_java.contains(todo), "{todo} is not in {loop_java}");
    }

    assert_eq!(
        route_code(&field_java),
        [
            r#"from("platform-http:/field?httpMethodRestrict=POST")"#,
            r#".routeId("fieldFlow")"#,
            r##".throwException(new UnsupportedOperationException("foreach collection not migrated: #[payload.items]"))"##,
            r#".log(LoggingLevel.INFO, "each item")"#,
            r#".throwException(new UnsupportedOperationException("foreach collection not migrated: payload (the default) with batchSize=\"2\""))"#,
            r#".log(LoggingLevel.INFO, "each batch")"#,
            r#".log(LoggingLevel.INFO, "after the field loop")"#,
            ";",
        ],
        "{field_java}"
    );
    assert!(
        field_java.contains(
            "// TODO: review this foreach, its collection is not converted (Mule: #[payload.items])"
        ) && field_java.contains("// - the steps of the foreach follow it and are not reached"),
        "{field_java}"
    );

    assert_eq!(
        route_code(&scatter_java)[2..],
        [
            r#".throwException(new UnsupportedOperationException("scatter-gather not migrated"))"#,
            ";",
        ],
        "{scatter_java}"
    );
    assert!(
        scatter_java.contains("// TODO: review this scatter-gather, it is not migrated"),
        "{scatter_java}"
    );

    // One row for each foreach and for the scatter-gather, none of them done.
    let rows: Vec<_> = decisions
        .iter()
        .filter(|d| d.mule_element == "foreach" || d.mule_element == "scatter-gather")
        .collect();
    assert_eq!(rows.len(), 5, "{rows:?}");
    for row in &rows {
        assert_eq!(row.status, report::DecisionStatus::ManualReview, "{row:?}");
    }
    assert!(
        rows[0]
            .rationale
            .contains("assuming the body is a Java collection"),
        "{}",
        rows[0].rationale
    );
    assert!(
        rows[1]
            .rationale
            .contains("the steps set the variable seen")
            && rows[1].rationale.contains("the steps read vars.counter"),
        "{}",
        rows[1].rationale
    );
    assert!(
        rows[2]
            .rationale
            .contains("the collection #[payload.items] is not converted"),
        "{}",
        rows[2].rationale
    );
    assert!(
        rows[3].rationale.contains("batchSize"),
        "{}",
        rows[3].rationale
    );

    // The page of the flow and the diagram say what the route does.
    let acquired = muleforge_core::git::AcquiredInput {
        working_dir: dir.path().to_path_buf(),
        source_description: "test project".to_string(),
        source_commit: None,
        is_temporary: false,
    };
    let migration_report = report::build(&decisions, &acquired);
    docgen::flow_pages::generate(&camel_ir, &migration_report, &out.path().join("docs")).unwrap();
    emitter::route_diagram::generate(&camel_ir, out.path()).unwrap();
    let page = |flow: &str| {
        std::fs::read_to_string(out.path().join(format!("docs/flows/{flow}.md"))).unwrap()
    };
    let field_page = page("fieldflow");
    assert_eq!(
        field_page
            .matches(
                "**Not migrated:** split - its value is a Mule expression that is not converted, \
                 the route fails at this step (see the TODO in the route class)"
            )
            .count(),
        2,
        "{field_page}"
    );
    let loop_page = page("loopflow");
    assert_eq!(
        loop_page.matches("Split (to review: ").count(),
        2,
        "{loop_page}"
    );
    let diagram = std::fs::read_to_string(out.path().join("docs/route-diagram.md")).unwrap();
    assert_eq!(
        diagram.matches("NOT MIGRATED: split (fails here)").count(),
        2,
        "{diagram}"
    );
}

/// Every rule `mapper::load_rules` reads from `mappings/*.yaml` has a row in
/// the "Mapping rules" table of `docs/mapping/MATRIX.md`, and that table has
/// no row for a rule that does not exist: a mapping cannot be added, renamed
/// or removed without the matrix saying what it does.
///
/// The test only checks that the row is there. What the row says has to be
/// measured on a migrated application, as described at the top of the matrix.
#[test]
fn every_mapping_rule_has_a_row_in_the_mapping_matrix() {
    use std::collections::BTreeSet;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let rules = mapper::load_rules(&root.join("mappings")).unwrap();
    let loaded: BTreeSet<&str> = rules.iter().map(|rule| rule.id.as_str()).collect();
    assert!(!loaded.is_empty(), "no rule was loaded from mappings/");
    assert_eq!(
        loaded.len(),
        rules.len(),
        "two rules in mappings/ have the same id"
    );

    let matrix = std::fs::read_to_string(root.join("docs/mapping/MATRIX.md")).unwrap();
    let table = matrix
        .split("\n## Mapping rules\n")
        .nth(1)
        .expect("docs/mapping/MATRIX.md has no \"## Mapping rules\" section");
    let table = table.split("\n## ").next().unwrap();
    // A row of the table starts with the rule id in backticks: | `db-select` | ...
    let documented: BTreeSet<&str> = table
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split('`').next())
        .collect();

    let missing: Vec<&&str> = loaded.difference(&documented).collect();
    assert!(
        missing.is_empty(),
        "rules in mappings/*.yaml without a row in the \"Mapping rules\" table of \
         docs/mapping/MATRIX.md: {missing:?}. Migrate an application that uses the element, \
         look at what is emitted and add the row"
    );
    let stale: Vec<&&str> = documented.difference(&loaded).collect();
    assert!(
        stale.is_empty(),
        "rows in the \"Mapping rules\" table of docs/mapping/MATRIX.md for rules that are not \
         in mappings/*.yaml: {stale:?}"
    );
}
