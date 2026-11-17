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
| `set-payload` | `setBody()` | ⚠️ Partial | Auto for a literal value and for the expressions listed under [Expressions](#expressions); any other `#[...]` value is flagged `MANUAL_REVIEW` and the route fails at that step |
| `set-variable` | `setProperty()` | ⚠️ Partial | As `set-payload`. A Mule variable is an exchange property |
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
| `choice` | `choice()` | ⚠️ Partial | Every `when` and the `otherwise` are kept, in order. Auto when every condition is one of the shapes under [Expressions](#expressions); otherwise the choice is flagged `MANUAL_REVIEW` and an unconverted condition fails when it is evaluated |
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
| `#[payload.field]` in a logger message | Simple `${body.field}` | ✅ Auto | Logger messages only (lenient, see [Expressions](#expressions)) |
| `#[vars.x]` / `#[flowVars.x]` | Simple `${exchangeProperty.x}` | ✅ Auto | |
| `parse-template` | Freemarker | ✅ Auto | |

## Expressions

How a Mule expression (`#[...]`, DataWeave in Mule 4) is converted where it
decides what the route does: the `expression` of a choice `<when>` and the
`value` of `set-variable` / `set-payload`.

The rule: an expression is reported `DONE` only if the Camel expression reads
the same value and gives the same result **whatever the payload is**. Anything
else is `MANUAL_REVIEW`, with the Mule expression kept in a `TODO` in the
generated route. No expression is ever passed on to Camel as it was written
(`simple("#[...]")` does not parse: "Unexpected token # at location 0", and the
application does not start).

### Values (`set-variable value`, `set-payload value`)

| Mule value | Camel | Status | Why |
|---|---|---|---|
| no `#[...]`: `value="Hello"`, `value="true"` | `constant("Hello")`, `constant("true")` | `DONE` | A value without `#[...]` is a literal string in Mule, `"true"` included |
| `#[payload]`, `#[message.payload]` | `simple("${body}")` | `DONE` | The whole payload is the message body. A single Simple function gives the object as it is, so nothing is read or converted |
| `#[vars.name]`, `#[vars['name']]`, `#[flowVars.name]` | `simple("${exchangeProperty.name}")` | `DONE` | `set-variable` is `setProperty`, so the variable is that exchange property. The name has to be a plain identifier |
| `#['text']`, `#["text"]` | `constant("text")` | `DONE` | A string in both. Not converted if it contains a quote, `\`, `$` (interpolation in DataWeave and in Simple) or a brace (`{{...}}` is a Camel property placeholder) |
| `#[42]`, `#[-1]`, `#[true]`, `#[false]` | `constant(42)`, `constant(true)` | `DONE` | A constant of the same type (`Integer`, `Boolean`), not the string `"42"`. Integers that are a Java `int` only |
| `#[payload.id]`, `#[payload['id']]`, `#[payload.items[0]]` | not converted | `MANUAL_REVIEW` | Field access into the payload, see below |
| `#[attributes.headers.x]`, `#[attributes.queryParams.x]`, ... | not converted | `MANUAL_REVIEW` | Attributes, see below |
| `#[vars.order.id]`, `#[vars['order-id']]` | not converted | `MANUAL_REVIEW` | Reads into the value of a variable, which depends on the type of that value |
| function calls (`#[now()]`, `#[uuid()]`, `#[sizeOf(payload)]`), `++`, arithmetic, `if`/`else`, `#[null]`, decimals, DataWeave scripts (`#[output application/json --- {...}]`), text mixed with `#[...]` | not converted | `MANUAL_REVIEW` | Not a shape with a Camel expression that is the same for every payload |

### Conditions (choice `<when expression>`)

| Mule condition | Camel | Status | Why |
|---|---|---|---|
| `#[vars.name == 'A']`, `#[vars.name != 'A']` | `simple("${exchangeProperty.name} == 'A'")` | `DONE` | Same comparison (case-sensitive in both). See "Type coercion" below |
| `#[vars.n == 3]`, `#[vars.flag == true]`, `#[vars.id != null]` | `simple("${exchangeProperty.n} == 3")`, `... == true`, `... != null` | `DONE` | Simple takes a number, `true` / `false` and `null` as the right-hand value |
| `#[vars.a == vars.b]` | `simple("${exchangeProperty.a} == ${exchangeProperty.b}")` | `DONE` | |
| `#['A' == vars.name]` | `simple("${exchangeProperty.name} == 'A'")` | `DONE` | Simple needs the `${...}` function on the left; `==` and `!=` are the same with their sides swapped |
| `#[true]`, `#[false]` | `constant(true)`, `constant(false)` | `DONE` | |
| `#[payload.type == 'A']` | not converted | `MANUAL_REVIEW` | Field access into the payload, see below |
| `#[payload == 'A']`, `#[payload != null]` | not converted | `MANUAL_REVIEW` | Mule compares the value it reads according to the MIME type of the payload, Simple the body as it arrived. Checked on Camel 4.18: for the JSON body `"A"`, `${body} == 'A'` is false, where Mule's `payload == 'A'` is true |
| `#[attributes.queryParams.id == '1']`, `#[attributes.headers['x'] == 'a']` | not converted | `MANUAL_REVIEW` | Attributes, see below |
| `#[vars.n > 3]` and `<`, `>=`, `<=` | not converted | `MANUAL_REVIEW` | Ordering depends on the types of the two values (number or text), which are not known |
| `#[vars.flag]`, `#[payload]` (a value as the condition) | not converted | `MANUAL_REVIEW` | Mule expects a Boolean; Camel takes every value except `null`, `false`, `"false"` and `""` as true |
| `and` / `or` / `not`, function calls (`#[isEmpty(payload)]`), `~=`, anything else, a `<when>` without `expression` | not converted | `MANUAL_REVIEW` | Not converted yet, or no Camel predicate that is the same for every payload |

### Why field access into the payload is not converted

`payload.type` in Mule reads the field `type` of whatever the payload is: a
JSON or XML document, a Map, a Java object. Mule decides by the MIME type of
the payload at run time. Camel has no single expression for that: Simple's
`${body.type}` is OGNL, it calls `getType()` on a Java object (and needs the
`bean` language, `camel-quarkus-bean`, which is not among the generated
dependencies: without it the application does not start, "No language could be
found for: bean"). It does not read a field out of a JSON or XML body; that
takes `jsonpath`, `jq` or `xpath`, or an `unmarshal` first. Which one is right
depends on the payload type, and the migrator does not know it. So the
expression is left to a person, who does.

(Logger messages are the exception: `#[payload.name]` in a `logger` message is
still written as `${body.name}`. That conversion is lenient and has the same
weakness.)

### Why attributes are not converted

`attributes` holds what the source or the last connector operation of the flow
produced (HTTP request, Kafka record, HTTP response, ...); the conversion does
not follow that. For an HTTP listener there is a second reason: Camel's
`platform-http` puts request headers **and** query parameters into the one
header map of the message. Checked on Camel 4.18: `${header.x}` is `q` for
`?x=q`, `h` for the header `x: h`, and `[h, q]` when a request has both. A
check on `attributes.headers.x` written as `${header.x}` could be satisfied
through the query string.

### Type coercion

Simple's `==` converts the right-hand value to the type of the left-hand value
before it compares; DataWeave's `==` does not (`"true" == true` is false,
`~=` is the operator that coerces). So a `DONE` comparison gives the same
result as in Mule when both sides have the same type or one of them is null,
and can differ when Mule compares values of different types. Checked on Camel
4.18: an exchange property holding the string `"true"` equals `true`, and one
holding `"5"` equals `5`. In Mule both comparisons are false. A variable set
from a literal (`value="true"`) is a string in both, so compare it with a
string (`vars.flag == 'true'`).

### What an unconverted expression is emitted as

The route has to compile and the application has to start, and the route must
not do something else than the Mule flow without a sign of it.

* **Choice condition**: the branch stays where it is, with its steps. Its
  condition is a predicate that throws:

  ```java
  // TODO: review this condition, it is not converted (Mule: #[payload.type == 'A'])
  // - <reason>
  // - a message that reaches this condition fails here instead of taking a branch by guess; replace the predicate
  .when(exchange -> { throw new UnsupportedOperationException("choice condition not migrated: #[payload.type == 'A']"); })
      .setBody(constant("Routed to handler A"))
  ```

  Camel, like Mule, evaluates the conditions in order and takes the first that
  is true. Branches in front of the unconverted condition therefore work as in
  Mule. A message that gets as far as the unconverted condition fails there
  (HTTP 500 for an HTTP route, with the Mule expression in the log).

  A condition that never matches (`constant(false)`) would be worse: such a
  message would be handled by a later branch or by `otherwise`, the reply
  would look like a success, and nothing would say that it is the wrong one.
  A condition that always matches is the same mistake the other way round.

* **`set-variable` / `set-payload` value**: the step is replaced by one that
  throws:

  ```java
  // TODO: review this setProperty("id"), its value is not converted (Mule: #[payload.id])
  // - <reason>
  // - the route fails at this step instead of going on with a wrong value; replace it with the converted step
  .throwException(new UnsupportedOperationException("setProperty(\"id\") value not migrated: #[payload.id]"))
  ```

  Setting the Mule expression as a string (what used to happen) or leaving the
  step out would let the route go on with a wrong or missing value: a later
  condition on it takes the wrong branch, a reply carries the text `#[...]`.
  The steps behind it are still emitted.

The exception is one like any other. If the route has an `onException` clause
that takes every exception (a Mule `on-error-continue` / `on-error-propagate`
without `type`, or with `ANY`), that clause handles it too, and the reply is
then the handler's.

The generated smoke test of such a route (`testRoute...Returns200`) fails
until the expression is replaced. That is intended: the route is not migrated
yet.

### In the report

A choice is one row: `DONE` if every condition is converted, otherwise
`MANUAL_REVIEW` with the unconverted conditions and the reasons. A
`set-variable` / `set-payload` is one row, `MANUAL_REVIEW` if its value is not
converted. A mapping rule without `camel_uri` (`set-payload`, `set-variable`,
`choice-router`, `transform-message`, ...) no longer adds a `DONE` row of its
own next to the row of the built-in mapping; its id is shown on that row.

### Not covered

* `split` / `foreach` expressions are not converted yet and are still emitted
  as `simple("#[...]")`.
* `mimeType` / `encoding` of `set-payload` and `set-variable` are ignored.
* A variable is an exchange property (`setProperty`), as before. Camel 4.4+
  also has variables (`setVariable`, `${variable.name}`); the mapping was not
  changed to them.

### Documentation this is based on

* Apache Camel, Simple language (functions, "Built-in Operators", OGNL):
  <https://camel.apache.org/components/next/languages/simple-language.html>.
  "Camel operators require the left value must be enclosed in `${ }`", "There
  must be spaces around the operator", "Camel will automatically type convert
  the rightValue type to the leftValue type".
* Apache Camel, Choice EIP:
  <https://camel.apache.org/components/next/eips/choice-eip.html>. "The first
  when clause that matches determines which route branch to follow."
* Apache Camel, Set Property, Set Body and Set Variable EIPs:
  <https://camel.apache.org/components/next/eips/setProperty-eip.html>,
  <https://camel.apache.org/components/next/eips/setBody-eip.html>,
  <https://camel.apache.org/components/next/eips/setVariable-eip.html>.
* MuleSoft, Choice router:
  <https://docs.mulesoft.com/mule-runtime/latest/choice-router-concept>. "the
  first expression that evaluates to `true` triggers that route's execution
  and the others are not checked"; its example is
  `#[vars.language == 'Spanish']`.
* MuleSoft, Set Payload transformer:
  <https://docs.mulesoft.com/mule-runtime/latest/set-payload-transformer-reference>.
  The value "Accepts a literal string or DataWeave expression".
* MuleSoft, DataWeave operators:
  <https://docs.mulesoft.com/dataweave/latest/dw-operators>. `("true" == true)`
  is false; `~=` is the "Equality operator that tries to coerce one value to
  the type of the other when the types are different".

The Camel side was run, not only read: the emitted forms above (each `DONE`
row, both unconverted forms) were compiled into a Camel Quarkus application
(Quarkus 3.33.1, Camel 4.18.0) and called over HTTP. The Mule side rests on
the documentation; no Mule runtime was used.

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
