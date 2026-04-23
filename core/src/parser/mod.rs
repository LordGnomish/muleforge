//! Mule XML parser. Reads a Mule 4 project directory into a Mule AST.
//!
//! Uses quick-xml for streaming XML parsing. Every element, attribute, and
//! text node is preserved. Interpretation happens in the mapper.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::ast::mule_ast::{MuleElement, MuleFlow, MuleProject};
use crate::{MuleForgeError, Result};

pub mod normalizer;

/// Parse a Mule 4 project directory.
pub fn parse_project(input_dir: &Path) -> Result<MuleProject> {
    let mule_xml_dir = input_dir.join("src/main/mule");
    if !mule_xml_dir.is_dir() {
        return Err(MuleForgeError::Parse(format!(
            "expected directory {} to exist",
            mule_xml_dir.display()
        )));
    }

    let mut flows = Vec::new();
    let mut configs = Vec::new();
    let xml_files = discover_xml_files(&mule_xml_dir)?;

    for xml_path in xml_files {
        let content = std::fs::read_to_string(&xml_path)?;
        let parsed = parse_xml(&content, &xml_path)?;
        flows.extend(parsed.flows);
        configs.extend(parsed.configs);
    }

    let properties = load_properties(input_dir).unwrap_or_default();

    Ok(MuleProject {
        name: infer_project_name(input_dir),
        flows,
        configs,
        properties,
    })
}

pub fn normalize(project: MuleProject) -> Result<MuleProject> {
    normalizer::normalize(project)
}

// ---------- internals ----------

struct ParsedFile {
    flows: Vec<MuleFlow>,
    configs: Vec<MuleElement>,
}

fn discover_xml_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("xml") {
            out.push(path);
        } else if path.is_dir() {
            out.extend(discover_xml_files(&path)?);
        }
    }
    out.sort();
    Ok(out)
}

/// Extract namespace prefix and local name from a qualified XML name.
/// e.g. "http:listener" -> ("http", "listener"), "set-payload" -> ("", "set-payload")
fn split_ns(qname: &str) -> (String, String) {
    if let Some(idx) = qname.find(':') {
        (qname[..idx].to_string(), qname[idx + 1..].to_string())
    } else {
        (String::new(), qname.to_string())
    }
}

/// Known config element suffixes that identify connector configurations.
const CONFIG_SUFFIXES: &[&str] = &["-config", "_config", "Config"];

fn is_config_element(ns: &str, name: &str) -> bool {
    if !ns.is_empty() {
        CONFIG_SUFFIXES.iter().any(|s| name.ends_with(s))
    } else {
        false
    }
}

fn parse_attributes(start: &BytesStart) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    for attr in start.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        attrs.insert(key, val);
    }
    attrs
}

fn parse_element_tree(
    reader: &mut Reader<&[u8]>,
    start: &BytesStart,
    source: &Path,
) -> Result<MuleElement> {
    let qname = String::from_utf8_lossy(start.name().as_ref()).to_string();
    let (ns, name) = split_ns(&qname);
    let attributes = parse_attributes(start);
    let line = Some(reader.buffer_position() as u32);
    let mut children = Vec::new();
    let mut text = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let child = parse_element_tree(reader, e, source)?;
                children.push(child);
            }
            Ok(Event::Empty(ref e)) => {
                let cqname = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let (cns, cname) = split_ns(&cqname);
                children.push(MuleElement {
                    namespace: cns,
                    name: cname,
                    attributes: parse_attributes(e),
                    children: vec![],
                    text: None,
                    line: Some(reader.buffer_position() as u32),
                    resolved_config: None,
                });
            }
            Ok(Event::Text(ref e)) => {
                let t = e.unescape().unwrap_or_default().to_string();
                if !t.trim().is_empty() {
                    text = Some(t);
                }
            }
            Ok(Event::CData(ref e)) => {
                let t = String::from_utf8_lossy(e.as_ref()).to_string();
                text = Some(t);
            }
            Ok(Event::End(_)) => break,
            Ok(Event::Eof) => {
                return Err(MuleForgeError::Parse(format!(
                    "unexpected EOF parsing element {} in {}",
                    qname,
                    source.display()
                )));
            }
            Err(e) => {
                return Err(MuleForgeError::Parse(format!(
                    "XML error in {}: {}",
                    source.display(),
                    e
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(MuleElement {
        namespace: ns,
        name,
        attributes,
        children,
        text,
        line,
        resolved_config: None,
    })
}

fn parse_xml(content: &str, source: &Path) -> Result<ParsedFile> {
    let mut reader = Reader::from_str(content);
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut flows = Vec::new();
    let mut configs = Vec::new();

    // Skip to the root <mule> element
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let qname = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let (_ns, local) = split_ns(&qname);
                if local == "mule" {
                    // We are inside <mule>; now parse children.
                    break;
                }
            }
            Ok(Event::Eof) => {
                return Ok(ParsedFile {
                    flows: vec![],
                    configs: vec![],
                })
            }
            Err(e) => {
                return Err(MuleForgeError::Parse(format!(
                    "XML error in {}: {}",
                    source.display(),
                    e
                )));
            }
            _ => {}
        }
        buf.clear();
    }
    buf.clear();

    // Parse children of <mule>
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let elem = parse_element_tree(&mut reader, e, source)?;
                let _qn = elem.qualified_name();
                if elem.name == "flow" || elem.name == "sub-flow" {
                    let flow_name = elem.attr("name").unwrap_or("unnamed").to_string();
                    let is_sub = elem.name == "sub-flow";
                    let mut processors = Vec::new();
                    let mut error_handlers = Vec::new();
                    for child in elem.children {
                        if child.name == "error-handler" {
                            error_handlers.push(child);
                        } else {
                            processors.push(child);
                        }
                    }
                    flows.push(MuleFlow {
                        name: flow_name,
                        is_sub_flow: is_sub,
                        source_file: source.to_path_buf(),
                        processors,
                        error_handlers,
                    });
                } else if is_config_element(&elem.namespace, &elem.name) {
                    configs.push(elem);
                }
                // else: skip global elements like <configuration>, xml namespace decls, etc.
            }
            Ok(Event::Empty(ref e)) => {
                let qname = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let (ns, name) = split_ns(&qname);
                let attrs = parse_attributes(e);
                let elem = MuleElement {
                    namespace: ns.clone(),
                    name: name.clone(),
                    attributes: attrs,
                    children: vec![],
                    text: None,
                    line: Some(reader.buffer_position() as u32),
                    resolved_config: None,
                };
                if is_config_element(&ns, &name) {
                    configs.push(elem);
                }
            }
            Ok(Event::End(_)) => break,
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(MuleForgeError::Parse(format!(
                    "XML error in {}: {}",
                    source.display(),
                    e
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(ParsedFile { flows, configs })
}

fn load_properties(input_dir: &Path) -> Result<HashMap<String, String>> {
    let mut props = HashMap::new();
    let resources = input_dir.join("src/main/resources");
    if !resources.is_dir() {
        return Ok(props);
    }
    // Load .properties files
    for entry in std::fs::read_dir(&resources)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("properties") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some(idx) = line.find('=') {
                        let key = line[..idx].trim().to_string();
                        let val = line[idx + 1..].trim().to_string();
                        props.insert(key, val);
                    }
                }
            }
        }
    }
    Ok(props)
}

fn infer_project_name(input_dir: &Path) -> String {
    input_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("migrated-project")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_mule_project(xml_content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let mule_dir = dir.path().join("src/main/mule");
        std::fs::create_dir_all(&mule_dir).unwrap();
        let mut f = std::fs::File::create(mule_dir.join("test-flow.xml")).unwrap();
        write!(f, "{}", xml_content).unwrap();
        dir
    }

    #[test]
    fn test_parse_simple_http_flow() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns="http://www.mulesoft.org/schema/mule/core">

    <http:listener-config name="HTTP_Listener_config">
        <http:listener-connection host="0.0.0.0" port="8081" />
    </http:listener-config>

    <flow name="helloFlow">
        <http:listener config-ref="HTTP_Listener_config" path="/hello" />
        <set-payload value="Hello World" />
    </flow>
</mule>"##;
        let dir = create_mule_project(xml);
        let project = parse_project(dir.path()).unwrap();
        assert_eq!(project.flows.len(), 1);
        assert_eq!(project.flows[0].name, "helloFlow");
        assert_eq!(project.flows[0].processors.len(), 2);
        assert_eq!(project.configs.len(), 1);
    }

    #[test]
    fn test_parse_choice_flow() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<mule xmlns:http="http://www.mulesoft.org/schema/mule/http"
      xmlns="http://www.mulesoft.org/schema/mule/core">
    <flow name="routerFlow">
        <http:listener config-ref="HTTP_config" path="/route" />
        <choice>
            <when expression="#[payload.type == 'A']">
                <set-payload value="Type A" />
            </when>
            <otherwise>
                <set-payload value="Default" />
            </otherwise>
        </choice>
    </flow>
</mule>"##;
        let dir = create_mule_project(xml);
        let project = parse_project(dir.path()).unwrap();
        assert_eq!(project.flows.len(), 1);
        assert_eq!(project.flows[0].name, "routerFlow");
        let choice = &project.flows[0].processors[1];
        assert_eq!(choice.name, "choice");
        assert!(choice.children.len() >= 2);
    }

    #[test]
    fn test_parse_hello_world_example() {
        // Test against the shipped example project
        let example_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples/hello-world");

        if !example_dir.exists() {
            // Skip if example not present (CI without examples)
            return;
        }

        let project = parse_project(&example_dir).unwrap();
        assert_eq!(project.name, "hello-world");

        // Should find flows: helloFlow, echoFlow, routerFlow, safeFlow
        // And sub-flow: enrichPayload
        assert!(
            project.flows.len() >= 4,
            "expected at least 4 flows, got {}",
            project.flows.len()
        );

        let flow_names: Vec<&str> = project.flows.iter().map(|f| f.name.as_str()).collect();
        assert!(flow_names.contains(&"helloFlow"));
        assert!(flow_names.contains(&"echoFlow"));
        assert!(flow_names.contains(&"routerFlow"));
        assert!(flow_names.contains(&"safeFlow"));

        // Should find HTTP listener config
        assert!(!project.configs.is_empty(), "expected at least 1 config");

        // Properties should be loaded
        assert_eq!(
            project.properties.get("app.name").map(|s| s.as_str()),
            Some("hello-world")
        );
    }
}
