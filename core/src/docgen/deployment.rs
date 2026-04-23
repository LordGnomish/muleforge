//! Deployment documentation.

use crate::ast::camel_ir::CamelProject;
use crate::Result;
use std::path::Path;

pub fn generate(ir: &CamelProject, docs_dir: &Path) -> Result<()> {
    let ops_dir = docs_dir.join("operations");
    std::fs::create_dir_all(&ops_dir)?;

    let md = format!(
        r#"# Deployment

## Container Build

```bash
# JVM image
docker build -t {name}:latest .

# Native image (requires GraalVM)
docker build -t {name}:latest --target native .
```

## Kubernetes

```bash
# Dev environment
kubectl apply -k k8s/overlays/dev/

# Production
kubectl apply -k k8s/overlays/prod/
```

## Configuration

All configuration is via environment variables or Kubernetes ConfigMaps.
See `application.properties` for the full list of configurable values.

## Rolling Update

```bash
kubectl set image deployment/{name} {name}={name}:new-tag
```

## Rollback

```bash
kubectl rollout undo deployment/{name}
```
"#,
        name = ir.name
    );

    std::fs::write(ops_dir.join("deployment.md"), md)?;
    Ok(())
}
