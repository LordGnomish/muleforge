//! Makefile generator.
//!
//! Generates a Makefile with common commands so developers don't have to
//! remember Maven syntax.

use std::path::Path;

use crate::ast::camel_ir::CamelProject;
use crate::Result;

pub fn generate(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    let name = &ir.name;

    let makefile = format!(
        r#".PHONY: dev build test run native docker deploy clean help

## Development
dev:                ## Run in dev mode (hot reload)
	mvn quarkus:dev

build:              ## Build the project
	mvn clean package -DskipTests

test:               ## Run all tests
	mvn verify

run:                ## Run the JAR
	java -jar target/quarkus-app/quarkus-run.jar

## Native
native:             ## Build native binary (requires GraalVM)
	mvn package -Pnative -DskipTests

native-run:         ## Run the native binary
	./target/*-runner

## Container
docker:             ## Build Docker image
	docker build -t {name}:latest .

docker-run:         ## Run in Docker
	docker run -p 8080:8080 --env-file .env {name}:latest

## Kubernetes
deploy-dev:         ## Deploy to dev environment
	kubectl apply -k k8s/overlays/dev/

deploy-prod:        ## Deploy to production
	kubectl apply -k k8s/overlays/prod/

## Maintenance
clean:              ## Clean build artifacts
	mvn clean
	rm -rf target/

deps:               ## Download dependencies
	mvn dependency:go-offline

## Help
help:               ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {{FS = ":.*?## "}}; {{printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}}'

.DEFAULT_GOAL := help
"#
    );

    std::fs::write(output_dir.join("Makefile"), makefile)?;
    Ok(())
}
