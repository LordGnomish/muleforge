//! Intermediate representation of the target Camel Quarkus project.
//!
//! This IR is intentionally decoupled from both the Mule AST and the final
//! emission format (Java / YAML). It allows us to add new emitters without
//! touching the mapper, and new mapping rules without touching emitters.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CamelProject {
    pub name: String,
    pub routes: Vec<CamelRoute>,
    /// Java beans generated for transformations, adapters, etc.
    pub beans: Vec<GeneratedBean>,
    /// Maven dependencies to include in pom.xml.
    pub maven_dependencies: Vec<MavenDependency>,
    /// Entries for application.properties.
    pub quarkus_properties: HashMap<String, String>,
    /// Kong declarative config, if emission is requested.
    pub kong_config: Option<KongConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CamelRoute {
    pub id: String,
    pub source: RouteEndpoint,
    pub steps: Vec<RouteStep>,
    pub on_exceptions: Vec<ExceptionHandler>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RouteEndpoint {
    /// e.g., platform-http:/path?httpMethodRestrict=GET
    Uri(String),
    /// e.g., direct:helloFlow (for sub-flows)
    Direct(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RouteStep {
    SetBody {
        expression: Expression,
    },
    SetHeader {
        name: String,
        expression: Expression,
    },
    SetProperty {
        name: String,
        expression: Expression,
    },
    RemoveProperty {
        name: String,
    },
    Log {
        level: LogLevel,
        message: String,
    },
    ToUri(String),
    ProcessBean(String),
    Choice {
        whens: Vec<(Expression, Vec<RouteStep>)>,
        otherwise: Option<Vec<RouteStep>>,
    },
    Split {
        expression: Expression,
        parallel: bool,
        aggregation_strategy: Option<String>,
        steps: Vec<RouteStep>,
    },
    Unmarshal(DataFormat),
    Marshal(DataFormat),
    Transform {
        bean_ref: String,
    },
    ThrowException {
        class: String,
        message: String,
    },
    /// Raw Camel DSL text — escape hatch for cases the IR does not yet model.
    RawDsl(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    Constant(String),
    Simple(String),
    Header(String),
    Property(String),
    /// Reference to a generated bean that produces the value.
    BeanRef(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataFormat {
    Json,
    Xml,
    JsonJackson,
    XmlJackson,
    Csv,
    Avro,
    Protobuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionHandler {
    pub exception_classes: Vec<String>,
    pub handled: bool,
    pub steps: Vec<RouteStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedBean {
    pub class_name: String,
    pub package: String,
    pub java_source: String,
    /// If the bean was generated from a DataWeave expression, the original source.
    pub origin_dw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MavenDependency {
    pub group_id: String,
    pub artifact_id: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KongConfig {
    pub services: Vec<KongService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KongService {
    pub name: String,
    pub url: String,
    pub routes: Vec<KongRoute>,
    pub plugins: Vec<KongPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KongRoute {
    pub name: String,
    pub paths: Vec<String>,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KongPlugin {
    pub name: String,
    pub config: HashMap<String, serde_json::Value>,
}
