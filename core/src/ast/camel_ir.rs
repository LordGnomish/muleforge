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
    /// e.g., direct:helloFlow (for sub-flows). A sub-flow has no error
    /// handling of its own: such a route without exception handlers is
    /// emitted without error handler, so that its failures are handled by
    /// the route that called it.
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expression {
    /// A string constant: `constant("text")`.
    Constant(String),
    /// A constant that is not a string, written as the Java literal it is:
    /// `constant(42)`, `constant(true)`.
    Literal(String),
    Simple(String),
    Header(String),
    Property(String),
    /// Reference to a generated bean that produces the value.
    BeanRef(String),
    /// A Mule expression that is not converted (see `docs/mapping/MATRIX.md`,
    /// "Expressions"): `original` is the Mule attribute value, `reason` says
    /// why it is left to a person.
    ///
    /// It is never emitted as an expression Camel would evaluate. The
    /// emitter writes the step so that the route compiles and starts, says
    /// in a `TODO` what is missing, and fails when the step is reached: a
    /// choice condition becomes a predicate that throws, a step that needs
    /// the value (`setBody`, `setProperty`, ...) becomes `throwException`.
    Unconverted {
        original: String,
        reason: String,
    },
    /// A Mule condition `vars.name == value` / `vars.name != value` with a
    /// literal value, as an exact comparison of the exchange property
    /// `name`: Mule's `==` does not convert types, so the property equals
    /// the value only if it holds a value of that type (see
    /// [`ComparedValue`]); `negated` is `!=`, the exact opposite.
    ///
    /// The emitter writes it as a Java predicate on the property, not in
    /// Simple, whose `==` converts one side to the type of the other.
    ///
    /// `review` holds what a person has to check before relying on it (for
    /// example that the variable is also set from something that is not a
    /// literal). The predicate is emitted either way; with a note the
    /// emitter puts a `TODO` in front of it, quoting `original`, the Mule
    /// expression as written.
    PropertyEquals {
        original: String,
        name: String,
        value: ComparedValue,
        negated: bool,
        review: Vec<String>,
    },
}

/// The literal an exchange property is compared with in
/// [`Expression::PropertyEquals`], and when the property equals it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparedValue {
    /// The property holds a `java.lang.String` with this text.
    Text(String),
    /// The property holds a `java.lang.Number` of any type with this
    /// value: `Integer` 3, `Long` 3, `Double` 3.0 and `BigDecimal` 3.0 all
    /// equal 3; the string `"3"` does not.
    Integer(i32),
    /// The property holds a `java.lang.Boolean` with this value.
    Boolean(bool),
    /// The property is not set, or set to null.
    Null,
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
