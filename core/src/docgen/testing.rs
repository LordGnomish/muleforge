//! Testing guide.

use crate::ast::camel_ir::CamelProject;
use crate::Result;
use std::path::Path;

pub fn generate(_ir: &CamelProject, docs_dir: &Path) -> Result<()> {
    let dev_dir = docs_dir.join("development");
    std::fs::create_dir_all(&dev_dir)?;

    let md = r#"# Testing

## Running Tests

```bash
# All tests
mvn verify

# Unit tests only
mvn test

# Integration tests only
mvn verify -DskipUnitTests

# Single test class
mvn test -Dtest=HelloFlowRouteTest
```

## Test Structure

- `src/test/java/generated/` — Auto-generated smoke tests (one per route)
- Add custom tests alongside generated ones

## Route Testing

Each route has a generated smoke test using `@QuarkusTest`:

- HTTP routes: REST Assured tests verifying status code and response body
- Non-HTTP routes: Context startup tests verifying route loads correctly

## Adding Tests

```java
@QuarkusTest
public class CustomRouteTest {

    @Test
    public void testCustomScenario() {
        given()
            .body("{}")
            .contentType("application/json")
            .when().post("/your-endpoint")
            .then()
            .statusCode(200);
    }
}
```

## Native Testing

```bash
mvn verify -Pnative
```
"#
    .to_string();

    std::fs::write(dev_dir.join("testing.md"), md)?;
    Ok(())
}
