//! Contributing guide for the output project.

use crate::Result;
use std::path::Path;

pub fn generate(docs_dir: &Path) -> Result<()> {
    let md = r#"# Contributing

## Development Setup

See [Local Setup](./development/local-setup.md) for prerequisites and IDE configuration.

## Making Changes

1. Create a feature branch from `main`
2. Make your changes
3. Add or update tests
4. Run `mvn verify` to ensure all tests pass
5. Open a pull request

## Code Style

- Follow standard Java conventions
- Use meaningful variable and method names
- Add Javadoc to public methods
- Keep routes readable — extract complex logic into beans

## Testing Requirements

- All new routes must have at least one smoke test
- Business logic in beans must have unit tests
- Integration tests for external service interactions
"#;

    std::fs::write(docs_dir.join("contributing.md"), md)?;
    Ok(())
}
