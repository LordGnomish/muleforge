//! Debugging guide.

use crate::ast::camel_ir::CamelProject;
use crate::Result;
use std::path::Path;

pub fn generate(_ir: &CamelProject, docs_dir: &Path) -> Result<()> {
    let dev_dir = docs_dir.join("development");
    std::fs::create_dir_all(&dev_dir)?;

    let md = r#"# Debugging

## Dev Mode

```bash
mvn quarkus:dev
```

Dev mode supports:
- Hot reload on code changes
- Dev UI at `http://localhost:8080/q/dev/`
- Live Camel route visualization

## Remote Debugging

```bash
mvn quarkus:dev -Ddebug=5005
```

Then attach your IDE debugger to port 5005.

## Camel Route Debugging

Enable verbose Camel logging:
```properties
quarkus.log.category."org.apache.camel".level=DEBUG
quarkus.log.category."org.apache.camel.processor".level=TRACE
```

## Common Issues

### Route Not Starting
- Check `application.properties` for missing required config
- Verify all environment variables are set
- Check `/q/health/ready` for dependency failures

### Message Not Reaching Endpoint
- Enable exchange tracing: `camel.context.message-history=true`
- Add `.log()` steps in the route for visibility
- Check error handler configuration
"#;

    std::fs::write(dev_dir.join("debugging.md"), md)?;
    Ok(())
}
