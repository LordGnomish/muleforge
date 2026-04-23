# Mapping Coverage Matrix

This document shows the full mapping coverage between Mule 4 elements and Apache Camel Quarkus components.

## Sources (Flow Entry Points)

| Mule Element | Camel Equivalent | Status | Notes |
|---|---|---|---|
| `http:listener` | `platform-http:` | ✅ Auto | Path + method extracted |
| `kafka:consumer` / `kafka:listener` | `kafka:` | ✅ Auto | Topic from config |
| `jms:listener` | `jms:queue:` | ✅ Auto | Destination from config |
| `amqp:listener` | `amqp:queue:` | ✅ Auto | Via mapping YAML |
| `file:listener` / `file:read` | `file:` | ✅ Auto | Directory from config |
| `sftp:listener` | `sftp:` | ✅ Auto | Host/port from config |
| `ftp:listener` | `ftp:` | ✅ Auto | Host/port from config |
| `scheduler` (fixed) | `timer:` | ✅ Auto | Frequency preserved |
| `scheduler` (cron) | `quartz:` | ✅ Auto | Cron expression preserved |
| `email:listener-imap` | `imap:` | ✅ Auto | Via mapping YAML |
| `email:listener-pop3` | `pop3:` | ✅ Auto | Via mapping YAML |

## Processors (Mid-Flow)

| Mule Element | Camel Equivalent | Status | Notes |
|---|---|---|---|
| `set-payload` | `setBody()` | ✅ Auto | MEL expressions auto-converted |
| `set-variable` | `setProperty()` | ✅ Auto | MEL expressions auto-converted |
| `set-attribute` | `setHeader()` | ✅ Auto | |
| `remove-variable` | `removeProperty()` | ✅ Auto | |
| `remove-attribute` | `removeHeader()` | ✅ Auto | |
| `logger` | `log()` | ✅ Auto | Level preserved |
| `flow-ref` | `to("direct:name")` | ✅ Auto | Sub-flows → direct endpoints |
| `http:request` | `to("http:...")` | ✅ Auto | URL + method from config-ref |
| `db:select/insert/update/delete` | `to("sql:...")` | ✅ Auto | SQL from element text |
| `db:stored-procedure` | `to("sql-stored:...")` | ✅ Auto | Via mapping YAML |
| `kafka:publish` | `to("kafka:topic")` | ✅ Auto | |
| `jms:publish` | `to("jms:queue:dest")` | ✅ Auto | |
| `amqp:publish` | `to("amqp:queue:dest")` | ✅ Auto | Via mapping YAML |
| `file:write` / `sftp:write` | `to("file/sftp:path")` | ✅ Auto | |
| `email:send` | `to("smtp:...")` | ✅ Auto | Via mapping YAML |
| `salesforce:query/create/update/delete` | `to("salesforce:...")` | ✅ Auto | Via mapping YAML |
| `wsc:consume` (SOAP) | `to("cxf:...")` | ✅ Auto | Via mapping YAML |

## Routing / EIP

| Mule Element | Camel Equivalent | Status | Notes |
|---|---|---|---|
| `choice` | `choice()` | ✅ Auto | When + otherwise preserved |
| `scatter-gather` | `multicast()` | ✅ Auto | |
| `split` | `split()` | ✅ Auto | Parallel flag preserved |
| `foreach` | `split()` | ✅ Auto | Sequential split |
| `first-successful` | Failover LB | ✅ Auto | Via mapping YAML |
| `round-robin` | Round-robin LB | ✅ Auto | Via mapping YAML |
| `until-successful` | Error handler retry | ✅ Auto | maxRetries + delay |
| `async` | `wireTap()` | ✅ Auto | Fire-and-forget |
| `idempotent-message-validator` | `idempotentConsumer()` | ✅ Auto | |

## Transforms

| Mule Element | Camel Equivalent | Status | Notes |
|---|---|---|---|
| Simple DataWeave (field mapping) | Java bean | ✅ Pattern | No LLM needed |
| Simple DataWeave (filter/map) | Java bean | ✅ Pattern | No LLM needed |
| Simple DataWeave (type coercion) | Java bean | ✅ Pattern | No LLM needed |
| Simple DataWeave (string concat) | Java bean | ✅ Pattern | No LLM needed |
| Simple DataWeave (null coalescing) | Java bean | ✅ Pattern | No LLM needed |
| Complex DataWeave | Java bean | 🤖 LLM | Claude/OpenAI/Ollama |
| DataWeave (no LLM) | TODO stub | 📝 Stub | Original DW in Javadoc |
| MEL `#[payload.field]` | Simple `${body.field}` | ✅ Auto | |
| MEL `#[flowVars.x]` | Simple `${exchangeProperty.x}` | ✅ Auto | |
| `parse-template` | Freemarker | ✅ Auto | |

## Error Handling

| Mule Element | Camel Equivalent | Status | Notes |
|---|---|---|---|
| `on-error-propagate` | `onException(handled=false)` | ✅ Auto | |
| `on-error-continue` | `onException(handled=true)` | ✅ Auto | |
| `try` scope | `doTry/doCatch/doFinally` | ✅ Auto | |
| `raise-error` | `throwException()` | ✅ Auto | Error type in message |
| `validation:is-true` | `validate()` | ✅ Auto | |

## Infrastructure Output

| Output | Generated | Notes |
|---|---|---|
| Java RouteBuilder classes | ✅ | One per flow |
| `pom.xml` with dependencies | ✅ | Auto-detected from connectors |
| `application.properties` | ✅ | Kafka/JMS/DB auto-detected |
| `Dockerfile` (JVM + native) | ✅ | Multi-stage build |
| Kubernetes manifests | ✅ | Deployment, Service, ConfigMap, HPA |
| Kustomize overlays (dev/prod) | ✅ | |
| Kong Gateway config | ✅ | Optional, decK format |
| GitHub Actions CI/CD | ✅ | Build + native release |
| Smoke tests (JUnit 5) | ✅ | Per route |
| Migration report | ✅ | Every element classified |
| Project documentation | ✅ | Architecture, per-flow, runbook, etc. |

## Not Yet Supported

| Mule Element | Workaround | Planned |
|---|---|---|
| Custom Java components | Auto-copied with TODO markers | ✅ Auto-copy |
| Anypoint MQ | Use Kafka or AMQP | - |
| MUnit tests | Scaffolded to JUnit 5 with TODO | ✅ Scaffold |
| API specs (RAML/OAS) | Auto-copied to output | ✅ Auto-copy |
| Mule domain projects | Flatten configs manually | - |
| Batch processing (complex) | Split + aggregate manually | MVP-2 |
| CloudHub features | Use K8s equivalents | - |
| Object Store (persistent) | Use Redis/Infinispan | Via mapping |
| VM queues | SEDA (in-memory) | ✅ Auto |
