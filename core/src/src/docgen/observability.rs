//! Observability documentation.

use crate::ast::camel_ir::CamelProject;
use crate::Result;
use std::path::Path;

pub fn generate(_ir: &CamelProject, docs_dir: &Path) -> Result<()> {
    let ops_dir = docs_dir.join("operations");
    std::fs::create_dir_all(&ops_dir)?;

    let md = r#"# Observability

## Metrics

Quarkus exposes Micrometer metrics at `/q/metrics` in Prometheus format.

Key metrics:
- `camel_exchanges_total` — total exchanges processed per route
- `camel_exchanges_failed_total` — failed exchanges per route
- `camel_exchange_processing_time` — processing time histogram

## Logging

Default: JBoss Logging with console output.

```properties
# Structured JSON logging
quarkus.log.console.json=true

# Per-category levels
quarkus.log.category."org.apache.camel".level=DEBUG
```

## Tracing

Add OpenTelemetry for distributed tracing:

```xml
<dependency>
    <groupId>io.quarkus</groupId>
    <artifactId>quarkus-opentelemetry</artifactId>
</dependency>
```

Configure the exporter:
```properties
quarkus.otel.exporter.otlp.endpoint=http://jaeger:4317
```

## Health Checks

- `/q/health/live` — liveness (is the JVM running?)
- `/q/health/ready` — readiness (are dependencies connected?)
- `/q/health` — combined
"#;

    std::fs::write(ops_dir.join("observability.md"), md)?;
    Ok(())
}
