use muleforge_core::{emitter, mapper, parser, report};
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
