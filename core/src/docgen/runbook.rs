//! Operations runbook.

use crate::ast::camel_ir::CamelProject;
use crate::Result;
use std::path::Path;

pub fn generate(ir: &CamelProject, docs_dir: &Path) -> Result<()> {
    let ops_dir = docs_dir.join("operations");
    std::fs::create_dir_all(&ops_dir)?;

    let md = format!(
        r#"# Operations Runbook

## Service: {}

### Health Checks

- **Liveness:** `GET /q/health/live`
- **Readiness:** `GET /q/health/ready`
- **Full health:** `GET /q/health`

### Metrics

Quarkus exposes Micrometer metrics at `/q/metrics` (Prometheus format).

### Logging

- Default level: INFO
- Override: set `quarkus.log.level` in application.properties or via env var
- Structured JSON logging: add `quarkus-logging-json` dependency

### Common Issues

| Symptom | Likely Cause | Resolution |
|---------|-------------|------------|
| 503 on startup | Dependencies not ready | Check readiness probe; verify DB/broker connectivity |
| OOM killed | Heap too small | Increase `-Xmx` in JAVA_OPTS; check container memory limits |
| Route timeout | Downstream slow | Check downstream service; adjust timeout in application.properties |

### Restart Procedure

```bash
# Kubernetes
kubectl rollout restart deployment/{}

# Docker
docker restart {}
```

### Scaling

```bash
# Manual
kubectl scale deployment/{} --replicas=5

# Auto (HPA configured in k8s/overlays/prod/)
kubectl get hpa {}
```
"#,
        ir.name, ir.name, ir.name, ir.name, ir.name
    );

    std::fs::write(ops_dir.join("runbook.md"), md)?;
    Ok(())
}
