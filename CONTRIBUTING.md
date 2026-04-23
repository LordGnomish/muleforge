# Contributing to MuleForge

Thanks for your interest. MuleForge is community-governed. There is no single vendor steering the roadmap.

## Ways to contribute

### 1. Mapping rules (easiest entry point, no Rust needed)

Mapping rules live as YAML files under `mappings/`. Each rule describes how a specific Mule element translates into Camel.

To add a rule:

1. Identify the Mule element you want to map (e.g., `<file:read>`).
2. Create or extend a YAML file in `mappings/connectors/` or `mappings/components/`.
3. Add a test fixture under `tests/golden/`:
   - `input.xml` — a minimal Mule snippet using the element.
   - `expected.java` — the Camel RouteBuilder output you expect.
4. Run `cargo test` locally. Open a PR.

### 2. Core (Rust)

The parser, mapper, and emitters live in `core/`. Changes here require:
- Unit tests alongside the change.
- Documentation updates in `ARCHITECTURE.md` if the change affects the pipeline.

Build:
```bash
cd core
cargo build
cargo test
```

### 3. CLI (TypeScript)

The CLI lives in `cli/`. It spawns the Rust core binary and handles user interaction.

```bash
cd cli
npm install
npm run build
npm test
```

### 4. LLM providers

To add a new provider, implement the `LlmProvider` trait in `core/src/llm/`. Include:
- A provider module (`core/src/llm/<name>.rs`).
- CLI integration for credential handling.
- Documentation in `docs/llm-providers.md`.

### 5. DataWeave fixtures

DataWeave semantics are the hardest part of MuleForge. If you encounter a DataWeave expression that MuleForge mishandles, please add it to `tests/dataweave/` as:
- `input.dwl`
- `input-sample.json` (sample input payload)
- `expected-output.json` (what the DW returns)

Even if MuleForge cannot handle it yet, the fixture is valuable: it drives test coverage and prompt tuning.

## Code style

- **Rust:** `cargo fmt` + `cargo clippy -- -D warnings` must pass.
- **TypeScript:** `prettier` + `eslint`. Configs are in the repo.
- **YAML:** 2-space indent, lowercase keys, no trailing whitespace.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/). Examples:
- `feat(mapper): add http:listener rule`
- `fix(parser): handle empty payload elements`
- `docs: clarify LLM fallback behavior`

## Pull requests

- One logical change per PR.
- Tests required for new behavior.
- Link to any related issue.
- Maintainers will review within a reasonable timeframe; be patient and be kind.

## Community norms

- Assume good faith.
- No vendor attacks. We are building an open alternative, not criticizing any company.
- No proprietary Mule code in PRs or issues. Strip secrets, customer data, and business logic before sharing samples.

## Licensing

By contributing, you agree that your contributions will be licensed under the Apache License 2.0, the same license as the project.

## Governance

MuleForge is governed by its maintainers as listed in `MAINTAINERS.md`. Major decisions (breaking changes, roadmap shifts, governance changes) require a documented proposal (issue labeled `proposal`) and a 14-day comment window before merging.

No single contributor, company, or employer controls the project.
