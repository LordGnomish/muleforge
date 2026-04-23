//! .env.example generator.
//!
//! Scans the generated application.properties for ${VAR:default} patterns
//! and produces a .env.example file with all required environment variables.

use std::path::Path;

use crate::ast::camel_ir::{CamelProject, RouteEndpoint, RouteStep};
use crate::Result;

pub fn generate(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    let mut vars = Vec::new();

    // Core vars
    vars.push(("JAVA_OPTS", "-Xmx512m -Xms256m", "JVM memory settings"));

    // Detect from routes
    let mut needs_kafka = false;
    let mut needs_jms = false;
    let mut needs_db = false;
    let mut needs_sftp = false;

    for route in &ir.routes {
        check_uri(
            &route.source,
            &mut needs_kafka,
            &mut needs_jms,
            &mut needs_db,
            &mut needs_sftp,
        );
        for step in &route.steps {
            if let RouteStep::ToUri(ref uri) = step {
                let endpoint = RouteEndpoint::Uri(uri.clone());
                check_uri(
                    &endpoint,
                    &mut needs_kafka,
                    &mut needs_jms,
                    &mut needs_db,
                    &mut needs_sftp,
                );
            }
        }
    }

    if needs_kafka {
        vars.push(("KAFKA_BROKERS", "localhost:9092", "Kafka broker addresses"));
        vars.push(("KAFKA_GROUP_ID", "my-app-group", "Kafka consumer group ID"));
    }

    if needs_jms {
        vars.push((
            "JMS_BROKER_URL",
            "tcp://localhost:61616",
            "JMS/ActiveMQ broker URL",
        ));
        vars.push(("JMS_USERNAME", "admin", "JMS broker username"));
        vars.push(("JMS_PASSWORD", "admin", "JMS broker password"));
    }

    if needs_db {
        vars.push((
            "DB_URL",
            "jdbc:postgresql://localhost:5432/mydb",
            "Database JDBC URL",
        ));
        vars.push(("DB_USERNAME", "postgres", "Database username"));
        vars.push(("DB_PASSWORD", "postgres", "Database password"));
    }

    if needs_sftp {
        vars.push(("SFTP_HOST", "localhost", "SFTP server host"));
        vars.push(("SFTP_PORT", "22", "SFTP server port"));
        vars.push(("SFTP_USERNAME", "", "SFTP username"));
        vars.push(("SFTP_PASSWORD", "", "SFTP password"));
    }

    // Build .env.example
    let mut content = String::new();
    content.push_str("# Environment variables for the migrated Camel Quarkus project.\n");
    content.push_str("# Copy this file to .env and fill in your values.\n");
    content.push_str("#\n");
    content.push_str("# cp .env.example .env\n\n");

    for (name, default, comment) in &vars {
        content.push_str(&format!("# {}\n", comment));
        content.push_str(&format!("{}={}\n\n", name, default));
    }

    std::fs::write(output_dir.join(".env.example"), content)?;
    Ok(())
}

fn check_uri(
    endpoint: &RouteEndpoint,
    kafka: &mut bool,
    jms: &mut bool,
    db: &mut bool,
    sftp: &mut bool,
) {
    if let RouteEndpoint::Uri(ref uri) = endpoint {
        if uri.starts_with("kafka:") {
            *kafka = true;
        }
        if uri.starts_with("jms:") {
            *jms = true;
        }
        if uri.contains("sql:") || uri.contains("jdbc:") {
            *db = true;
        }
        if uri.starts_with("sftp:") {
            *sftp = true;
        }
    }
}
