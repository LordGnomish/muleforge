//! CI emitter: generates GitHub Actions workflows.

use std::path::Path;

use crate::ast::camel_ir::CamelProject;
use crate::Result;

pub fn emit(ir: &CamelProject, output_dir: &Path) -> Result<()> {
    let workflows_dir = output_dir.join(".github/workflows");
    std::fs::create_dir_all(&workflows_dir)?;

    let ci = format!(
        r#"name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up JDK 17
        uses: actions/setup-java@v4
        with:
          java-version: '17'
          distribution: 'temurin'
          cache: maven
      - name: Build
        run: mvn verify -B
      - name: Upload test results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: test-results
          path: target/surefire-reports/

  container:
    needs: build
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build container image
        run: |
          docker build -t {name}:${{{{ github.sha }}}} .
          docker tag {name}:${{{{ github.sha }}}} {name}:latest
"#,
        name = ir.name
    );

    let release = r#"name: Release

on:
  push:
    tags: ['v*']

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Set up JDK 17
        uses: actions/setup-java@v4
        with:
          java-version: '17'
          distribution: 'temurin'
          cache: maven
      - name: Build native
        run: mvn package -Pnative -DskipTests -B
      - name: Create Release
        uses: softprops/action-gh-release@v1
        with:
          files: target/*-runner
"#
    .to_string();

    std::fs::write(workflows_dir.join("ci.yaml"), ci)?;
    std::fs::write(workflows_dir.join("release.yaml"), release)?;
    Ok(())
}
