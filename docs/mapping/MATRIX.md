# Mule → Camel Quarkus Mapping Matrix

This document is the authoritative reference for how Mule elements map to Camel Quarkus. Each row corresponds to a mapping rule in `mappings/`.

**Legend:**
- ✅ Rule-based (deterministic)
- 🤖 LLM-assisted
- ⚠️ Manual review
- ❌ Not supported

## Flow and routing

| Mule element | Camel Quarkus equivalent | Status | Rule file |
|---|---|---|---|
| `<flow name="X">` | `RouteBuilder` with `.routeId("X")` | ✅ | `mappings/core/flow.yaml` |
| `<sub-flow name="X">` | `direct:X` route | ✅ | `mappings/core/subflow.yaml` |
| `<flow-ref name="X">` | `.to("direct:X")` | ✅ | `mappings/core/flow-ref.yaml` |
| `<choice>` / `<when>` / `<otherwise>` | `.choice().when().otherwise()` | ✅ | `mappings/core/choice.yaml` |
| `<scatter-gather>` | `.multicast().aggregationStrategy(...)` | ✅ | `mappings/core/scatter-gather.yaml` |
| `<foreach>` | `.split().body()` or `.loop()` | ✅ | `mappings/core/foreach.yaml` |
| `<parallel-foreach>` | `.split().parallelProcessing()` | ✅ | `mappings/core/parallel-foreach.yaml` |
| `<until-successful>` | `errorHandler(deadLetterChannel(...).maximumRedeliveries(...))` | ⚠️ | partial |
| `<async>` | `.wireTap(...)` or SEDA endpoint | ✅ | `mappings/core/async.yaml` |
| `<try>` / `<error-handler>` | `.doTry().doCatch().doFinally()` or `onException()` | ✅ | `mappings/core/try.yaml` |

## Transformers

| Mule element | Camel Quarkus equivalent | Status |
|---|---|---|
| `<set-payload>` (literal) | `.setBody(constant(...))` | ✅ |
| `<set-payload>` (DW expression) | `.transform(...)` with generated bean | ✅ / 🤖 |
| `<set-variable>` | `.setProperty(...)` or `.setHeader(...)` | ✅ |
| `<remove-variable>` | `.removeProperty(...)` | ✅ |
| `<ee:transform>` with DW | Generated Java bean or Jolt spec | ✅ / 🤖 |
| `<json-to-object>` | `.unmarshal().json(JsonLibrary.Jackson, ...)` | ✅ |
| `<object-to-json>` | `.marshal().json(JsonLibrary.Jackson)` | ✅ |
| `<xml-to-object>` | `.unmarshal().jacksonXml(...)` | ✅ |
| `<logger>` | `.log(...)` | ✅ |

## HTTP

| Mule element | Camel Quarkus equivalent | Camel component |
|---|---|---|
| `<http:listener>` | `from("platform-http:/path")` | `camel-quarkus-platform-http` |
| `<http:request>` | `.to("https://...")` | `camel-quarkus-http` |
| `<http:listener-config>` (TLS) | `quarkus.http.ssl.*` properties | native Quarkus |
| `<http:basic-security-filter>` | Basic auth filter / Quarkus security | `quarkus-security` |

## Database

| Mule element | Camel Quarkus equivalent | Camel component |
|---|---|---|
| `<db:select>` | `.to("sql:SELECT ...")` | `camel-quarkus-sql` |
| `<db:insert>` | `.to("sql:INSERT ...")` | `camel-quarkus-sql` |
| `<db:update>` | `.to("sql:UPDATE ...")` | `camel-quarkus-sql` |
| `<db:stored-procedure>` | `.to("sql-stored:...")` | `camel-quarkus-sql` |
| `<db:bulk-insert>` | `.split().to("sql:...")` or JDBC batch | `camel-quarkus-sql` |

## Messaging

| Mule element | Camel Quarkus equivalent | Camel component |
|---|---|---|
| `<jms:publish>` | `.to("jms:queue:...")` | `camel-quarkus-jms` |
| `<jms:listener>` | `from("jms:queue:...")` | `camel-quarkus-jms` |
| `<kafka:publish>` / `<kafka:message-publisher>` | `.to("kafka:topic")` | `camel-quarkus-kafka` |
| `<kafka:consumer>` | `from("kafka:topic")` | `camel-quarkus-kafka` |
| `<anypoint-mq:publish>` | Depends on target broker; Kafka by default | `camel-quarkus-kafka` |
| `<anypoint-mq:subscriber>` | Depends on target broker | `camel-quarkus-kafka` |
| `<amqp:publish>` | `.to("amqp:exchange:...")` | `camel-quarkus-amqp` |

**Note on Anypoint MQ:** there is no direct open-source equivalent. MuleForge defaults to Kafka with a note in the migration report. If the user prefers RabbitMQ or ActiveMQ, it is a configuration choice.

## Scheduling

| Mule element | Camel Quarkus equivalent | Camel component |
|---|---|---|
| `<scheduler>` (fixed frequency) | `from("timer:...")` | `camel-quarkus-timer` |
| `<scheduler>` (cron) | `from("quartz:...?cron=...")` | `camel-quarkus-quartz` |

## File and stream

| Mule element | Camel Quarkus equivalent | Camel component |
|---|---|---|
| `<file:read>` | `.to("file:...?fileName=...&noop=true")` | `camel-quarkus-file` |
| `<file:write>` | `.to("file:...")` | `camel-quarkus-file` |
| `<file:listener>` | `from("file:...")` | `camel-quarkus-file` |
| `<sftp:*>` | `camel-quarkus-sftp` endpoints | `camel-quarkus-sftp` |
| `<ftp:*>` | `camel-quarkus-ftp` endpoints | `camel-quarkus-ftp` |
| `<email:send>` | `.to("smtp:...")` | `camel-quarkus-mail` |

## SaaS / Enterprise connectors

| Mule connector | Status | Notes |
|---|---|---|
| Salesforce | 🤖 | `camel-quarkus-salesforce` covers most operations; complex flows get LLM assistance. |
| SAP | ⚠️ | No drop-in Camel component. MuleForge emits an adapter service skeleton using SAP JCo; requires manual JCo library installation. |
| ServiceNow | 🤖 | HTTP-based; falls back to `camel-quarkus-http` with `MANUAL_REVIEW`. |
| Workday | ⚠️ | Emits stub. |
| NetSuite | ⚠️ | Emits stub. |

## Security

| Mule element | Camel Quarkus equivalent |
|---|---|
| `<oauth2-provider>` | Quarkus OIDC (`quarkus-oidc`) — emitted as configuration, not code. |
| `<crypto:encrypt>` / `<crypto:decrypt>` | `camel-quarkus-crypto` |
| Secure property placeholder | Quarkus config + MicroProfile Config, or Vault extension. |

## Error handling

| Mule element | Camel Quarkus equivalent |
|---|---|
| `<on-error-continue>` | `.doCatch(Exception.class).continued(true)` |
| `<on-error-propagate>` | `onException(...).handled(false)` |
| `<raise-error>` | `.throwException(...)` |

## Observability

Mule's built-in logging, CloudHub analytics, and Anypoint Monitoring are replaced by:

- **Logging:** Quarkus logging (JSON to stdout), already configured by default.
- **Metrics:** `quarkus-micrometer-registry-prometheus` (emitted automatically).
- **Tracing:** `quarkus-opentelemetry` (emitted automatically, configurable endpoint).

These are added to `pom.xml` whenever a flow uses `<logger>` or any HTTP/DB/messaging element.

## Gaps and known limitations

- DataWeave modules with complex recursion or user-defined functions may emit `MANUAL_REVIEW`.
- Mule's object store (`<os:*>`) has no direct Camel equivalent; MuleForge emits an abstraction over Redis or Caffeine depending on config.
- Mule's batch jobs (`<batch:job>`) map imperfectly to Camel's split/aggregate pattern; complex batch with persistent queues require review.
- Custom Java components referenced via `<java:invoke>` are preserved as-is and bridged via Camel's `bean:` endpoint.
