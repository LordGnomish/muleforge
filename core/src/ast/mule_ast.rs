//! Faithful representation of a Mule 4 application.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuleProject {
    pub name: String,
    pub flows: Vec<MuleFlow>,
    /// Connector-config elements (e.g., <http:listener-config>, <db:config>).
    pub configs: Vec<MuleElement>,
    /// Merged key-value map from all application.properties / yaml files.
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuleFlow {
    pub name: String,
    /// true if this is a <sub-flow>.
    pub is_sub_flow: bool,
    pub source_file: PathBuf,
    /// The ordered list of message processors inside the flow.
    pub processors: Vec<MuleElement>,
    /// <error-handler> children if present.
    pub error_handlers: Vec<MuleElement>,
}

/// Generic Mule element — preserves every attribute and child verbatim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MuleElement {
    /// XML namespace prefix, e.g., "http", "db", "kafka", or empty for core.
    pub namespace: String,
    /// Local element name, e.g., "listener", "select", "publish".
    pub name: String,
    pub attributes: HashMap<String, String>,
    pub children: Vec<MuleElement>,
    /// Text content (CDATA or mixed content); empty if none.
    pub text: Option<String>,
    /// Line number in the source XML (for diagnostics).
    pub line: Option<u32>,
    /// For config-ref resolution: the resolved config element, if any.
    #[serde(skip)]
    pub resolved_config: Option<Box<MuleElement>>,
}

impl MuleElement {
    pub fn qualified_name(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}:{}", self.namespace, self.name)
        }
    }

    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }
}
