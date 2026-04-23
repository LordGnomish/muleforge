//! MuleForge core binary — reads a JSON migration request from stdin,
//! runs the migration, and writes the result as JSON to stdout.

use std::io::{self, Read};

fn main() {
    // Read JSON request from stdin
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() || input.trim().is_empty() {
        eprintln!("muleforge-core: reads a JSON migration request from stdin.");
        eprintln!("Usage: echo '{{\"method\":\"migrate\",\"params\":{{...}}}}' | muleforge-core");
        eprintln!();
        eprintln!("For interactive use, install the CLI: npm install -g muleforge");
        std::process::exit(1);
    }

    // Parse the request
    let request: serde_json::Value = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to parse JSON request: {}", e);
            std::process::exit(1);
        }
    };

    let method = request
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("migrate");
    let params = request
        .get("params")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));

    // Extract params
    let input_path = params
        .get("input_path")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let output_path = params.get("output_path").and_then(|v| v.as_str());
    let force = params
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match method {
        "analyze" => {
            // Dry-run: parse and report without emitting
            eprintln!("Analyzing: {}", input_path);
            match muleforge_core::parser::parse_project(std::path::Path::new(input_path)) {
                Ok(project) => {
                    let result = serde_json::json!({
                        "flow_count": project.flows.len(),
                        "total_elements": project.flows.iter().map(|f| f.processors.len()).sum::<usize>(),
                        "config_count": project.configs.len(),
                        "property_count": project.properties.len(),
                    });
                    println!("{}", serde_json::to_string(&result).unwrap());
                }
                Err(e) => {
                    eprintln!("Analysis failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "migrate" => {
            let output = output_path.unwrap_or("./output");
            eprintln!("Migrating: {} -> {}", input_path, output);

            // Build runtime and run migration
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            rt.block_on(async {
                let mappings_dir = params
                    .get("mappings_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("./mappings");

                let cfg = muleforge_core::MigrationConfig {
                    input: muleforge_core::git::InputSource::LocalPath(input_path.into()),
                    output_dir: output.into(),
                    mappings_dir: mappings_dir.into(),
                    llm: None, // LLM configured via TS side
                    git: muleforge_core::git::GitEmitOptions::default(),
                    docgen: muleforge_core::docgen::DocgenConfig::default(),
                    emit_kong_config: params
                        .get("emit_kong_config")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    emit_k8s: params
                        .get("emit_k8s")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true),
                    force,
                };

                match muleforge_core::migrate(&cfg).await {
                    Ok(report) => {
                        let result = serde_json::json!({
                            "summary": {
                                "done": report.summary.done,
                                "manual_review": report.summary.manual_review,
                                "skipped": report.summary.skipped,
                                "total_elements": report.summary.total_elements,
                            },
                            "flow_count": report.decisions.iter()
                                .filter_map(|d| d.flow_name.as_ref())
                                .collect::<std::collections::HashSet<_>>()
                                .len(),
                        });
                        println!("{}", serde_json::to_string(&result).unwrap());
                    }
                    Err(e) => {
                        eprintln!("Migration failed: {}", e);
                        std::process::exit(1);
                    }
                }
            });
        }
        _ => {
            eprintln!("Unknown method: {}", method);
            std::process::exit(1);
        }
    }
}
