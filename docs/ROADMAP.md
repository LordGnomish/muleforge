# MuleForge Roadmap

This roadmap is indicative, not a commitment. Priorities are set by maintainer consensus and community input.

## MVP-0 — Hello World (current)

**Goal:** demonstrate the end-to-end pipeline on the simplest possible Mule app.

- [ ] Rust core skeleton with Mule XML parser (`quick-xml`).
- [ ] Mule AST and Camel IR definitions.
- [ ] Mapping rule loader (YAML).
- [ ] Three rules: `http:listener`, `set-payload`, `logger`.
- [ ] Java RouteBuilder emitter.
- [ ] `pom.xml` and `application.properties` emitter.
- [ ] CLI stub: `muleforge migrate <input> --output <output>`.
- [ ] Golden test: `examples/hello-world-mule/` → buildable Camel Quarkus project.
- [ ] Migration report markdown output.

## MVP-1 — Core connectors

**Goal:** cover the 80% of Mule apps built on HTTP + DB + JMS/Kafka.

- [ ] `http:request` rule.
- [ ] `db:select` / `db:insert` / `db:update` rules → `camel-quarkus-jdbc` or `camel-quarkus-sql`.
- [ ] `jms:*` rules → `camel-quarkus-jms` (or AMQP based on connector).
- [ ] `kafka:*` rules → `camel-quarkus-kafka`.
- [ ] `choice` / `when` / `otherwise` → Camel `.choice()`.
- [ ] `logger` with MEL / DW-lite expressions.
- [ ] `set-variable`, `remove-variable`.
- [ ] `flow-ref` → Camel `direct:` routes.
- [ ] Dockerfile emitter.
- [ ] Kubernetes Deployment + Service emitter.

## MVP-2 — DataWeave

**Goal:** handle non-trivial DataWeave expressions.

- [ ] Simple deterministic DW → Jolt spec emitter (pure field mapping).
- [ ] DW complexity classifier (simple / medium / complex).
- [ ] LLM provider trait and Claude implementation.
- [ ] LLM-assisted DW → Java bean emitter with confidence scoring.
- [ ] `--interactive` CLI mode: preview LLM output, approve or reject.
- [ ] DataWeave fixture test suite.

## MVP-3 — Gateway integration

**Goal:** reduce the "what about the API gateway layer" friction.

- [ ] Kong declarative config emitter (decK format).
- [ ] Policy mapping table: Mule API Manager policies → Kong plugins.
- [ ] Optional Istio VirtualService / Gateway emitter.

## Post-MVP

- Additional LLM providers: OpenAI, Gemini, Azure OpenAI, Ollama, Bedrock.
- Scheduler, file, email connectors.
- Salesforce connector (best-effort via LLM).
- SAP connector stubs (JCo integration is environment-specific; emit adapter service skeleton).
- YAML DSL emitter (in addition to Java).
- `muleforge analyze` command — read-only: report what would be migrated without writing files.
- `muleforge diff` command — compare a previous MuleForge run to the current one, show what changed.
- Mule 3 support (currently 4.x only).
- Web UI (optional, separate repo) for visualizing migration reports.

## Explicit non-goals

- Mule runtime emulation.
- Migrating Anypoint Platform artifacts (API Manager, Exchange, Runtime Manager) themselves — only their application code and derived gateway config.
- Providing commercial support. Third parties may offer support; the project itself does not.
