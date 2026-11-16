# Mapping Coverage Matrix

This document shows the full mapping coverage between Mule 4 elements and Apache Camel Quarkus components.

## Sources (Flow Entry Points)

| Mule Element | Camel Equivalent | Status | Notes |
|---|---|---|---|
| `http:listener` | `platform-http:` | ✅ Auto | Path + method extracted |
| `kafka:consumer` / `kafka:listener` | `kafka:` | ✅ Auto | Topic and group ID from the listener, brokers from its config |
| `kafka:message-listener` | `kafka:` | ⚠️ Partial | Auto (like `kafka:consumer`) only when the listener itself has a `topic` attribute. Kafka connector 4.x keeps topic patterns, group ID and bootstrap servers on the `consumer-config` connection, which is not read: such a listener is flagged for manual review |
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
| `flow-ref` | `to("direct:name")` | ✅ Auto | Sub-flows → direct endpoints. A `flow-ref` to a `<flow>`, or to a name without sub-flow route, is flagged `MANUAL_REVIEW` (see Error Handling) |
| `http:request` | `to("http:...")` | ✅ Auto | URL + method from config-ref |
| `db:select/insert/update/delete` | `to("sql:...")` | ✅ Auto | SQL from element text |
| `db:stored-procedure` | `to("sql-stored:...")` | ✅ Auto | Via mapping YAML |
| `kafka:publish` | `to("kafka:topic")` | ✅ Auto | Brokers from its config |
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
| `on-error-propagate` | `onException(X.class).handled(false)` | ✅ Auto | Handler steps run, the caller still gets the error |
| `on-error-continue` | `onException(X.class).handled(true)` | ✅ Auto | Handler result is the reply (not `continued(true)`, which would resume the route) |
| `try` scope | `doTry/doCatch/doFinally` | ✅ Auto | |
| `raise-error` | `throwException()` | ✅ Auto | Error type in message |
| `validation:is-true` | `validate()` | ✅ Auto | |

Error types of an on-error scope (`type="A, B"`):

- `ANY`, or no `type`: `java.lang.Exception.class` — every exception, as `ANY`
  is every error; reported as `DONE`. What the clause covers is described
  under "Which failures a clause handles" below.
- Any other known type (`HTTP:CONNECTIVITY`, `HTTP:TIMEOUT`, `HTTP:UNAUTHORIZED`,
  `HTTP:FORBIDDEN`, `DB:CONNECTIVITY`, `DB:BAD_SQL_SYNTAX`,
  `VALIDATION:INVALID_PAYLOAD`): the nearest Java exception class. A class is
  never exactly a Mule error type, so the clause is emitted and flagged
  `MANUAL_REVIEW`, with a `TODO` in the generated code that says how it differs.
- A type without known class is never widened to `Exception`. A scope left
  without class, a scope with a `when` condition (not converted) and a scope
  shadowed by an earlier one (Mule takes the first match) are emitted commented
  out and flagged `MANUAL_REVIEW`.
- `<error-handler ref="...">` (global error handler) is not migrated and flagged
  `MANUAL_REVIEW`.

Which failures a clause handles. In Mule the error handler of a flow also
handles what fails in a flow or sub-flow the flow calls with `flow-ref`, unless
the called flow handles it itself. A Camel `onException` clause handles what
fails in its own route; a failure in a route called through `direct:` reaches
it only if that route has no error handler of its own:

- `<sub-flow>` (no error handling of its own in Mule): its route is emitted
  with `.errorHandler(noErrorHandler())`, so its failures go to the
  `onException` clauses of the calling route, as in Mule. The `flow-ref` is
  reported as `DONE`.
- `<flow>` called with `flow-ref`: not equivalent, flagged `MANUAL_REVIEW` on
  the `flow-ref`. The route of the flow keeps its error handler (its own
  `onException` clauses need it), so an error the flow propagates
  (`on-error-propagate`, or no matching on-error scope) fails the exchange
  without running the caller's clauses, where Mule would run the caller's
  error handler next. The route also starts at the flow's own source and does
  not consume from `direct:<name>`.
- `flow-ref` to a name for which no sub-flow route is generated: flagged
  `MANUAL_REVIEW`.

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
| Kafka connector 4.x subscription on the `consumer-config` connection (topic patterns, group ID, bootstrap servers) | `kafka:message-listener` is flagged for manual review; set the `kafka:` source by hand | - |
| MUnit tests | Scaffolded to JUnit 5 with TODO | ✅ Scaffold |
| API specs (RAML/OAS) | Auto-copied to output | ✅ Auto-copy |
| Mule domain projects | Flatten configs manually | - |
| Batch processing (complex) | Split + aggregate manually | MVP-2 |
| CloudHub features | Use K8s equivalents | - |
| Object Store (persistent) | Use Redis/Infinispan | Via mapping |
| VM queues | SEDA (in-memory) | ✅ Auto |
