//! Migration gotchas and known differences.

use crate::report::MigrationReport;
use crate::Result;
use std::path::Path;

pub fn generate(_report: &MigrationReport, docs_dir: &Path) -> Result<()> {
    let mig_dir = docs_dir.join("migration");
    std::fs::create_dir_all(&mig_dir)?;

    let md = r#"# Migration Gotchas

## DataWeave Expressions

DataWeave expressions do not have a direct equivalent in Camel. MuleForge handles them as:
- Simple expressions -> Camel Simple language
- Complex transforms -> Generated Java beans (may need manual review)

## Error Handling

- Mule `on-error-propagate` -> Camel `onException(handled=false)`
- Mule `on-error-continue` -> Camel `onException(handled=true)`
- Mule error types (e.g., `HTTP:CONNECTIVITY`) are mapped to Java exception classes

## Variables vs Properties

- Mule variables -> Camel exchange properties
- Mule session variables -> Not directly supported; use exchange properties

## Connector Config

- Mule connector configs (e.g., `http:listener-config`) are resolved at migration time
- Connection details are extracted into `application.properties` as environment variable references

## Testing

- Mule MUnit tests are not automatically migrated
- Generated smoke tests verify routes load and respond
- Add integration tests manually for business logic validation
"#;

    std::fs::write(mig_dir.join("gotchas.md"), md)?;
    Ok(())
}
