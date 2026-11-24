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
        /// Whether the split stops at the first element whose steps fail
        /// (`stopOnException()`), as a Mule `foreach` does. Without it Camel
        /// runs the steps for every element and fails afterwards.
        stop_on_exception: bool,
        aggregation_strategy: Option<String>,
        steps: Vec<RouteStep>,
        /// The Mule element the split is, and what has to be reviewed
        /// about it.
        source: SplitSource,
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

impl RouteStep {
    /// Whether the step needs a value (`setBody`, `setHeader`,
    /// `setProperty`) or an expression (`split`) that is a Mule expression
    /// which is not converted. Such a step is not emitted as that step: the
    /// route throws there (see [`Expression::Unconverted`]), and whatever
    /// describes the route has to say so.
    pub fn is_unconverted(&self) -> bool {
        matches!(
            self,
            RouteStep::SetBody { expression }
                | RouteStep::SetHeader { expression, .. }
                | RouteStep::SetProperty { expression, .. }
                | RouteStep::Split { expression, .. }
                if matches!(expression, Expression::Unconverted { .. })
        )
    }
}

/// What a [`RouteStep::Split`] is in the Mule application: the element, the
/// attribute that says what it iterates, and what a person has to check
/// before relying on the split. The emitter names the element in the
/// comments and messages about the split, and writes the notes in a `TODO`
/// in front of it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SplitSource {
    /// The Mule element: `foreach`, `split`.
    pub element: String,
    /// The attribute with the expression: `collection` of a `foreach`.
    pub attribute: String,
    /// That attribute as it is written. `None` if the element does not
    /// have it: a `foreach` then iterates the payload.
    pub original: Option<String>,
    /// What the split rests on (the assumption its expression is converted
    /// on) and what the steps in it rely on that a Camel split does not do
    /// as the Mule element does.
    pub review: Vec<String>,
}

impl SplitSource {
    /// The Mule element, `split` if it is not known.
    pub fn element_name(&self) -> &str {
        if self.element.is_empty() {
            "split"
        } else {
            &self.element
        }
    }

    /// The attribute with the expression, `expression` if it is not known.
    pub fn attribute_name(&self) -> &str {
        if self.attribute.is_empty() {
            "expression"
        } else {
            &self.attribute
        }
    }

    /// The element as it is written in the Mule application:
    /// `foreach collection="#[vars.items]"`, `foreach without collection`.
    pub fn describe(&self) -> String {
        let (element, attribute) = (self.element_name(), self.attribute_name());
        match &self.original {
            Some(original) => format!("{element} {attribute}=\"{original}\""),
            None => format!("{element} without {attribute}"),
        }
    }
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
    /// the value (`setBody`, `setProperty`, ...) becomes `throwException`,
    /// and so does a split whose collection it is: the steps of the split
    /// follow that step, where they are not reached.
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
