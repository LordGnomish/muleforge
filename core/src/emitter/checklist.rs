//! Post-migration checklist generator.
//!
//! After migration, generates a CHECKLIST.md with exact steps the developer
//! needs to follow. No thinking required — just follow the list.

use std::path::Path;

use crate::ast::camel_ir::{CamelProject, RouteEndpoint};
use crate::report::{DecisionStatus, MigrationReport};
use crate::Result;

pub fn generate(ir: &CamelProject, report: &MigrationReport, output_dir: &Path) -> Result<()> {
    let mut md = String::new();
    md.push_str("# Migration Checklist\n\n");
    md.push_str("Follow these steps **in order** to get your migrated project running.\n\n");

    // Step 1: Environment
    md.push_str("## Step 1: Set up environment variables\n\n");
    md.push_str("Copy the `.env.example` file and fill in your values:\n\n");
    md.push_str("```bash\ncp .env.example .env\n# Edit .env with your actual values\n```\n\n");
    md.push_str("Required variables:\n\n");

    let mut env_vars = Vec::new();
    for v in ir.quarkus_properties.values() {
        if v.contains("${") {
            // Extract env var name from ${VAR_NAME:default}
            if let Some(start) = v.find("${") {
                if let Some(end) = v[start..].find('}') {
                    let var_expr = &v[start + 2..start + end];
                    let var_name = var_expr.split(':').next().unwrap_or(var_expr);
                    env_vars.push(var_name.to_string());
                }
            }
        }
    }

    // Detect from route URIs
    for route in &ir.routes {
        if let RouteEndpoint::Uri(ref uri) = route.source {
            if uri.starts_with("kafka:") {
                env_vars.push("KAFKA_BROKERS".into());
                env_vars.push("KAFKA_GROUP_ID".into());
            }
            if uri.starts_with("jms:") {
                env_vars.push("JMS_BROKER_URL".into());
                env_vars.push("JMS_USERNAME".into());
                env_vars.push("JMS_PASSWORD".into());
            }
        }
        for step in &route.steps {
            if let crate::ast::camel_ir::RouteStep::ToUri(ref uri) = step {
                if uri.contains("sql:") || uri.contains("jdbc:") {
                    env_vars.push("DB_URL".into());
                    env_vars.push("DB_USERNAME".into());
                    env_vars.push("DB_PASSWORD".into());
                }
            }
        }
    }

    env_vars.sort();
    env_vars.dedup();
    if env_vars.is_empty() {
        md.push_str("- No environment variables required for basic operation.\n\n");
    } else {
        for var in &env_vars {
            md.push_str(&format!("- [ ] `{}` — fill in `.env`\n", var));
        }
        md.push('\n');
    }

    // Step 2: Build
    md.push_str("## Step 2: Build the project\n\n");
    md.push_str("```bash\nmvn clean package -DskipTests\n```\n\n");
    md.push_str("If the build fails, check the errors — most likely a MANUAL_REVIEW item needs fixing first.\n\n");

    // Step 3: Fix MANUAL_REVIEW items
    let manual_items: Vec<_> = report
        .decisions
        .iter()
        .filter(|d| d.status == DecisionStatus::ManualReview)
        .collect();

    if manual_items.is_empty() {
        md.push_str("## Step 3: Manual review items\n\n");
        md.push_str("**None!** All elements were automatically migrated.\n\n");
    } else {
        md.push_str(&format!(
            "## Step 3: Fix {} manual review item(s)\n\n",
            manual_items.len()
        ));
        md.push_str(
            "These items need your attention. Search for `// TODO:` in the generated code.\n\n",
        );
        for (i, item) in manual_items.iter().enumerate() {
            md.push_str(&format!("### 3.{}: `{}`\n\n", i + 1, item.mule_element));
            md.push_str(&format!("**What:** {}\n\n", item.rationale));
            if let Some(ref flow) = item.flow_name {
                md.push_str(&format!(
                    "**Where:** Look in the route file for flow `{}`\n\n",
                    flow
                ));
            }
            md.push_str(&format!("**File:** `{}`", item.source_file));
            if let Some(line) = item.source_line {
                md.push_str(&format!(" (line {})", line));
            }
            md.push_str("\n\n");
        }
    }

    // Step 4: Run tests
    md.push_str("## Step 4: Run tests\n\n");
    md.push_str("```bash\nmvn verify\n```\n\n");
    md.push_str("Generated smoke tests verify that routes load correctly. ");
    md.push_str("Add your own tests for business logic.\n\n");

    // Step 5: Run locally
    md.push_str("## Step 5: Run locally\n\n");
    md.push_str("```bash\n# Development mode (hot reload)\nmvn quarkus:dev\n\n");
    md.push_str("# Or run the JAR directly\njava -jar target/quarkus-app/quarkus-run.jar\n```\n\n");
    md.push_str("The application starts on **http://localhost:8080**\n\n");

    // Step 6: Test endpoints
    let http_routes: Vec<_> = ir
        .routes
        .iter()
        .filter(|r| matches!(&r.source, RouteEndpoint::Uri(u) if u.starts_with("platform-http:")))
        .collect();

    if !http_routes.is_empty() {
        md.push_str("## Step 6: Test your endpoints\n\n");
        md.push_str("```bash\n");
        for route in &http_routes {
            if let RouteEndpoint::Uri(ref uri) = route.source {
                let path = uri
                    .strip_prefix("platform-http:")
                    .unwrap_or("/")
                    .split('?')
                    .next()
                    .unwrap_or("/");
                let method = uri
                    .split("httpMethodRestrict=")
                    .nth(1)
                    .and_then(|m| m.split('&').next())
                    .unwrap_or("GET");
                if method == "GET" {
                    md.push_str(&format!(
                        "# {}\ncurl http://localhost:8080{}\n\n",
                        route.id, path
                    ));
                } else {
                    md.push_str(&format!("# {}\ncurl -X {} http://localhost:8080{} -H 'Content-Type: application/json' -d '{{}}'\n\n", route.id, method, path));
                }
            }
        }
        md.push_str("```\n\n");
    }

    // Step 7: Deploy
    md.push_str("## Step 7: Deploy (when ready)\n\n");
    md.push_str("```bash\n# Build container\ndocker build -t my-app:latest .\n\n");
    md.push_str("# Deploy to Kubernetes\nkubectl apply -k k8s/overlays/dev/\n```\n\n");

    // Summary
    md.push_str("---\n\n");
    md.push_str(&format!("**Migration summary:** {} elements total, {} auto-migrated, {} need review, {} skipped.\n\n",
        report.summary.total_elements,
        report.summary.done,
        report.summary.manual_review,
        report.summary.skipped,
    ));
    md.push_str("See `MIGRATION_REPORT.md` for the full element-by-element breakdown.\n");

    std::fs::write(output_dir.join("CHECKLIST.md"), md)?;
    Ok(())
}
