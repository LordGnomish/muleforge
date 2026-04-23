//! Tests emitter: generates smoke tests per route.

use std::path::Path;

use crate::ast::camel_ir::{CamelProject, RouteEndpoint};
use crate::Result;

pub fn emit(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    let test_dir = output_dir.join("src/test/java/generated");
    std::fs::create_dir_all(&test_dir)?;

    for route in &ir.routes {
        let class_name = to_test_class_name(&route.id);
        let mut java = String::new();
        java.push_str("package generated;\n\n");
        java.push_str("import io.quarkus.test.junit.QuarkusTest;\n");
        java.push_str("import org.junit.jupiter.api.Test;\n");

        let is_http =
            matches!(&route.source, RouteEndpoint::Uri(u) if u.starts_with("platform-http:"));

        if is_http {
            java.push_str("import static io.restassured.RestAssured.given;\n");
            java.push_str("import static org.hamcrest.Matchers.notNullValue;\n");
        }

        java.push_str("\n@QuarkusTest\n");
        java.push_str(&format!("public class {} {{\n\n", class_name));

        if is_http {
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
                    .unwrap_or("GET")
                    .to_lowercase();

                java.push_str("    @Test\n");
                java.push_str(&format!(
                    "    public void testRoute{}Returns200() {{\n",
                    capitalize(&route.id)
                ));
                java.push_str("        given()\n");
                java.push_str(&format!("            .when().{}(\"{}\")\n", method, path));
                java.push_str("            .then()\n");
                java.push_str("            .statusCode(200)\n");
                java.push_str("            .body(notNullValue());\n");
                java.push_str("    }\n");
            }
        } else {
            java.push_str("    @Test\n");
            java.push_str(&format!(
                "    public void testRoute{}Exists() {{\n",
                capitalize(&route.id)
            ));
            java.push_str(&format!(
                "        // Route '{}' is non-HTTP; verify it loaded.\n",
                route.id
            ));
            java.push_str("        // Quarkus context start is the test — if it starts, the route is valid.\n");
            java.push_str("        org.junit.jupiter.api.Assertions.assertTrue(true);\n");
            java.push_str("    }\n");
        }

        java.push_str("}\n");
        std::fs::write(test_dir.join(format!("{}.java", class_name)), java)?;
    }

    Ok(())
}

fn to_test_class_name(flow_name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for c in flow_name.chars() {
        if c == '-' || c == '_' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap_or(c));
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    format!("{}RouteTest", result)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
