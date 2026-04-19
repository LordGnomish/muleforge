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

MuleForge is a migration **assistant**, not a magic converter. Here is what to expect:

### Fully Automated (~60-65% of typical Mule apps)

| Category | Mule Elements | Camel Equivalent |
|----------|--------------|------------------|
| HTTP | listener, request | platform-http, http |
| Messaging | Kafka consumer/publish, JMS listener/publish, AMQP | kafka, jms, amqp |
| Database | DB select/insert/update/delete, stored procedures | sql, sql-stored |
| File I/O | File/SFTP/FTP listener, read, write | file, sftp, ftp |
| Scheduling | Fixed frequency, cron | timer, quartz |
| Email | IMAP, POP3, SMTP | mail |
| Salesforce | Query, CRUD, upsert, subscribe | salesforce |
| SOAP | Web Service Consumer | cxf-soap |
| Routing | Choice, scatter-gather, round-robin, first-successful | choice, multicast, loadBalance |
| Flow control | Split, foreach, flow-ref, until-successful, try/catch | split, direct, doTry |
| Transforms | Set-payload, set-variable, set-attribute, logger | setBody, setProperty, setHeader, log |
| Error handling | on-error-propagate, on-error-continue, raise-error | onException, throwException |
| Async | Async scope, VM queues | wireTap, seda |

### LLM-Assisted (~15-20% additional)

DataWeave expressions are converted using a three-tier approach:

1. **Pattern matching** (no LLM needed) — identity passthrough, field mapping, type coercion, string concatenation, filter, map, null coalescing
2. **LLM conversion** (Claude, OpenAI, or Ollama) — complex transforms are sent to an LLM which produces Java Processor beans
3. **Structured stubs** (fallback) — if no LLM is configured, generates TODO stubs with the original DataWeave preserved in Javadoc

### Requires Manual Review (~15-25%)

- Custom Java components (Mule-specific Java classes)
- SAP connectors (mapping exists but complex configuration needs manual validation)
- Anypoint MQ (no direct Camel equivalent — consider Kafka or AMQP)
- MUnit tests (test framework is completely different — generated smoke tests replace them)
- API autodiscovery / API Manager integration (platform-specific)
- Complex MEL expressions (legacy Mule 3 artifact)
- Mule domain projects (shared connector configs)
- CloudHub-specific features (persistent queues, worker management)

Every element is classified in `MIGRATION_REPORT.md` as `DONE`, `MANUAL_REVIEW`, or `SKIPPED` with a rationale and link to the source XML.

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
