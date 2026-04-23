# MuleForge Architecture

This document describes the internal architecture of MuleForge. For user-facing docs see [README.md](./README.md).

## Design principles

1. **Repository-centric.** Input is a Git repository. Output is a Git repository. Everything else is an implementation detail.
2. **Deterministic first, LLM last.** Every element that can be mapped by rule is mapped by rule. LLMs are reserved for semantic gaps (complex DataWeave) and for documentation generation.
3. **Transparent by design.** Every decision appears in the migration report with a reason. No black-box transformations.
4. **Pluggable LLM backend.** No hard dependency on a single provider. Offline execution via Ollama is a first-class path.
5. **Idempotent.** Running MuleForge twice on the same input produces the same output (given the same LLM seed / temperature 0).
6. **Incremental output.** The generated Camel Quarkus project is always buildable, even if parts of the migration are marked `MANUAL_REVIEW`. Stubs are emitted with `TODO` markers and links back to the source Mule element.

## High-level pipeline

```
┌─────────────────────────────────────────────────────────────────────┐
│                             INPUT                                   │
│  Git repository (local path or remote URL):                         │
│  ├── src/main/mule/*.xml         (flows, configs, sub-flows)        │
│  ├── src/main/resources/dwl/*.dwl (DataWeave modules)               │
│  ├── src/main/resources/*.yaml    (app properties)                  │
│  └── pom.xml                      (dependencies, connector deps)    │
└──────────────────────────────┬──────────────────────────────────────┘
                               │
                    ┌──────────▼──────────┐
                    │    GIT: ACQUIRE     │
                    │  clone / use local  │
                    │  → working tree     │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │       PARSER        │
                    │  quick-xml (Rust)   │
                    │  → raw Mule AST     │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │     NORMALIZER      │
                    │  - resolve imports  │
                    │  - inline sub-flows │
                    │  - resolve prop refs│
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │       MAPPER        │
                    │  Mule AST → Camel IR│
                    │  (rule-based + LLM) │
                    └──────────┬──────────┘
                               │
    ┌──────────────────────────┼──────────────────────────┐
    │                          │                          │
┌───▼────────────┐   ┌─────────▼────────┐    ┌────────────▼────────┐
│ CODE EMITTER   │   │ CONFIG EMITTER   │    │  DOCGEN EMITTER     │
│ → routes       │   │ → app.properties │    │ → README            │
│ → beans        │   │ → pom.xml        │    │ → CONTRIBUTING      │
│ → tests        │   │ → Dockerfile     │    │ → docs/architecture │
│                │   │ → k8s/           │    │ → docs/flows/*      │
│                │   │ → kong/ (opt)    │    │ → docs/operations/* │
│                │   │ → .github/       │    │ → docs/development/*│
│                │   │                  │    │ → docs/migration/*  │
└────────────────┘   └──────────────────┘    └─────────────────────┘
                               │
                    ┌──────────▼──────────┐
                    │ PROJECT ASSEMBLER   │
                    │  - combine outputs  │
                    │  - MIGRATION_REPORT │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │    GIT: EMIT        │
                    │  init / commit(s) / │
                    │  optionally push    │
                    └──────────┬──────────┘
                               │
┌──────────────────────────────▼──────────────────────────────────────┐
│                            OUTPUT                                   │
│  New Git repository, fully populated and committed.                 │
│  `mvn package` works. `docker build` works. `kubectl apply` works.  │
└─────────────────────────────────────────────────────────────────────┘
```

## Components

### 1. Git acquire (`core/src/git/acquire.rs`)

Unifies input sources behind a single interface:

- **Local path:** use as-is (no modifications to the source).
- **Remote URL (`https://`, `git@...`):** shallow-clone into a temporary directory; delete on exit.
- **Local path that is a Git repo:** may record the source commit SHA in the output migration report, for traceability.

The parser consumes a plain directory; it never sees Git.

### 2. Parser (`core/src/parser/`)

- Implementation: Rust, using `quick-xml`.
- Responsibility: Read Mule XML files and produce a **raw AST** preserving every element, attribute, and text node.
- Does not interpret semantics. Lossless.

### 3. Normalizer (`core/src/parser/normalizer.rs`)

- Resolves `<mule:import>` / `<configuration-properties>` / `<sub-flow>` references.
- Inlines sub-flows where unambiguous; otherwise preserves them as named routes.
- Substitutes `${property}` placeholders where their values are resolvable at build time.
- Produces a **normalized Mule AST**.

### 4. AST (`core/src/ast/`)

Two ASTs live here:
- `mule_ast.rs` — faithful representation of the Mule model (flows, message processors, connector configs).
- `camel_ir.rs` — intermediate representation of the target Camel project (routes, endpoints, processors, beans).

The mapper transforms `MuleAst → CamelIR`. This separation allows future emitters (e.g., Camel Spring Boot) to reuse the IR.

### 5. Mapper (`core/src/mapper/`)

- Loads **mapping rules** from `mappings/**/*.yaml`.
- Applies rules in priority order. Rule matches produce CamelIR nodes.
- Unmatched Mule elements are routed to the **LLM fallback** (see §7).
- Emits `MappingDecision` records for the migration report: `DONE` / `MANUAL_REVIEW` / `SKIPPED` with rationale.

Mapping rule schema (YAML):

```yaml
# mappings/connectors/http.yaml
- id: mule.http.listener
  match:
    element: http:listener
  emit:
    camel_component: platform-http
    uri_template: "platform-http:{{path}}"
    methods_attr: allowedMethods
  maven_dependencies:
    - groupId: org.apache.camel.quarkus
      artifactId: camel-quarkus-platform-http
  notes: |
    Mule's http:listener maps to Quarkus platform-http.
    TLS config is emitted as application.properties entries.
```

### 6. Emitters (`core/src/emitter/`)

- `routes_emitter.rs` — CamelIR → Camel Java RouteBuilder source (default) or YAML DSL.
- `dataweave_emitter.rs` — DataWeave → target transformation language (Jolt for pure JSON, Groovy for imperative, Java bean for complex).
- `config_emitter.rs` — `application.properties`, `pom.xml`, `Dockerfile`.
- `k8s_emitter.rs` — `Deployment`, `Service`, `ConfigMap`, optional `HPA` and `Istio VirtualService`, with Kustomize overlays.
- `kong_emitter.rs` — Kong declarative config (decK format) derived from HTTP listeners and their policies.
- `ci_emitter.rs` — GitHub Actions workflows for build, test, container publish.
- `tests_emitter.rs` — Smoke tests per generated route (`@QuarkusTest` + RestAssured where applicable).

### 7. LLM layer (`core/src/llm/`)

- `provider.rs` defines the `LlmProvider` trait:
  ```rust
  #[async_trait]
  pub trait LlmProvider: Send + Sync {
      async fn transform(&self, req: TransformRequest) -> Result<TransformResponse>;
  }
  ```
- Implementations: `claude.rs` (default), `openai.rs`, `gemini.rs`, `azure.rs`, `ollama.rs`.
- All prompts live in `core/src/llm/prompts/` as versioned, reviewable files.
- LLM is called with structured input (DataWeave source + source/target schemas when available) and must return structured output (target code + confidence + explanation).
- Responses below a confidence threshold are emitted as `MANUAL_REVIEW` stubs.

### 8. Docgen (`core/src/docgen/`)

Documentation is a first-class output. The docgen module walks the Camel IR and the migration decisions, then produces structured Markdown for the output repository.

Generator types (each lives in its own file):

- `readme.rs` — Top-level `README.md` for the generated project: what it is, how to build, how to run locally, how to deploy.
- `contributing.rs` — `CONTRIBUTING.md` tailored to the generated project (not to MuleForge itself).
- `architecture.rs` — `docs/architecture.md`: system overview, component map, data flow diagrams (Mermaid).
- `flow_pages.rs` — `docs/flows/<flow-name>.md`: one page per migrated Mule flow, explaining what it does, what changed vs the original Mule XML, and runtime characteristics.
- `runbook.rs` — `docs/operations/runbook.md`: health checks, common failure modes, recovery procedures.
- `observability.rs` — `docs/operations/observability.md`: metrics exposed, trace spans, log format.
- `deployment.rs` — `docs/operations/deployment.md`: K8s, Kong wiring, secrets/config handling.
- `local_setup.rs` — `docs/development/local-setup.md`: prerequisites, how to run locally.
- `testing.rs` — `docs/development/testing.md`: how to run tests, what is covered.
- `debugging.rs` — `docs/development/debugging.md`: logs, remote debug, common issues.
- `migration_overview.rs` — `docs/migration/overview.md`: narrative summary of the Mule-to-Camel transformation.
- `migration_gotchas.rs` — `docs/migration/gotchas.md`: known behavioral differences and things to watch for.

Every generator has two modes:

- **Structured mode (no LLM):** emits a well-formed document skeleton with section headers, IR-derived tables, and `TODO` markers with explanations of what to add.
- **LLM mode:** passes the structured skeleton plus relevant IR + source context to the configured LLM, which fleshes out the prose. Output is validated (Markdown parses, internal links resolve) before being written.

LLM prompts for docgen are versioned under `core/src/llm/prompts/docgen/` and kept separate from code-transformation prompts.

### 9. Project assembler (`core/src/assembler.rs`)

Combines the emitter outputs into the final working tree:

- Creates the output directory structure.
- Writes all generated files.
- Writes `MIGRATION_REPORT.md`.
- Writes `.gitignore`, `LICENSE`, and other static boilerplate.

### 10. Git emit (`core/src/git/emit.rs`)

Takes the final working tree and produces a Git repository:

- **Default:** `git init`, single commit "Initial migration via MuleForge".
- **Incremental:** writes the output in logical commits:
  1. `chore: scaffold Quarkus project (MuleForge)`
  2. `feat: add HTTP routes`
  3. `feat: add Kafka integration`
  4. `feat: add database routes`
  5. `feat: add error handlers`
  6. `docs: generate project documentation`
  7. `ci: add GitHub Actions workflows`
  8. `chore: add migration report`
- **Push:** if `--push-to <remote>` is supplied, adds the remote and pushes the default branch.
- **Do not init:** `--no-git` leaves the directory as a plain folder without initializing Git.

Commit author defaults to `"MuleForge <noreply@muleforge.dev>"`; configurable via `--author` or `muleforge.config.yaml`.

### 11. CLI (`cli/`)

- TypeScript (Node ≥ 20).
- Spawns the Rust core binary via a thin JSON-RPC-over-stdio protocol.
- Owns UX: progress bars, diff preview, interactive prompts (`--interactive` mode asks the user to approve LLM outputs before writing files).
- Handles LLM SDK integration (Anthropic SDK, OpenAI SDK, Ollama client).
- Handles Git operations via `simple-git` where convenient; the Rust core also supports `git2` for path-internal use.

Why split? Rust core gives us deterministic, fast, portable XML → code transformation. TypeScript CLI gives us ergonomic developer experience and a mature LLM SDK ecosystem. They communicate over a narrow, versioned protocol.

## Data flow example

Input Mule flow:

```xml
<flow name="getOrdersFlow">
    <http:listener config-ref="HTTP_Listener_config" path="/orders"/>
    <db:select config-ref="Database_Config">
        <db:sql>SELECT * FROM orders WHERE status = :status</db:sql>
        <db:input-parameters>#[{ status: attributes.queryParams.status }]</db:input-parameters>
    </db:select>
    <ee:transform>
        <ee:message>
            <ee:set-payload><![CDATA[%dw 2.0
                output application/json
                ---
                payload map { id: $.id, total: $.amount }
            ]]></ee:set-payload>
        </ee:message>
    </ee:transform>
</flow>
```

Generated Camel Quarkus route:

```java
// src/main/java/generated/routes/GetOrdersFlowRoute.java
package generated.routes;

import org.apache.camel.builder.RouteBuilder;

public class GetOrdersFlowRoute extends RouteBuilder {
    @Override
    public void configure() {
        from("platform-http:/orders?httpMethodRestrict=GET")
            .routeId("getOrdersFlow")
            .to("sql:SELECT * FROM orders WHERE status = :#status?dataSource=#defaultDS")
            .process("ordersPayloadMapper"); // MuleForge: generated from DataWeave
    }
}
```

Generated documentation page:

```markdown
# Flow: getOrdersFlow

**Source:** migrated from `src/main/mule/orders.xml` (Mule 4)
**Route ID:** `getOrdersFlow`
**Endpoint:** `GET /orders`

## What this flow does

Accepts an HTTP GET request with an optional `status` query parameter...
(LLM-filled prose continues)

## Runtime characteristics
- Input: HTTP GET /orders?status=<value>
- Downstream: PostgreSQL `orders` table
- Output: JSON array of `{ id, total }` objects

## Differences from the original Mule flow
- DataWeave transformation replaced with a Jackson-based Java bean
  (`OrdersPayloadMapper`) — same semantics, validated against the
  DataWeave fixture in `tests/dataweave/orders-map.dwl`.
- Database access now uses Camel's `sql:` component with a managed DataSource
  instead of Mule's `<db:config>`.
```

## Extensibility

Three primary extension points:

1. **Mapping rules** (`mappings/**/*.yaml`) — anyone can contribute new connector or component mappings by adding a YAML rule and test fixtures. No Rust knowledge required.
2. **LLM providers** (`core/src/llm/*.rs`) — implement the `LlmProvider` trait to add a new provider.
3. **Docgen generators** (`core/src/docgen/*.rs`) — add a new documentation page type, register it in `docgen/mod.rs`, ship it.

## Non-goals

- **Not a Mule runtime.** MuleForge does not execute Mule applications.
- **Not a one-click migration.** Non-trivial apps will produce `MANUAL_REVIEW` items. The goal is to reduce migration effort by an order of magnitude, not to eliminate it.
- **Not a DataWeave compiler.** MuleForge translates DataWeave **best-effort** into Camel-compatible transformation code. Equivalence is verified by user-supplied test fixtures, not proved.
- **Not tied to Kubernetes.** K8s output is optional; the generated Quarkus project runs on bare metal, JVM, native image, or any container platform.
- **Not a Git host.** MuleForge produces a Git repository; pushing it to GitHub, GitLab, Bitbucket, or Gitea is a user choice.
