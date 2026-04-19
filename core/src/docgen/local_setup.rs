//! Local development setup guide.

use crate::ast::camel_ir::CamelProject;
use crate::Result;
use std::path::Path;

pub fn generate(ir: &CamelProject, docs_dir: &Path) -> Result<()> {
    let dev_dir = docs_dir.join("development");
    std::fs::create_dir_all(&dev_dir)?;

    let md = format!(
        r#"# Local Development Setup

## Prerequisites

- JDK 17+ (Temurin recommended)
- Maven 3.9+
- Docker (for container builds)

## Getting Started

```bash
# Clone the repository
git clone <repo-url>
cd {}

# Run in dev mode (hot reload)
mvn quarkus:dev

# The application starts on http://localhost:8080
```

## Environment Variables

Configure these before running:

| Variable | Description | Default |
|----------|-------------|---------|
| `JAVA_OPTS` | JVM options | `-Xmx512m` |

## IDE Setup

### IntelliJ IDEA
1. Import as Maven project
2. Install Quarkus plugin
3. Run/Debug `io.quarkus:quarkus-maven-plugin:dev`

### VS Code
1. Install Extension Pack for Java
2. Install Quarkus extension
3. Use the Quarkus dev command from command palette
"#,
        ir.name
    );

    std::fs::write(dev_dir.join("local-setup.md"), md)?;
    Ok(())
}
