# MuleForge

> Transform MuleSoft repositories into Apache Camel Quarkus repositories — with generated documentation, Kubernetes manifests, and API gateway config.

**Status:** 🚧 Early development · MVP in progress

MuleForge is an open-source transformation framework that takes a **MuleSoft 4.x Git repository** as input and produces a **brand-new Camel Quarkus Git repository** as output — containerized, documented, and ready to deploy on Kubernetes, with declarative configuration for modern API gateways like Kong.

## What makes MuleForge different

**Repository in, repository out.** MuleForge is designed around the Git-native workflow developers actually use. Point it at a Mule repo, tell it where the new project should live, and it produces a clean, well-structured Camel Quarkus repository with its own history — commit by commit, if you want to see how the migration was built.

**Documentation is a first-class output.** Mule projects are notoriously under-documented. MuleForge generates a full `/docs` tree for the new project: architecture overview, flow-by-flow reference, operations runbook, local development guide, and CONTRIBUTING. If an LLM provider is configured, the generated docs are far more than stubs — they explain what each route does, why the original Mule flow was shaped a certain way, and how the new Camel Quarkus version differs.

**Transparent transformation.** Every element is traced in a migration report: `DONE`, `MANUAL_REVIEW`, or `SKIPPED`, with a reason and a link back to the source Mule XML. Nothing is hidden.

## How it works

```
┌────────────────────────┐        ┌────────────────────────┐
│  Mule Repository       │        │  Camel Quarkus Repo    │
│  (Git: local or remote)│        │  (Git: new repository) │
│                        │        │                        │
│  - src/main/mule/*.xml │        │  - src/main/java/...   │
│  - DataWeave modules   │  ───►  │  - pom.xml (Quarkus)   │
│  - pom.xml             │        │  - Dockerfile          │
│  - connector configs   │        │  - k8s/ manifests      │
│  - .env / properties   │        │  - kong/ config (opt)  │
│                        │        │  - docs/ (generated)   │
│                        │        │  - README.md           │
│                        │        │  - CONTRIBUTING.md     │
│                        │        │  - MIGRATION_REPORT.md │
└────────────────────────┘        └────────────────────────┘
```

## Quick start

```bash
# Install
npm install -g muleforge

# Simplest form: local Mule repo → local Camel Quarkus repo
muleforge migrate ./my-mule-repo ./my-quarkus-repo

# From a remote Git repo, initializing a fresh output repo
muleforge migrate \
  --from git@github.com:acme/orders-mule.git \
  --to ./orders-quarkus \
  --init-git

# Push the output directly to a new remote
muleforge migrate \
  --from ./my-mule-repo \
  --to ./my-quarkus-repo \
  --init-git \
  --push-to git@github.com:acme/orders-quarkus.git

# Preview without writing anything
muleforge analyze ./my-mule-repo

# Incremental migration: build the output repo commit-by-commit
muleforge migrate ./my-mule-repo ./my-quarkus-repo --incremental-commits
```

## What you get in the output repository

A typical MuleForge-generated project:

```
my-quarkus-repo/
├── README.md                          # Project overview, generated
├── CONTRIBUTING.md                    # How to contribute, generated
├── MIGRATION_REPORT.md                # What was migrated, how, and why
├── LICENSE
├── .gitignore
├── pom.xml                            # Quarkus + Camel BOM, all needed deps
├── Dockerfile                         # Multi-stage, JVM + native variants
├── .github/workflows/                 # CI: build, test, container publish
│   ├── ci.yaml
│   └── release.yaml
├── src/
│   ├── main/
│   │   ├── java/
│   │   │   └── generated/
│   │   │       ├── routes/            # One RouteBuilder per Mule flow
│   │   │       └── beans/             # Generated transformation beans
│   │   └── resources/
│   │       ├── application.properties
│   │       └── application.prod.properties
│   └── test/
│       └── java/
│           └── generated/             # Smoke tests per route
├── k8s/
│   ├── base/
│   │   ├── deployment.yaml
│   │   ├── service.yaml
│   │   ├── configmap.yaml
│   │   └── kustomization.yaml
│   └── overlays/
│       ├── dev/
│       └── prod/
├── kong/                              # optional, with --emit-kong-config
│   └── kong.yaml                      # decK declarative config
└── docs/
    ├── architecture.md                # System overview with diagrams
    ├── flows/                         # One page per migrated flow
    │   ├── hello-flow.md
    │   └── order-ingest.md
    ├── operations/
    │   ├── runbook.md
    │   ├── observability.md
    │   └── deployment.md
    ├── development/
    │   ├── local-setup.md
    │   ├── testing.md
    │   └── debugging.md
    └── migration/
        ├── overview.md                # Summary of what changed vs Mule
        └── gotchas.md                 # Things to watch out for
```

## Migration Scope

MuleForge is a migration **assistant**, not a magic converter, and at this stage most of what it emits needs work before it runs. What follows is measured, not estimated: 98 probe applications, each with one Mule element or one variant of it, were migrated on `main`; for each, the migration report, the emitted route, the build of the emitted project and the start of the application were checked. [docs/mapping/MATRIX.md](./docs/mapping/MATRIX.md) has the row for every element, how it was measured, and the list of known defects.

| Class of the probe | Probes | What it means |
|---|---|---|
| Converted | 14 | A Camel endpoint or step with the values of the Mule element, reported `DONE`. The emitted project built and the application started |
| Converted, to review | 3 | Emitted and working on an assumption that is stated in the route and in the report (`MANUAL_REVIEW`) |
| Wrong, reported `DONE` | 5 | A defect: the application starts, but the step is not what the Mule element says and fails, or works on something else, when it is reached |
| Breaks the build or startup | 11 | A defect: reported `DONE`, and the emitted project does not build or the application does not start |
| Not migrated - fails loudly | 25 | `MANUAL_REVIEW`; the route fails at that step instead of going on with a wrong result |
| Not migrated - flagged only | 40 | `MANUAL_REVIEW`; a `// TODO` comment, and the route goes on without the step (for a source: the route consumes from a `direct:` endpoint that nothing sends to) |

All 98 emitted projects build. 29 of the probes in the two "not migrated" classes are there because the value their mapping rule needs is not an attribute of the element (the statement of a `db:select`, the host and port of a connector config, ...) and is not read yet: such an element is flagged with what is missing, instead of being emitted with a placeholder in its endpoint.

These are counts of probes, not a share of a typical Mule application: the probes weigh every element the same, and no percentage of "automated" migration is derived from them.

### By category

| Category | Converted | Emitted, but wrong or breaking (reported `DONE`) | Flagged `MANUAL_REVIEW` (to review, or not migrated) |
|---|---|---|---|
| HTTP | `http:listener` with `allowedMethods` | | `http:listener` without `allowedMethods` (source not migrated), `http:request` (fails loudly: the rule needs a `url` and a `path`) |
| Messaging | Kafka `consumer`, `listener`, `message-listener` with a `topic`, `publish` | JMS `listener` (does not start), `publish`, `consume`; VM `listener`, `publish`, `consume` (do not start) | AMQP `listener` (source not migrated), `publish` (fails loudly); `kafka:message-listener` of Kafka connector 4.x (no `topic` attribute), Anypoint MQ |
| Database | | | `db:select`, `insert`, `update`, `delete`, `stored-procedure` (fail loudly: the SQL is not read) |
| File I/O | | `file:listener` with `recursive`, `file:write` | `file:listener` without `recursive`, SFTP and FTP `listener` (source not migrated); `file:read`, SFTP and FTP `write`, `read` (fail loudly) |
| Scheduling | | | `scheduler` (source not migrated: neither a fixed frequency nor a cron expression is read) |
| Email | | | `email:listener-imap`, `email:listener-pop3` (source not migrated), `email:send` (fails loudly) |
| Salesforce | | `create`, `update`, `subscribe-topic` (do not start) | `query`, `delete`, `upsert` (fail loudly), `subscribe-topic-listener` |
| SOAP | | | `wsc:consume`, `ws:consumer` (fail loudly) |
| Object Store | | `os:store`, `os:retrieve` (do not start) | |
| Routing | `choice` whose conditions are all converted | | `choice` with other conditions (to review, or failing at the unconverted condition), `scatter-gather` (fails loudly), `first-successful`, `round-robin`, `until-successful`, `async`, `parallel-foreach`, `idempotent-message-validator`, `batch:job` |
| Iteration | | | `foreach`, `split`: a Camel `split` on a stated assumption (to review), or a step that fails loudly |
| Flow control | `flow-ref` to a sub-flow | | `flow-ref` to a flow (fails loudly), `try` |
| Core processors | `set-payload` and `set-variable` with a literal or one of the converted expressions, `logger` | `logger` with `#[payload.field]` in its message (does not start) | `set-payload` / `set-variable` with any other expression (fail loudly), `set-attribute`, `remove-variable`, `remove-attribute`, `parse-template` |
| Error handling | `on-error-continue` / `on-error-propagate` for every error (no `type`, or `ANY`) | | An on-error scope with a known `type` (to review) or with any other `type` (commented out), `error-handler ref`, `raise-error`, `validation:is-true` |
| DataWeave | | | `ee:transform`: a comment, and the payload goes on unchanged |
| Other | | | SAP (`sap:sync-rfc`: there is no SAP rule), custom Java (`java:invoke-static`) |

### DataWeave and LLM

An `ee:transform` is not converted: the route gets a comment, the element is flagged `MANUAL_REVIEW`, and no bean is written for a script in `<ee:message><ee:set-payload>`. The repository has a DataWeave pattern matcher and LLM providers (Claude, OpenAI, Ollama) meant to produce Java beans. Their result was **not measured**: no LLM provider was available, and a request to the core binary with an `llm` block gave the same output as one without.

### The migration report

`MIGRATION_REPORT.md` has a row for every element that was mapped: `Done` or `ManualReview`, the rule id and a rationale. As measured: its `File` and `Flow` columns are empty and its `Line` column is not a line number; the processors inside a scope that is not migrated (`try`, `async`, `batch:job`, ...) have no row; no probe produced a `Skipped` row; and `Done` does not mean that the element works (16 of the 30 probes with a `DONE` class are in one of the two defect classes).

### Not measured

Mule domain projects, API autodiscovery and API Manager integration, CloudHub features, MEL expressions of Mule 3 other than `flowVars.x`, and every connector that is not in the tables of the matrix. Custom Java classes are copied to the output with a `TODO` comment, MUnit suites become JUnit classes whose tests only call `fail(...)`, and API specifications are copied.

## Features

- **Git-native I/O.** Input can be a local path or a remote Git URL. Output can be a fresh local directory or pushed to a new remote.
- **Optional incremental commits.** With `--incremental-commits`, MuleForge writes the output in logical commits: "scaffold project", "add HTTP routes", "add Kafka integration", "add generated docs". You can `git log` the migration itself.
- **Deterministic mapping layer.** Rule-based conversion for well-defined Mule components lives in versioned YAML files. Anyone can add rules.
- **LLM-assisted layer.** Complex DataWeave expressions, custom logic, and all generated documentation use a pluggable LLM backend.
- **Pluggable LLM providers.** Claude (default), OpenAI, Gemini, Azure OpenAI, Ollama (local, offline).
- **Full documentation generation.** Every migrated project comes with architecture docs, per-flow pages, runbooks, local setup, and testing guides.
- **Migration report.** Every element is classified and explained.
- **Kubernetes-native output.** Dockerfile (JVM + native), Deployment, Service, HPA, ConfigMap, with Kustomize overlays for dev/prod.
- **API gateway integration.** Optional Kong Konnect declarative config, so the migrated service is exposed consistently.
- **Safe by default.** Refuses to overwrite a non-empty output directory unless `--force`. Dry-run mode (`muleforge analyze`) shows what would happen.

## LLM configuration

MuleForge uses an LLM for semantics that cannot be safely captured by rules (complex DataWeave) and for documentation generation. Provider is pluggable:

```yaml
# muleforge.config.yaml
llm:
  provider: claude           # claude | openai | gemini | ollama | azure
  model: claude-opus-4-7
  api_key_env: ANTHROPIC_API_KEY
  temperature: 0.0
  fallback:
    provider: ollama
    model: llama3.1:70b
    host: http://localhost:11434

docgen:
  enabled: true
  generate:
    - architecture
    - per-flow
    - runbook
    - local-setup
    - testing
    - migration-overview
  style: technical            # technical | accessible
```

Offline / air-gapped environments: use `ollama` with a locally-hosted model. No data leaves your network.

If LLM is disabled entirely (`--no-llm`), MuleForge still produces working code where rules suffice, and documentation is written as structured stubs with `TODO` markers explaining what a human needs to fill in.

## Scope (MVP)

See [docs/mapping/MATRIX.md](./docs/mapping/MATRIX.md) for the full Mule → Camel mapping coverage. Current scope targets the most common Mule 4 patterns: HTTP, DB, JMS/Kafka, schedulers, file/SFTP, choice/split/aggregate, DataWeave transformations, error handlers.

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full design of the Rust core + TypeScript CLI split and the pipeline.

## Contributing

Contributions welcome. See [CONTRIBUTING.md](./CONTRIBUTING.md). The project is governed openly. Mapping rules live as versioned YAML files under `mappings/` and are the primary contribution surface — no Rust required.

## License

Apache License 2.0 — see [LICENSE](./LICENSE).

## Acknowledgments

MuleForge is not affiliated with or endorsed by MuleSoft, Salesforce, Red Hat, or the Apache Software Foundation. "MuleSoft" is a trademark of Salesforce. "Apache Camel" and "Apache Camel Quarkus" are trademarks of the Apache Software Foundation. "Quarkus" is a trademark of Red Hat. All trademarks are property of their respective owners.
