# MuleForge Usage Guide

## Installation

### Prerequisites

- **Rust 1.75+** (for building the core engine)
- **Node.js 20+** (for the CLI)
- **Git** (for repository operations)

### Build from source

```bash
# Clone the repository
git clone https://github.com/muleforge/muleforge.git
cd muleforge

# Build the Rust core
cd core
cargo build --release
cd ..

# Install the CLI
cd cli
npm install
npm run build
npm link    # makes 'muleforge' available globally
cd ..
```

### Verify installation

```bash
muleforge --version
muleforge --help
```

## Configuring Output Location

The `<to>` argument specifies where the Camel Quarkus project is created:

```bash
# Relative path (created in current directory)
muleforge migrate ./my-mule-app ./my-quarkus-app

# Absolute path
muleforge migrate ./my-mule-app /home/dev/projects/my-quarkus-app

# Any writable directory
muleforge migrate ./my-mule-app ~/Desktop/migrated-project
```

### Safety rules

- MuleForge **refuses to overwrite** a non-empty directory unless `--force` is passed
- The output directory is created automatically if it doesn't exist
- Parent directories are created as needed

### Git repository initialization

```bash
# Create a Git repo in the output (recommended)
muleforge migrate ./input ./output --init-git

# Push to a remote after migration
muleforge migrate ./input ./output --init-git --push-to git@github.com:acme/new-repo.git

# Skip Git entirely (just files)
muleforge migrate ./input ./output --no-git
```

### Output structure

After migration, the output directory contains a complete, buildable Quarkus project:

```
my-quarkus-app/
├── pom.xml                    # Maven build with Quarkus + Camel dependencies
├── Dockerfile                 # JVM and native build stages
├── README.md                  # Generated project overview
├── MIGRATION_REPORT.md        # Every element: DONE / MANUAL_REVIEW / SKIPPED
├── src/main/java/generated/
│   ├── routes/                # One RouteBuilder class per Mule flow
│   └── beans/                 # Java beans from DataWeave conversions
├── src/main/resources/
│   └── application.properties # Auto-detected configs (Kafka, DB, JMS, etc.)
├── src/test/java/generated/   # Smoke tests per route
├── k8s/                       # Kubernetes manifests (if --k8s)
│   ├── base/                  # Deployment, Service, ConfigMap
│   └── overlays/              # dev/ and prod/ Kustomize overlays
├── kong/                      # Kong gateway config (if --emit-kong-config)
├── .github/workflows/         # CI/CD pipelines
└── docs/                      # Generated documentation
```

## LLM Configuration

MuleForge uses an LLM for two purposes:
1. **DataWeave conversion** — complex DataWeave → Java bean classes
2. **Documentation generation** — rich prose for architecture docs

### Option 1: Claude (recommended)

```yaml
# muleforge.config.yaml
llm:
  provider: claude
  model: claude-sonnet-4-6
  api_key_env: ANTHROPIC_API_KEY
  temperature: 0.0
```

```bash
export ANTHROPIC_API_KEY=sk-ant-...
muleforge migrate ./input ./output
```

### Option 2: OpenAI

```yaml
llm:
  provider: openai
  model: gpt-4o
  api_key_env: OPENAI_API_KEY
  temperature: 0.0
```

### Option 3: Azure OpenAI

```yaml
llm:
  provider: azure
  model: gpt-4o
  api_key_env: AZURE_OPENAI_API_KEY
  host: https://my-instance.openai.azure.com
  temperature: 0.0
```

### Option 4: Ollama (local, air-gapped)

```bash
# Start Ollama locally
ollama pull llama3.1:70b
ollama serve
```

```yaml
llm:
  provider: ollama
  model: llama3.1:70b
  host: http://localhost:11434
  temperature: 0.0
```

### Option 5: No LLM

```bash
muleforge migrate ./input ./output --no-llm
```

Without an LLM:
- Simple DataWeave patterns (field mapping, filter, type coercion) are still converted automatically
- Complex DataWeave generates TODO stub beans with the original expression in Javadoc
- Documentation is generated as structured templates (not prose)

## Migration Workflow

### Step 1: Analyze (dry-run)

```bash
muleforge analyze ./my-mule-app
```

Shows what would be migrated without writing anything.

### Step 2: Migrate

```bash
muleforge migrate ./my-mule-app ./my-quarkus-app --init-git --k8s
```

### Step 3: Review the migration report

Open `./my-quarkus-app/MIGRATION_REPORT.md`. Every Mule element is listed with:
- **DONE** — automatically migrated, no action needed
- **MANUAL_REVIEW** — migrated with best effort, developer should verify
- **SKIPPED** — not applicable or not supported

### Step 4: Build and test

```bash
cd my-quarkus-app
mvn quarkus:dev           # Start in dev mode
mvn verify                # Run tests
mvn package -Pnative      # Build native binary (optional)
```

### Step 5: Fix MANUAL_REVIEW items

Search for `// TODO:` comments in the generated Java files. Each one explains what needs manual attention and references the original Mule element.

## CLI Reference

```
muleforge migrate <from> <to> [options]

Options:
  --from <path|url>         Input Mule project (local path or Git URL)
  --to <path>               Output directory for Camel Quarkus project
  --init-git                Initialize a Git repository in the output
  --push-to <url>           Push the output repo to this remote
  --incremental-commits     Write output in logical commits (not one big commit)
  --force                   Overwrite non-empty output directory
  --k8s                     Generate Kubernetes manifests (default: true)
  --emit-kong-config        Generate Kong Gateway declarative config
  --no-llm                  Disable LLM (pattern matching + stubs only)
  --llm-provider <name>     Override LLM provider (claude|openai|azure|ollama)
  --llm-model <name>        Override LLM model
  --config <path>           Path to muleforge.config.yaml (default: ./muleforge.config.yaml)
  --dry-run                 Analyze without writing (same as 'muleforge analyze')
  --verbose                 Show detailed progress
  --no-git                  Skip Git initialization

muleforge analyze <path> [options]

Options:
  --config <path>           Path to muleforge.config.yaml
  --output <path>           Write analysis report to file (default: stdout)
```

## Mapping Rules

MuleForge uses YAML files in `mappings/` to define how Mule elements map to Camel components. You can add custom rules:

```yaml
# mappings/my-custom-connector.yaml
component: my-connector
version: "1.0"

mappings:
  - id: my-listener
    mule_ns: myns
    mule_name: listener
    camel_uri: "direct:{name}"
    camel_component: direct
    maven_deps:
      - group_id: com.example
        artifact_id: my-camel-component
    notes: "Custom connector mapping"
```

Custom rules take priority over built-in mappings.

## Troubleshooting

### "muleforge-core binary not found"

Build the Rust core first:
```bash
cd core && cargo build --release
```
Or set the path manually:
```bash
export MULEFORGE_CORE=/path/to/muleforge-core
```

### "expected directory src/main/mule to exist"

The input directory must be a valid Mule 4 project with XML flows in `src/main/mule/`.

### Many MANUAL_REVIEW items

Configure an LLM provider for better DataWeave conversion. Without an LLM, complex transforms generate TODO stubs.

### Generated Java doesn't compile

Check `MIGRATION_REPORT.md` for MANUAL_REVIEW items. These are elements that need developer attention. Common causes:
- Complex DataWeave expressions that need manual Java implementation
- Custom Java components from the Mule project that need porting
- Connector configurations that need environment-specific values
