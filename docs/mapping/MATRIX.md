# Mapping Coverage Matrix

What the migrator emits for each Mule 4 element. The tables are **measured,
not read from the mapping rules**: for every row a probe application with that
element was migrated with the core binary, and the row says what came out
([How this was measured](#how-this-was-measured)). The defects this turned up
are listed under [Known defects](#known-defects). They are documented here,
not fixed.

[Where a `choice` ends](#where-a-choice-ends), [Expressions](#expressions)
and the text under [Error Handling](#error-handling) go into the detail of
`choice`, `foreach`, expressions and on-error scopes; the tables link to them.

## Classes

Every row has one of these classes. "Report" is the status of the element's
row in `MIGRATION_REPORT.md` (written there as `Done` / `ManualReview`).

| Class | Report | What it means |
|---|---|---|
| **Converted** | `DONE` | A Camel endpoint or step with the values of the Mule element. The emitted project was built, the application started and, for a processor, a request went through the step |
| **Converted, to review** | `MANUAL_REVIEW` | Emitted and working, on an assumption that is stated in a `TODO` in the route and in the report |
| **Wrong, reported `DONE`** | `DONE` | A defect. An endpoint or step is emitted and the application starts, but it is not what the Mule element says: a `{placeholder}` of the rule's URI template is left in the URI, a value of the element or of its connector config is missing, or the step does something else. It fails, or works on something else, when it is reached. Neither the report nor the route says so |
| **Breaks the build or startup** | `DONE` | A defect. With this element in the application the emitted project does not build, or the application does not start |
| **Not migrated - fails loudly** | `MANUAL_REVIEW` | The route fails when it reaches the step, instead of going on with a wrong result |
| **Not migrated - flagged only** | `MANUAL_REVIEW` | A `// TODO` comment in the route, or the report row alone, and nothing executable: the route goes on without the step. If the element is a scope, the processors in it are not emitted and have no row of their own in the report |
| **Not measured** | | No probe was written or run for it. Nothing is claimed |

A `DONE` row in a migration report therefore does not mean that the element
works: of the 98 probes, 59 are in a class whose report status is `DONE`, and
45 of those are in one of the two defect classes.

## Sources (Flow Entry Points)

The first child of a `<flow>` is taken as its source.

| Mule element | What is emitted | Class | Notes |
|---|---|---|---|
| `http:listener` with `allowedMethods` | `from("platform-http:/orders?httpMethodRestrict=POST")` | Converted | `POST` was answered with 200, `GET` with 405. Host and port of the `http:listener-config` are not carried over: `quarkus.http.port=8080` whatever the config says |
| `http:listener` without `allowedMethods` | `from("platform-http:/orders?httpMethodRestrict={allowedMethods}")` | Breaks the build or startup | The generated smoke test does not compile (`.when().{allowedmethods}("/orders")`), so `mvn package` and `mvn test` fail. With the tests left out (`-Dmaven.test.skip=true`) the application does not start: `No enum constant org.apache.camel.component.platform.http.spi.Method.{ALLOWEDMETHODS}` |
| `kafka:consumer`, `kafka:listener` | `from("kafka:orders.raw?brokers=localhost:19092&groupId=orders-group")` | Converted | Topic and `groupId` from the element, `brokers` from the `bootstrapServers` attribute of its config. Started; not run against a broker |
| `kafka:message-listener` with a `topic` attribute | the same endpoint | Converted | Built-in mapping, no rule |
| `kafka:message-listener` without `topic` | `from("direct:kafka-message-listener")` | Not migrated - flagged only | Kafka connector 4.x keeps topic patterns, group ID and bootstrap servers on the `consumer-config` connection, which is not read. The steps of the flow are emitted behind a `direct:` endpoint that nothing sends to |
| `jms:listener` | `from("jms:queue:orders.in")` | Breaks the build or startup | `Failed to start route: probeFlow because: connectionFactory must be specified`. The emitted `pom.xml` has `camel-quarkus-jms` and no JMS client; `application.properties` gets `quarkus.artemis.*` keys, and no Artemis extension is in the `pom.xml`. A topic listener (`<jms:topic-consumer/>`) is emitted as `jms:queue:` as well |
| `amqp:listener` | `from("amqp:queue:{destination}")` | Wrong, reported `DONE` | The rule reads an attribute `destination`; the probe has the connector's `queueName`, which is not read. The application started (and logged that it cannot connect to `amqp://localhost:5672`); not run against a broker |
| `file:listener` with `recursive` | `from("file:in?recursive=false")` | Wrong, reported `DONE` | `directory` is copied as it is. The `workingDir` of the `file:config` is not read and is nowhere in the emitted project: the application created and polled `in` under the working directory of the JVM |
| `file:listener` without `recursive` | `from("file:in?recursive={recursive}")` | Breaks the build or startup | `Cannot convert the String value: {recursive} to type: boolean` |
| `sftp:listener` | `from("sftp:{host}:{port}/inbox")` | Breaks the build or startup | Host and port are on the `sftp:connection` of the config, which the rule does not read: `host must be specified and not empty` |
| `ftp:listener` | `from("ftp://{host}:{port}/inbox")` | Breaks the build or startup | As `sftp:listener` |
| `scheduler` with `<fixed-frequency frequency="5000"/>` | `from("timer:scheduler?period={frequency}")` | Wrong, reported `DONE` | `frequency` is an attribute of `<fixed-frequency>` in the `<scheduling-strategy>`, not of `<scheduler>`. The application started; the timer fired once and not again in the next 21 s |
| `scheduler` with `<cron expression="..."/>` | the same `timer:` endpoint | Wrong, reported `DONE` | The cron expression is not read; no `quartz:` endpoint is emitted. The row in the report is that of rule `scheduler-fixed`; rule `scheduler-cron`, written for an element `scheduling-strategy:cron-scheduler`, did not match |
| `email:listener-imap` | `from("imap://{host}:{port}")` | Breaks the build or startup | `Failed to resolve endpoint: imap://%7Bhost%7D:%7Bport%7D due to: host must be specified and not empty` |
| `email:listener-pop3` | `from("pop3://{host}:{port}")` | Breaks the build or startup | As `email:listener-imap` |
| `vm:listener` | `from("seda:work")` | Breaks the build or startup | `No endpoint could be found for: seda://work`: the rule adds no dependency and `camel-quarkus-seda` is not in the emitted `pom.xml` |
| `salesforce:subscribe-topic` | `from("salesforce:subscribe?topicName=AccountUpdates")` | Breaks the build or startup | `Failed to resolve endpoint ... topicName must be supplied for subscribe/pubsub operations` |
| `salesforce:subscribe-topic-listener` | `from("direct:salesforce-subscribe-topic-listener")` | Not migrated - flagged only | `Unknown source`, no rule |
| `anypoint-mq:subscriber` | `from("direct:anypoint-mq-subscriber")` | Not migrated - flagged only | `Unknown source`. `mappings/connectors/kafka.yaml` has rules for Anypoint MQ, but the files in `mappings/connectors/` are not loaded |
| a flow without source (its first child is a processor) | `from("direct:logger")`, `from("direct:set-payload")`; the processor itself is not emitted | Not migrated - flagged only | The first processor is reported as `Unknown source: logger - needs manual mapping` and left out of the route. With a `set-payload` first there is a second row for it, `DONE`, from its rule |

## Processors (Mid-Flow)

| Mule element | What is emitted | Class | Notes |
|---|---|---|---|
| `set-payload` with a literal value | `.setBody(constant("Hello"))` | Converted | Also converted: the expressions listed under [Expressions](#expressions) |
| `set-payload` with any other `#[...]` value | a `TODO` and `.throwException(new UnsupportedOperationException("setBody value not migrated: #[payload.id]"))` | Not migrated - fails loudly | The request was answered with 500. A stub bean `generated/beans/<Flow>Transform.java` is written for the expression; the route does not use it |
| `set-variable` | `.setProperty("kind", constant("a"))` | Converted | As `set-payload`, for a literal and for an expression. A Mule variable is an exchange property |
| `set-attribute` | `// TODO: manual mapping for set-attribute` | Not migrated - flagged only | No mapping; no `setHeader` |
| `remove-variable` | `// TODO: manual mapping for remove-variable` | Not migrated - flagged only | Rule `remove-variable` names `removeProperty` and emits nothing: the exchange property keeps its value |
| `remove-attribute` | `// TODO: manual mapping for remove-attribute` | Not migrated - flagged only | No mapping; no `removeHeader` |
| `logger` | `.log(LoggingLevel.WARN, "plain text")` | Converted | Level kept. `#[vars.x]` and `#[flowVars.x]` in the message become `${exchangeProperty.x}` |
| `logger` with `#[payload.field]` in its message | `.log(LoggingLevel.INFO, "id is ${body.id}")` | Breaks the build or startup | `Failed to create route: probeFlow at: >>> Log[id is ${body.id}] <<< ... No language could be found for: bean`: Simple's `${body.id}` needs `camel-quarkus-bean`, which is not in the emitted `pom.xml` (see [Why field access into the payload is not converted](#why-field-access-into-the-payload-is-not-converted)) |
| `flow-ref` to a `<sub-flow>` | `.to("direct:shared")` and a route `from("direct:shared")` | Converted | A sub-flow that is referenced exactly once is inlined instead: its steps are emitted in place of the `flow-ref`, without a route for the sub-flow and without a row for the `flow-ref` |
| `flow-ref` to a `<flow>` | `.to("direct:otherFlow")` | Not migrated - fails loudly | `MANUAL_REVIEW` in the report, no `TODO` in the route. The request failed with `No consumers available on endpoint: direct://otherFlow` (see [Error Handling](#error-handling)) |
| `parse-template` | `// TODO: manual mapping for parse-template` | Not migrated - flagged only | Rule `parse-template` adds `camel-quarkus-freemarker` to the `pom.xml` and emits nothing |
| `java:invoke-static` | `// TODO: manual mapping for java:invoke-static` | Not migrated - flagged only | No mapping. The classes under `src/main/java` are copied, see [Project content](#project-content) |

## Connector operations

Each of these has a rule with a `camel_uri` template. Such a rule is the whole
mapping: the placeholders that are attributes of the element are filled in,
the others stay in the URI as they are, and the element is reported `DONE`
either way.

| Mule element | What is emitted | Class | Notes |
|---|---|---|---|
| `http:request` | `.to("http:{url}/api/orders?httpMethod=POST")`; with a `url` attribute `.to("http:http://localhost:18099/api/orders{path}?httpMethod=GET")` | Wrong, reported `DONE` | Host and port of the `http:request-config` are not read. In a flow with an `http:listener` the step failed in both probes: `Invalid url: /probe/probe. If you are forwarding/bridging http endpoints, then enable the bridgeEndpoint option on the endpoint` |
| `db:select`, `db:insert`, `db:update`, `db:delete` | `.to("sql:{query}?dataSource=default")` | Breaks the build or startup | The statement is in the `<db:sql>` child, not in an attribute `query`: it is not in the route, and `<db:input-parameters>` are not read. The application does not start: `No type converter available to convert from type: java.lang.String to the required type: javax.sql.DataSource`. No JDBC driver and no data source are in the emitted `pom.xml` and `application.properties` |
| `db:stored-procedure` | `.to("sql-stored:{procedureName}?dataSource=default")` | Breaks the build or startup | As `db:select` |
| `kafka:publish` | `.to("kafka:orders.out?brokers=localhost:19092")` | Converted | Topic from the element, `brokers` from its config. Started; not run against a broker (without one the request did not return within 40 s) |
| `jms:publish` | `.to("jms:queue:orders.out")` | Wrong, reported `DONE` | The application starts; the step fails with `Failed to create Producer for endpoint: jms://queue:orders.out. Reason: ... connectionFactory must be specified` (no JMS client in the `pom.xml`, see `jms:listener`) |
| `jms:consume` | `.to("jms:queue:orders.replies?receiveTimeout=10000")` | Wrong, reported `DONE` | A producer step like that of `jms:publish`, and it fails in the same way |
| `amqp:publish` | `.to("amqp:queue:{destination}")` | Wrong, reported `DONE` | `exchangeName` is not read. The request failed on the connection (`Connection refused: localhost/127.0.0.1:5672`); not run against a broker |
| `file:write` | `.to("file:out/result.txt")` | Wrong, reported `DONE` | Camel takes the path as a directory: the body was written to `out/result.txt/<message id>` under the working directory of the JVM. `workingDir` of the `file:config` is not read |
| `file:read` | `.to("file:{directory}")` | Wrong, reported `DONE` | A producer step: the application created a directory named `{directory}` and tried to write the message into it (`Cannot write null body to file: {directory}/<message id>`). Nothing is read; `path` is not used |
| `sftp:write`, `ftp:write` | `.to("sftp:{host}:{port}/outbox/result.txt")`, `.to("ftp://{host}:{port}/outbox/result.txt")` | Wrong, reported `DONE` | The application starts; the step fails with `Failed to create Producer for endpoint: sftp://%7Bhost%7D:%7Bport%7D/outbox/result.txt. Reason: ... host must be specified and not empty` |
| `sftp:read`, `ftp:read` | `.to("sftp:{host}:{port}/{directory}")`, `.to("ftp://{host}:{port}/{directory}")` | Wrong, reported `DONE` | A producer step, and it fails like the write |
| `email:send` | `.to("smtp://{host}:{port}")` | Breaks the build or startup | `host must be specified and not empty`. Recipients, subject and body of the element are not read |
| `salesforce:query` | `.to("salesforce:query?sObjectQuery={query}")` | Breaks the build or startup | `Failed to start component salesforce because of loginUrl must be specified`: nothing of the `salesforce:sfdc-config` is carried over. The SOQL text is in a child element and is not read. Not measured with a configured login |
| `salesforce:create`, `salesforce:update` | `.to("salesforce:createSObject?sObjectName=Account")`, `.to("salesforce:updateSObject?sObjectName=Account")` | Breaks the build or startup | As `salesforce:query`; `type` is carried over |
| `salesforce:delete`, `salesforce:upsert` | `.to("salesforce:deleteSObject?sObjectName={type}")`, `.to("salesforce:upsertSObject?sObjectName={type}&sObjectIdName=Ext_Id__c")` | Breaks the build or startup | As `salesforce:query`. The probes have no `type` attribute (`upsert` has `objectType`) |
| `wsc:consume` | `.to("cxf://{address}?serviceClass={serviceClass}&wsdlURL={wsdlLocation}")` | Breaks the build or startup | Address and WSDL are on the `wsc:connection` of the config, which is not read: `No type converter available to convert from type: java.lang.String to the required type: java.lang.Class` |
| `ws:consumer` | `.to("cxf://{address}?wsdlURL={wsdlLocation}")` | Breaks the build or startup | `serviceClass must be specified` |
| `os:store`, `os:retrieve` | `.to("infinispan:objectStore?operation=PUT")`, `.to("infinispan:objectStore?operation=GET")` | Breaks the build or startup | `No configuration found for the default Infinispan client`. Key and value of the element are not read |
| `vm:publish`, `vm:consume` | `.to("seda:work")`, `.to("seda:work?timeout=10000")` | Breaks the build or startup | `No endpoint could be found for: seda://work` (no `camel-quarkus-seda`, see `vm:listener`) |
| `anypoint-mq:publish` | `// TODO: manual mapping for anypoint-mq:publish` | Not migrated - flagged only | No rule is loaded for it (see `anypoint-mq:subscriber`) |
| `sap:sync-rfc` | `// TODO: manual mapping for sap:sync-rfc` | Not migrated - flagged only | There is no rule for an SAP element in `mappings/` |

## Routing / EIP

| Mule element | What is emitted | Class | Notes |
|---|---|---|---|
| `choice` | `.choice()`, a `.when(...)` per `when`, `.otherwise()`, `.end()` | Converted, Converted, to review or Not migrated - fails loudly | Every `when` and the `otherwise` are kept, in order. `DONE` when every condition is one of the converted shapes under [Expressions](#expressions) and every variable it compares is only set from literals (probe: `#[vars.kind == 'a']` after a literal `set-variable`). Otherwise `MANUAL_REVIEW`: an unconverted condition is a predicate that throws when it is evaluated (probe: `#[payload.type == 'A']`, answered with 500), a comparison of a variable that may hold something else is emitted with a `TODO`. For the closing calls see [Where a `choice` ends](#where-a-choice-ends) |
| `scatter-gather` | a `TODO` and `.throwException(new UnsupportedOperationException("scatter-gather not migrated"))` | Not migrated - fails loudly | Answered with 500. Each `<route>` is a `// TODO: manual mapping for route` with a `MANUAL_REVIEW` row; the processors in the routes are not emitted and have no rows. Rule `scatter-gather` names `multicast`; no multicast is emitted. See [Collections](#collections-foreach-collection-split-expression) |
| `split` | `.split(simple("${body}"))` ... `.end()` with a `TODO` | Converted, to review | Its `expression` is converted like the collection of a `foreach`; always `MANUAL_REVIEW` (`split` is not an element of Mule 4) |
| `foreach` without `collection`, or over `#[payload]` / `#[vars.x]` | `.split(simple("${body}")).stopOnException()` ... `.end()` with a `TODO` | Converted, to review | Never `DONE`: a split of the body or of the exchange property, on the assumption that it is a Java collection. See [Collections](#collections-foreach-collection-split-expression) |
| `foreach` over any other collection, or with a `batchSize` other than `1` | a `TODO` and `.throwException(new UnsupportedOperationException("foreach collection not migrated: #[payload.items]"))` | Not migrated - fails loudly | Answered with 500 (probe: `#[payload.items]`; `batchSize` was not probed here, see [Collections](#collections-foreach-collection-split-expression)) |
| `parallel-foreach` | `// TODO: manual mapping for parallel-foreach` | Not migrated - flagged only | No mapping |
| `first-successful` | `// TODO: manual mapping for first-successful` | Not migrated - flagged only | Rule `first-successful` names `failover` and emits nothing; no load balancer |
| `round-robin` | `// TODO: manual mapping for round-robin` | Not migrated - flagged only | Rule `round-robin` names `loadBalance` and emits nothing |
| `until-successful` | `// TODO: manual mapping for until-successful` | Not migrated - flagged only | Rule `until-successful` names `loop` and emits nothing; `maxRetries` and `millisBetweenRetries` are not read |
| `async` | `// TODO: manual mapping for async` | Not migrated - flagged only | No mapping; no `wireTap` |
| `idempotent-message-validator` | `// TODO: manual mapping for idempotent-message-validator` | Not migrated - flagged only | No mapping; no `idempotentConsumer` |
| `batch:job` | `// TODO: manual mapping for batch:job` | Not migrated - flagged only | Rule `batch-job` names `split` and emits nothing. Its `batch:step` and `batch:aggregator` and their processors are not emitted and have no rows; rules `batch-step` and `batch-aggregator` are never reached |

For each of the flagged-only scopes above the request was answered as if the
scope were not there: the log lines in front of and behind it were written,
the processors in it did not run.

### Where a `choice` ends

A Mule `<choice>` ends at `</choice>`: the processors of a `when` or of the
`otherwise` run for the messages that take that branch, the processors after
`</choice>` for every message. The emitted choice is closed with `.end()`,
and the steps after it are steps of the route again:

```java
.choice()
    .when(exchange -> "a".equals(exchange.getProperty("kind")))
        .log(LoggingLevel.INFO, "a")
    .otherwise()
        .log(LoggingLevel.INFO, "another kind")
.end()
.log(LoggingLevel.INFO, "after the choice")
```

A `choice` in a `when` or in the `otherwise` of another choice is closed with
`.end().endChoice()`: `.end()` closes the inner choice, `.endChoice()` gives
the outer one back. What follows belongs to the outer choice: further steps of
the same branch, the next `.when(...)`, the `.otherwise()`, its `.end()`.

```java
.choice()
    .when(exchange -> "a".equals(exchange.getProperty("kind")))
        .choice()
            .when(exchange -> "x".equals(exchange.getProperty("size")))
                .log(LoggingLevel.INFO, "a, x")
            .otherwise()
                .log(LoggingLevel.INFO, "a, another size")
        .end().endChoice()
        .log(LoggingLevel.INFO, "after the inner choice, still kind a")
    .otherwise()
        .log(LoggingLevel.INFO, "another kind")
.end()
.log(LoggingLevel.INFO, "after the outer choice")
```

The rule is the same at every depth and in an on-error scope
(`onException(...)`): a choice among the steps of a route, of an exception
clause or of a split is closed with `.end()`; a choice or a split (`split`,
`foreach`) in a branch of a choice with `.end().endChoice()`.

`.endChoice()` alone, which used to be emitted, does not close a choice: called
on a choice it returns that choice (`ProcessorDefinition.endChoice()`, Camel
4.18.0). The steps after `</choice>` went into the last branch (the
`otherwise`, or the last `when` if there is none) and ran only for the
messages that took it, and the `.when(...)` / `.otherwise()` of an outer choice
were added to the inner one: a wrong branch, or no start at all when both have
an `otherwise` ("Cannot add a 2nd otherwise to this choice"). `.endChoice()` is
not written anywhere but behind the `.end()` of a block in a branch. Behind
the `.end()` of a choice among the steps of a route or of an exception clause
it compiles and the application does not start ("Cannot endChoice() to find
current/parent choice DSL", run). In a split it would end the split to look
for a choice (read in `ProcessorDefinition.endChoice()`, not run).

Checked on an emitted application that was built and run (Quarkus 3.33.1,
Camel 4.18.0), 55 HTTP routes: steps after a choice for each `when` and the
`otherwise`, a choice without `otherwise`, a choice as the last step, two
choices in a row (also inside a `when`), a choice in a `when` and in the
`otherwise` of another one with and without steps around it, three levels
deep, and a choice with a nested one in an `on-error-continue` scope. Every
branch and every step after a choice writes its own log line; each request
logged exactly the lines the Mule flow runs, once each. A `foreach` in a
`when`, with a choice in it, was checked the same way for its closing calls
after its expression was replaced by hand (it is not converted, see
[Not covered](#not-covered)). The Mule side rests on the documentation of the
Choice router (its example logs the reply after `</choice>`, "END OF CHOICE
BLOCK", whichever route ran); no Mule runtime was used.

## Transforms

| Mule element | What is emitted | Class | Notes |
|---|---|---|---|
| `ee:transform` with its script in `<ee:message><ee:set-payload>` | `// // DataWeave: // TODO: manual DataWeave conversion` | Not migrated - flagged only | The same for each of the six scripts probed: field mapping, `filter` / `map`, `as Number` / `as String`, `++`, `default`, `groupBy` / `mapObject`. The script is not found (it is looked for as text of a direct child of `ee:transform`), no bean is written, the payload goes on unchanged |
| `ee:transform` with `<ee:variables><ee:set-variable>` | the same comment | Not migrated - flagged only | The variable is not set |
| `ee:transform` with the script as text of a direct child (`<ee:transform><ee:set-payload>...`, without `ee:message`) | `// // DataWeave: %dw 2.0` and a class `generated/beans/<Flow>Transform.java` | Not migrated - flagged only | The class is a stub that throws `UnsupportedOperationException`, with the script in its Javadoc. The route does not reference it |
| DataWeave converted by a pattern (`core/src/dataweave/patterns.rs`) or by an LLM | | Not measured | No LLM provider was available. The pattern matcher is only called on the path that has one (read in `core/src/dataweave/converter.rs`, not run). Measured: a request to the core binary with an `llm` block gave the same output, byte for byte, as one without |
| `set-payload` / `set-variable` with an expression | see [Processors](#processors-mid-flow) | Converted or Not migrated - fails loudly | [Expressions](#expressions) lists which expressions are converted |
| `#[vars.x]` / `#[flowVars.x]` in a logger message | `${exchangeProperty.x}` | Converted | |
| `#[payload.field]` in a logger message | `${body.field}` | Breaks the build or startup | See `logger` under [Processors](#processors-mid-flow) |
| `parse-template` | `// TODO: manual mapping for parse-template` | Not migrated - flagged only | See [Processors](#processors-mid-flow) |

## Expressions

How a Mule expression (`#[...]`, DataWeave in Mule 4) is converted where it
decides what the route does: the `expression` of a choice `<when>`, the
`value` of `set-variable` / `set-payload` and the `collection` of a `foreach`.

The rule: an expression is reported `DONE` only if the Camel expression reads
the same value and gives the same result **whatever the payload and the values
of the variables are**. Anything else is `MANUAL_REVIEW`, with the Mule
expression kept in a `TODO` in the generated route. No expression is ever
passed on to Camel as it was written (as a predicate `simple("#[...]")` does
not parse: "Unexpected token # at location 0", and the application does not
start; as the expression of a split it is a constant, and the steps run once
for that text).

`MANUAL_REVIEW` comes in two forms. An expression that is **not converted** is
emitted as a step or predicate that throws (see "What an unconverted expression
is emitted as"). A comparison of a variable is **converted on an assumption**
when the migrator cannot tell what the variable holds: the predicate is
emitted and works, with a `TODO` that says what to check (see "When a
comparison is `DONE`").

### Values (`set-variable value`, `set-payload value`)

| Mule value | Camel | Status | Why |
|---|---|---|---|
| no `#[...]`: `value="Hello"`, `value="true"` | `constant("Hello")`, `constant("true")` | `DONE` | A value without `#[...]` is a literal string in Mule, `"true"` included |
| no `#[...]`, with a property placeholder that is left after normalisation: `value="${app.greeting}"`, `value="Hello ${app.name}"` | not converted | `MANUAL_REVIEW` | Mule replaces `${key}` with the value of the property when the application starts. The migration replaces the placeholders whose key is in a `src/main/resources/*.properties` file of the project (the result is the constant above). One that is left would be handed on as the text `${key}`. The usual replacement is a Camel property placeholder, `{{key}}`, resolved from `application.properties`; it is not written automatically |
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
| `#[vars.name == 'A']` | `.when(exchange -> "A".equals(exchange.getProperty("name")))` | `DONE` or `MANUAL_REVIEW`, see "When a comparison is `DONE`" | True only if the property holds a `java.lang.String` with that text (case-sensitive, as in Mule) |
| `#[vars.flag == true]`, `#[vars.flag == false]` | `.when(exchange -> Boolean.TRUE.equals(exchange.getProperty("flag")))`, `Boolean.FALSE.equals(...)` | as above | True only if the property holds a `java.lang.Boolean` with that value. The string `"true"` is not `true` |
| `#[vars.n == 3]` | `.when(exchange -> isNumber(exchange.getProperty("n"), 3))` | as above | True only if the property holds a `java.lang.Number` with that value, whatever its type. The literal has to be a Java `int`. See "Comparing a variable with a literal" |
| `#[vars.id == null]` | `.when(exchange -> exchange.getProperty("id") == null)` | as above | True if the property is not set, or set to null |
| `#[vars.name != 'A']`, `!= true`, `!= 3`, `!= null` | the same predicate with `!` in front: `exchange -> !"A".equals(exchange.getProperty("name"))`, `!Boolean.TRUE.equals(...)`, `!isNumber(..., 3)`; `exchange.getProperty("id") != null` | as above | The exact opposite of `==` for every value: `vars.name != 'A'` is also true when the variable is not set |
| `#['A' == vars.name]` | the predicate of `vars.name == 'A'` | as above | `==` and `!=` are the same with their sides swapped |
| `#[flowVars.name == 'A']` (Mule 3) | the predicate of `vars.name == 'A'` | `MANUAL_REVIEW` (converted on an assumption) | The predicate compares without converting types, as Mule 4 does. What MEL's `==` does with values of different types was not checked |
| `#[vars.a == vars.b]`, `#[vars.a != vars.b]` | not converted | `MANUAL_REVIEW` | Mule's `==` is true only if both values have the same type and are equal. There is no simple predicate that is exactly that for any two Java values; Simple's `==` converts one side to the type of the other |
| `#[true]`, `#[false]` | `constant(true)`, `constant(false)` | `DONE` | |
| `#[payload.type == 'A']` | not converted | `MANUAL_REVIEW` | Field access into the payload, see below |
| `#[payload == 'A']`, `#[payload != null]` | not converted | `MANUAL_REVIEW` | Mule compares the value it reads according to the MIME type of the payload, Simple the body as it arrived. Checked on Camel 4.18: for the JSON body `"A"`, `${body} == 'A'` is false, where Mule's `payload == 'A'` is true |
| `#[attributes.queryParams.id == '1']`, `#[attributes.headers['x'] == 'a']` | not converted | `MANUAL_REVIEW` | Attributes, see below |
| `#[vars.n > 3]` and `<`, `>=`, `<=` | not converted | `MANUAL_REVIEW` | Ordering depends on the types of the two values (number or text), which are not known |
| `#[vars.flag]`, `#[payload]` (a value as the condition) | not converted | `MANUAL_REVIEW` | Mule expects a Boolean; Camel takes every value except `null`, `false`, `"false"` and `""` as true |
| `and` / `or` / `not`, function calls (`#[isEmpty(payload)]`), `~=`, a decimal (`1.5`) or an integer that is not a Java `int`, a string literal with a quote, `\`, `$` or a brace, anything else, a `<when>` without `expression` | not converted | `MANUAL_REVIEW` | Not converted yet, or no Camel predicate that is the same for every value |

### Collections (`foreach collection`, `split expression`)

A `foreach` is a Camel `split`. What it iterates is its `collection`, the
payload if it has none (`mel::convert_collection`):

| Mule | Camel | Status | Why |
|---|---|---|---|
| `<foreach>` without `collection`, `collection="#[payload]"`, `#[message.payload]` | `.split(simple("${body}")).stopOnException()` | `MANUAL_REVIEW` (converted on an assumption) | The same elements only if the body is a Java collection when the split is reached, see below |
| `collection="#[vars.items]"`, `#[vars['items']]`, `#[flowVars.items]` | `.split(simple("${exchangeProperty.items}")).stopOnException()` | `MANUAL_REVIEW` (converted on an assumption) | The same elements only if the exchange property holds a Java collection |
| `#[payload.items]`, `#[payload.lines()]`, `#[vars.order.lines]`, `#[attributes...]`, ranges and list literals (`#[1 to 3]`, `#[['a', 'b']]`), function calls, DataWeave scripts, a literal, text that is not one `#[...]` expression | not converted: a step that throws | `MANUAL_REVIEW` | Field access into the payload, attributes and the value of a variable for the reasons below; the rest has no Camel expression that is the same for every value |
| any collection with a `batchSize` other than `1` | not converted: a step that throws | `MANUAL_REVIEW` | Mule hands the steps lists of that many elements. Camel's split hands over one element at a time; `SplitDefinition` (Camel 4.18.0) has no option that groups the elements of a collection |

No `foreach` is `DONE`. There is no collection expression that Camel's split
iterates like Mule for every value:

* Mule's For Each "can process any collection, including lists and arrays.
  The collection can be any supported content type, such as
  `application/json`, `application/java`, or `application/xml`".
* Camel's split iterates a Java `Iterable`, `Iterator` or array. Run on Camel
  4.18.0 with the emitted `split(simple("${body}"))`: an `ArrayList` and a
  `String[]` of a, b, c give three elements; the body of an HTTP request
  (`["a","b","c"]`, `application/json`, which arrives as
  `org.apache.camel.converter.stream.ByteArrayInputStreamCache`) is handed to
  the steps once, as the whole document; the `String` `a,b,c` is cut at its
  commas into three elements; a `java.util.Map` gives its entries (`k1=v1`).
  With `split(simple("${exchangeProperty.items}"))`: a property set by
  `set-variable value="#[payload]"` holds that stream (one element), a
  property that is not set gives no element at all.

So the conversion is right exactly when the value is a Java collection (the
result of a `sql:` select, for example), and the migrator does not know what
the value is. The split is emitted with the assumption in a `TODO`:

```java
// TODO: review this split (Mule: foreach collection="#[vars.items]")
// - converted assuming the exchange property items holds a Java collection (a java.util.Collection or another Iterable, an Iterator or an array) when the split is reached. Camel's split does not read a value as Mule does. Anything else is handed to the steps in one piece (...), except a String, which is cut at its commas, and a java.util.Map, of which the steps get the entries; nothing is iterated if the property is not set
.split(simple("${exchangeProperty.items}")).stopOnException()
    .log(LoggingLevel.INFO, "item")
.end()
```

(`Mule: foreach without collection` if there is none.)

A collection that is not converted, see "What an unconverted expression is
emitted as":

```java
// TODO: review this foreach, its collection is not converted (Mule: #[payload.items])
// - <reason>
// - the route fails at this step instead of going on with a wrong value; replace it with the converted step
// - the steps of the foreach follow it and are not reached
.throwException(new UnsupportedOperationException("foreach collection not migrated: #[payload.items]"))
.log(LoggingLevel.INFO, "item")
```

What used to be emitted, `.split(simple("#[payload.items]"))`, starts and
answers: Simple takes text without `${...}` as a constant. Run on Camel
4.18.0, the steps ran once, with the text `#[payload.items]` as the element,
and the request got HTTP 200; the foreach was reported `DONE`.

#### What a split does differently from a foreach

Checked on the emitted routes (Camel 4.18.0) against the MuleSoft page of the
For Each scope:

| | Mule (documentation) | Camel split (run) | What the migration does |
|---|---|---|---|
| Payload after the scope | "For Each does not modify the current payload. The output payload is the same as the input." | The body after `.end()` is the body from before the split, also when a step in the split sets the body | Nothing to do |
| An element whose steps fail | "the For Each scope stops processing that collection and invokes the error handler" | Without `stopOnException()` the steps run for every element and the exchange fails afterwards; with it the split stops at the first one | `stopOnException()` on the split of a `foreach`. An `onException` clause of the route then runs once and its result is the reply (checked with an `on-error-continue` without `type`). The exception that leaves the split is a `CamelExchangeException` ("Multicast processing failed for number 0") with the original as its cause |
| A variable set in the scope | "New variables or modifications to existing variables that take place when processing one element are visible during the processing of another element. These changes to variables continue to be available outside the For Each scope." | The steps run on a copy of the exchange for each element: a property set for one element is not set for the next one, nor after the split (a `choice` on it after the scope took `otherwise`) | Not bridged. The steps are emitted as they are; the `TODO` and the report name the variables the steps set (`set-variable`, `remove-variable`, a `target`, ...). A `choice` on such a variable is `MANUAL_REVIEW` as well, see "When a comparison is `DONE`" |
| `vars.counter`, `vars.rootMessage` | `counterVariableName` (default `counter`): "Name of the variable that stores the number of messages over which it iterates"; `rootMessageVariableName` (default `rootMessage`): "the complete, non-split message" | Neither property is set. The split sets `CamelSplitIndex` (0, 1, 2 for three elements) and `CamelSplitSize` | Not bridged. If a step in the scope reads `vars.counter` / `vars.rootMessage` (or the names the foreach gives them), the `TODO` and the report say so. A logger with `#[vars.counter]` logs an empty value |
| `batchSize` | "Partitions the collection into sub-collections of the specified size" | No such option for a collection | Not converted: a step that throws, see the table above |

A `flow-ref` in the scope is named in the `TODO` as well: what the flow it
calls sets or reads is not followed.

A split none of whose steps is written (a `foreach` of elements that are not
migrated, which are comments) is left out, with a `TODO`: Camel does not start
a route with a split without steps ("Definition has no children on Split").

`scatter-gather` sends the message to each of its routes and collects the
results; it iterates nothing. It was emitted as a split of
`simple("#[payload]")` whose steps were comments, and the application did not
start (the error above). It is now a step that throws
`UnsupportedOperationException("scatter-gather not migrated")` with a `TODO`,
`MANUAL_REVIEW`; its routes are not mapped.

`<split expression="...">` is converted like a collection, without
`stopOnException()` and always `MANUAL_REVIEW`: it is not an element of Mule 4,
and what it did in the application has to be checked.

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

### Comparing a variable with a literal

DataWeave's `==` does not convert types: `"true" == true` is false (`~=` is
the operator that coerces). Camel Simple's `==` does: it converts one side to
the type of the other before it compares. Checked on Camel 4.18: with
`${exchangeProperty.x} == true` an exchange property holding the string
`"true"` equals `true`; with `${exchangeProperty.x} == 5` one holding the
string `"5"`, the `Double` 5.5 or 5.9, the `BigDecimal` 5.5 or the `Boolean`
`true` equals `5`. In Mule all of these comparisons are false. So a comparison
is not written in Simple. (It was, up to this version: after
`<set-variable variableName="enriched" value="true" />` the condition
`#[vars.enriched == true]` took the `when` branch where Mule takes
`otherwise`.)

The condition is a Java predicate on the value of the exchange property (a
variable is an exchange property, see "Values"):

| Literal | The property equals it if it holds |
|---|---|
| a string, `'A'` | a `java.lang.String` with exactly that text |
| `true` / `false` | a `java.lang.Boolean` with that value |
| an integer, `3` | a `java.lang.Number` with that value: `Integer` 3, `Long` 3, `Double` 3.0 and `BigDecimal` 3.0 equal 3; `Double` 3.5, the string `"3"` and a `Boolean` do not. Nothing is truncated or rounded |
| `null` | nothing: the property is not set, or set to null |

`!=` is the exact opposite of `==` in every case. A route class that compares
a number gets the method that does it:

```java
private static boolean isNumber(Object value, long expected) {
    if (value instanceof java.math.BigDecimal decimal) {
        return decimal.compareTo(java.math.BigDecimal.valueOf(expected)) == 0;
    }
    if (value instanceof java.math.BigInteger integer) {
        return integer.equals(java.math.BigInteger.valueOf(expected));
    }
    return value instanceof Number number
            && number.doubleValue() == expected
            && number.longValue() == expected;
}
```

Checked on a running generated application (Quarkus 3.33.1, Camel 4.18.0):
the ten predicates for `==` and `!=` against `'A'`, `true`, `false`, `5` and
`null`, each with 37 values of the property - `String` (`A`, `B`, `a`, `true`,
`false`, `5`, `5.0`, the empty string, `null` as text), `Boolean`, `Byte`,
`Short`, `Integer`, `Long` (also 2^32+5), `Float`, `Double` (5.0, 5.5, 5.9,
NaN), `BigDecimal` (5, 5.0, 5.5, 5.00000000000000000001), `BigInteger` (5,
2^64+5), `AtomicInteger`, `AtomicLong`, `StringBuilder`, `Character`, null and
not set. The values were put into the property by a hand-written processor in
front of the generated routes, which themselves only set constants. All 370
answers are the ones the table above gives.

### When a comparison is `DONE`

The predicate is exact for the Java value in the exchange property. It gives
Mule's result if the property holds what the Mule variable holds, and the
migrator knows that in one case only: the variable is set by a
`<set-variable>` with a literal value, which is emitted as `setProperty` with
a constant of the same type (`value="true"` is the string `"true"` in both,
`#[3]` a number, `#[true]` a boolean).

A choice is therefore `DONE` only if, for every variable it compares,

* the application sets the variable at least once, and
* **every** place in the application that can set it - all flows and
  sub-flows, their error handlers and the global elements included - is a
  `<set-variable>` whose value is a literal (text without `#[...]` and without
  a `${...}` placeholder, `#['text']`, `#[42]`, `#[true]`), without `mimeType`,
  at a place the generated route runs as Mule does: directly in a flow or
  sub-flow, in a branch of a `choice`, or in an `on-error-continue` /
  `on-error-propagate` scope for every error (no `type` or `ANY`, no `when`).

Each of the following also counts as setting the variable, and is not a
literal:

* a `set-variable` with any other value: `#[payload]` (the property then holds
  the body as it arrived, not the value Mule reads from it), `#[vars.other]`,
  a value that is not converted;
* a `set-variable` with a `mimeType` (it is ignored by the migration and
  changes how Mule reads the value), or whose `variableName` contains an
  unresolved `${...}` placeholder (it may be any name);
* any other element with an attribute `target`, or an attribute whose name
  contains `variable` (`variableName`, `targetVariable`,
  `counterVariableName`, ...), that mentions the variable: `ee:set-variable`
  in an `ee:transform`, the `target` of a connector operation, a `flow-ref`
  or a scope, a Mule 3 enricher;
* a `foreach`, for `counter` and `rootMessage` (unless it renames them);
* a `remove-variable` (it is not migrated: the property keeps its value);
* a `set-variable` with a literal inside anything else: a `foreach` (a Camel
  `split`, whose steps work on a copy of the exchange), a scope whose steps
  are not emitted (`try`, `async`, `until-successful`, ...), an on-error scope
  with a `type` or a `when` (emitted as an approximation or commented out), a
  global error handler (not migrated);
* a `flow-ref` to a flow that is not among the migrated flows: what it sets is
  not known.

If there is such an assignment, or none at all, the choice is `MANUAL_REVIEW`.
The predicate is emitted all the same, so the route works; the route and the
report name the assumption and the assignments it is about:

```java
// TODO: review this condition (Mule: #[vars.originalPayload == 'A'])
// - converted assuming variable originalPayload holds a value this application set from a literal; it is also set at: set-variable in flow aliasFlow from #[payload]. Check the comparison
.when(exchange -> "A".equals(exchange.getProperty("originalPayload")))
```

This one does differ from Mule. Checked on Camel 4.18 with a `platform-http`
route: after `setProperty("originalPayload", simple("${body}"))` the predicate
is false for the request body `A` (`text/plain`) and for `"A"`
(`application/json`): the property holds the body as Camel's stream cache
(`org.apache.camel.converter.stream.ByteArrayInputStreamCache`), not a
`String`. Mule compares the value it reads from the payload.

The scan reads the Mule XML of the application. A variable set by code it does
not read (a custom Java component, a script) is not seen.

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

  The message of the exception quotes at most the first 200 characters of the
  Mule expression, followed by `... (truncated, full expression in the TODO
  above)`; the `TODO` comment has all of it. (A Java string constant holds at
  most 65535 bytes: quoted whole, an expression of 81 KB made the route class
  fail to compile, "constant string too long".) The same holds for the step
  below.

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

* **`foreach` collection**: the split is replaced by a step that throws
  `UnsupportedOperationException("foreach collection not migrated: ...")`
  (same `TODO`, same cut of a long expression). The steps of the scope are
  emitted behind it, as steps of the route (or of the branch the foreach is
  in): they are not reached, so they do not run once for the whole payload,
  and there is no split with a made-up expression for them to be in.

The exception is one like any other. If the route has an `onException` clause
that takes every exception (a Mule `on-error-continue` / `on-error-propagate`
without `type`, or with `ANY`), that clause handles the "not migrated"
exception too, and the reply is then the handler's. Checked on Camel 4.18: a
flow whose `set-payload value="#[now()]"` is not converted, with an
`on-error-continue` without `type`, answers HTTP 200 with the payload the
handler sets. The failure is then only visible in the `TODO` and in the
report.

The generated documentation follows: `docs/flows/<flow>.md` lists such a step
as "**Not migrated:** set message body - ... the route fails at this step" and
a choice with the number of its conditions that are not migrated or converted
on an assumption; the route diagram labels the step
`NOT MIGRATED: setBody (fails here)`. A foreach whose collection is not
converted is "**Not migrated:** split - ..." and
`NOT MIGRATED: split (fails here)`; one that is converted on an assumption is
"Split (to review: ...)" on the page of its flow.

The generated smoke test of such a route (`testRoute...Returns200`) fails
until the expression is replaced. That is intended: the route is not migrated
yet.

### In the report

A choice is one row: `DONE` if every condition is converted and none rests on
an assumption (see "When a comparison is `DONE`"), otherwise `MANUAL_REVIEW`
with the unconverted conditions and the reasons, and with the conditions that
are converted on an assumption and the assignments they are about. A
`set-variable` / `set-payload` is one row, `MANUAL_REVIEW` if its value is not
converted. A `foreach` is one row, always `MANUAL_REVIEW`: with the assumption
its split rests on, or with the collection that is not converted and why, and
with what its steps rely on that a split does not do. A mapping rule without `camel_uri` (`set-payload`, `set-variable`,
`choice-router`, `transform-message`, ...) no longer adds a `DONE` row of its
own next to the row of the built-in mapping; its id is shown on that row.

### Not covered

* `foreach`: no collection is converted without an assumption. A foreach
  over a value that is known to be a Java collection (after a database
  select, say) is not recognised as such and is `MANUAL_REVIEW` like the
  others. Ranges and list literals (`#[1 to 3]`, `#[['a', 'b']]`), field
  access (`#[payload.items]`) and `batchSize` are not converted.
* `foreach`: a variable set in the scope is not carried to the next element
  or out of the split, and `counter` / `rootMessage` are not set; both are
  reported, not bridged. A flow called from the scope with `flow-ref` is not
  searched for either.
* `scatter-gather` is not migrated (a step that throws); its routes are not
  mapped. `parallel-foreach` has no mapping.
* `mimeType` / `encoding` of `set-payload` and `set-variable` are ignored.
* A variable is an exchange property (`setProperty`), as before. Camel 4.4+
  also has variables (`setVariable`, `${variable.name}`); the mapping was not
  changed to them.
* What MEL (Mule 3) does for `flowVars.x == literal` with values of different
  types was not checked; such a comparison is always `MANUAL_REVIEW`.

### Documentation this is based on

* Apache Camel, Simple language (functions, "Built-in Operators", OGNL):
  <https://camel.apache.org/components/next/languages/simple-language.html>.
  "Camel operators require the left value must be enclosed in `${ }`", "There
  must be spaces around the operator", "Camel will automatically type convert
  the rightValue type to the leftValue type".
* Apache Camel, Choice EIP:
  <https://camel.apache.org/components/next/eips/choice-eip.html>. "The first
  when clause that matches determines which route branch to follow." On the
  Java DSL ("Why can I not use otherwise in Java DSL?"): "you do by using
  `.endChoice()`, which tells Camel to "pop the stack" and return to the scope
  of the Choice EIP"; the Javadoc of `ProcessorDefinition.endChoice()` (Camel
  4.18.0): "If you want to end the entire choice block, then use `end()`
  instead."
* Apache Camel, Set Property, Set Body and Set Variable EIPs:
  <https://camel.apache.org/components/next/eips/setProperty-eip.html>,
  <https://camel.apache.org/components/next/eips/setBody-eip.html>,
  <https://camel.apache.org/components/next/eips/setVariable-eip.html>.
* MuleSoft, Choice router:
  <https://docs.mulesoft.com/mule-runtime/latest/choice-router-concept>. "the
  first expression that evaluates to `true` triggers that route's execution
  and the others are not checked"; its example is
  `#[vars.language == 'Spanish']`.
* MuleSoft, For Each scope:
  <https://docs.mulesoft.com/mule-runtime/latest/for-each-scope-concept>
  (read 2026-11-24; quoted under "Collections"). Its reference table:
  `collection`, default `payload`, "An expression that returns a Java
  collection, object array, map, or DOM nodes"; `batchSize`, default `1`. The
  page does not say what the elements of a map are, what happens with a
  collection that is null or a string, or where `counter` starts.
* Apache Camel, Split EIP:
  <https://camel.apache.org/components/next/eips/split-eip.html>; the options
  of the Java DSL were read from `org.apache.camel.model.SplitDefinition` of
  Camel 4.18.0 (`javap`).
* MuleSoft, Set Payload transformer:
  <https://docs.mulesoft.com/mule-runtime/latest/set-payload-transformer-reference>.
  The value "Accepts a literal string or DataWeave expression".
* MuleSoft, DataWeave operators:
  <https://docs.mulesoft.com/dataweave/latest/dw-operators>. `("true" == true)`
  is false; `~=` is the "Equality operator that tries to coerce one value to
  the type of the other when the types are different".

The Camel side was run, not only read: the emitted forms above (the
comparison predicates with the values listed under "Comparing a variable with
a literal", both unconverted forms, the cut exception message, an unresolved
and a resolved property placeholder) were compiled into Camel Quarkus
applications (Quarkus 3.33.1, Camel 4.18.0) and called over HTTP. The Mule
side rests on the documentation; no Mule runtime was used.

So was what is said under "Collections": a Mule application with one
sub-flow per `foreach` shape (no collection, `#[payload]`, `#[vars.items]`,
`#[payload.items]`, `#[1 to 3]`, `batchSize="2"`, a `set-variable` in the
scope, a logger of `vars.counter`, a `set-payload` in the scope, a step that
fails in the scope, a scope without a migrated step), a flow with a foreach
and an `on-error-continue`, and a `scatter-gather` was migrated before and
after the change, built with `mvn package` and called over HTTP. One
hand-written class was added to the emitted project: routes that put an
`ArrayList`, a `String[]`, a `Map` or a `String` into the body or the property
`items` and call the emitted sub-flow routes through `direct:`, the route
that the scopes call per element (it logs the body and the split
properties), and two splits with and without `stopOnException()`. The
emitted routes were not changed. Not checked: an `onException` clause for a
specific exception class when the failure comes out of a split wrapped in a
`CamelExchangeException`.

## Error Handling

| Mule element | What is emitted | Class | Notes |
|---|---|---|---|
| `on-error-continue` without `type`, or with `ANY` | `onException(java.lang.Exception.class).handled(true)` with the steps of the scope | Converted | Built and started; no error was raised in this probe, so the clause itself did not run here. What it handles is described below the table |
| `on-error-propagate` without `type`, or with `ANY` | `onException(java.lang.Exception.class).handled(false)` with the steps of the scope | Converted | As above |
| an on-error scope with a known `type` (probe: `HTTP:CONNECTIVITY`) | `onException(java.net.ConnectException.class)` with a `TODO` | Converted, to review | The nearest Java exception class, which is never exactly the Mule error type. The known types are listed below |
| an on-error scope with any other `type` (probe: `APP:INVALID_ORDER`) | the clause, commented out, with a `TODO` | Not migrated - flagged only | A type without known class is never widened to `Exception`. A scope with a `when` and a scope shadowed by an earlier one are treated the same way (not probed here, see below) |
| `<error-handler ref="...">` | nothing in the route | Not migrated - flagged only | The report row is the only sign; the global error handler is not migrated |
| `try` | `// TODO: manual mapping for try` | Not migrated - flagged only | Rule `try-scope` names `doTry` and emits nothing. The processors of the scope and its `error-handler` are not emitted and have no rows: the request was answered without them |
| `raise-error` | `// TODO: manual mapping for raise-error` | Not migrated - flagged only | No mapping; no `throwException`. The route goes on where the Mule flow raises an error |
| `validation:is-true` | `// TODO: manual mapping for validation:is-true` | Not migrated - flagged only | No mapping; no `validate()`. The route goes on where the Mule flow may fail |

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

What a migration writes next to the routes. Checked on one probe, migrated
with `emit_kong_config`: the files are there. Unless a note says otherwise
their content was not checked and nothing was built, applied or deployed from
them.

| Output | Written | Notes |
|---|---|---|
| Java RouteBuilder classes | yes | `src/main/java/generated/routes/<Flow>Route.java`, one per flow and per sub-flow that is not inlined |
| `pom.xml` | yes | Quarkus 3.33.1 with the Camel Quarkus extensions of the rules and built-in mappings that matched. `mvn package -DskipTests` passed for 97 of the 98 probes. A JMS client, `camel-quarkus-seda`, `camel-quarkus-bean`, a JDBC driver and a data source are not added when the routes need them (see [Known defects](#known-defects)) |
| `application.properties`, `application.prod.properties` | yes | HTTP port (always 8080), application name, health, Camel context name. A JMS probe additionally got `quarkus.artemis.*` keys; the database and Kafka probes got nothing about a data source or brokers |
| `Dockerfile` | yes | Stages `build`, `jvm`, `native-build` and `native`. Not built |
| Kubernetes manifests | yes | `k8s/base` (deployment, service, configmap, kustomization), `k8s/overlays/dev` and `prod` (with an HPA). Not applied |
| Kong Gateway config | yes, with `emit_kong_config` | `kong/kong.yaml`. Not validated |
| GitHub Actions workflows | yes | `.github/workflows/ci.yaml` and `release.yaml`. Not run |
| Smoke tests (JUnit 5) | yes | `src/test/java/generated/<Flow>RouteTest.java`. Compiled in 97 probes, not run. For an `http:listener` without `allowedMethods` the test does not compile |
| Migration report | yes | `MIGRATION_REPORT.md`: one row per element that was mapped, with status, rule id and rationale. Its `File` and `Flow` columns are empty and its `Line` column is not a line number (543 for an element on line 10 of a file of 26 lines). The processors in a scope that is not migrated have no row. No row had the status `Skipped` in any probe |
| Project documentation | yes | `README.md`, `CHECKLIST.md`, `docs/architecture.md`, `docs/contributing.md`, `docs/development/local-setup.md` and `testing.md`, `docs/flows/<flow>.md`, `docs/migration/overview.md`, `docs/operations/runbook.md`, `docs/route-diagram.md` |
| `Makefile`, `docker-compose.yml`, `.env.example` | yes | |

## Project content

What is not a Mule element in a flow.

| In the Mule project | What is written | Notes |
|---|---|---|
| Java classes under `src/main/java` | The file at the same path, with a comment in front ("copied from original Mule project", a `TODO` to review Mule-specific imports, the path of the original) | The element that calls the class (`java:invoke-static`) is not migrated, see [Processors](#processors-mid-flow) |
| MUnit suites under `src/test/munit` | A JUnit class per suite under `src/test/java/generated/munit`, with a method per `munit:test` | Each method only calls `fail("Not yet implemented — port from MUnit")`. Not run |
| API specifications (probe: a RAML file under `src/main/resources/api`) | The file at the same path | |
| Kafka connector 4.x subscription on the `consumer-config` connection (topic patterns, group ID, bootstrap servers) | Nothing | `kafka:message-listener` is flagged for manual review, see [Sources](#sources-flow-entry-points) |

Not measured: Mule domain projects, API autodiscovery and API Manager
integration, CloudHub features (persistent queues, worker settings), MEL
expressions of Mule 3 other than `flowVars.x`, connectors that are not in the
tables above.

## Mapping rules

Every rule that is loaded from `mappings/*.yaml`, and what the probes showed
it does. A test (`every_mapping_rule_has_a_row_in_the_mapping_matrix` in
`core/tests/integration_test.rs`) fails when a rule has no row here or a row
has no rule. Adding a row means measuring: migrate an application with the
element and write down what is emitted.

A rule with a `camel_uri` is the whole mapping of its element: the URI
template with the attributes of the element filled in, reported `DONE`. A
rule without one only puts its id on the report row of the built-in mapping,
if there is one, and adds its dependencies.

The two files in `mappings/connectors/` (`http.yaml`, `kafka.yaml`, with rules
such as `mule.http.listener` and `mule.anypoint-mq.subscriber`) are not
loaded: the loader (`mapper::load_rules`) reads the files directly in
`mappings/` and the list under their `mappings:` key, and these files have a
`rules:` key. The probes agree: the Anypoint MQ elements those files name got
no rule.

| Rule id | File | Mule element | `camel_uri` | Class of the probe(s) |
|---|---|---|---|---|
| `amqp-listener` | `amqp.yaml` | `amqp:listener` | `amqp:queue:{destination}` | Wrong, reported `DONE` |
| `amqp-publish` | `amqp.yaml` | `amqp:publish` | `amqp:queue:{destination}` | Wrong, reported `DONE` |
| `batch-job` | `batch.yaml` | `batch:job` | none | Not migrated - flagged only |
| `batch-step` | `batch.yaml` | `batch:step` | none | Never reached: a `batch:job` is flagged only and what is in it is not mapped |
| `batch-aggregator` | `batch.yaml` | `batch:aggregator` | none | Never reached, as `batch-step` |
| `until-successful` | `batch.yaml` | `until-successful` | none | Not migrated - flagged only |
| `choice-router` | `choice.yaml` | `choice` | none | Converted; Not migrated - fails loudly |
| `scatter-gather` | `choice.yaml` | `scatter-gather` | none | Not migrated - fails loudly |
| `first-successful` | `choice.yaml` | `first-successful` | none | Not migrated - flagged only |
| `round-robin` | `choice.yaml` | `round-robin` | none | Not migrated - flagged only |
| `db-select` | `db.yaml` | `db:select` | `sql:{query}?dataSource=default` | Breaks the build or startup |
| `db-insert` | `db.yaml` | `db:insert` | `sql:{query}?dataSource=default` | Breaks the build or startup |
| `db-update` | `db.yaml` | `db:update` | `sql:{query}?dataSource=default` | Breaks the build or startup |
| `db-delete` | `db.yaml` | `db:delete` | `sql:{query}?dataSource=default` | Breaks the build or startup |
| `db-stored-procedure` | `db.yaml` | `db:stored-procedure` | `sql-stored:{procedureName}?dataSource=default` | Breaks the build or startup |
| `email-listener` | `email.yaml` | `email:listener-imap` | `imap://{host}:{port}` | Breaks the build or startup |
| `email-send` | `email.yaml` | `email:send` | `smtp://{host}:{port}` | Breaks the build or startup |
| `email-listener-pop3` | `email.yaml` | `email:listener-pop3` | `pop3://{host}:{port}` | Breaks the build or startup |
| `on-error-propagate` | `error-handler.yaml` | `on-error-propagate` | none | By `type`, see [Error Handling](#error-handling). The id does not appear in the report |
| `on-error-continue` | `error-handler.yaml` | `on-error-continue` | none | By `type`, see [Error Handling](#error-handling). The id does not appear in the report |
| `try-scope` | `error-handler.yaml` | `try` | none | Not migrated - flagged only |
| `file-listener` | `file.yaml` | `file:listener` | `file:{directory}?recursive={recursive}` | Wrong, reported `DONE`; Breaks the build or startup |
| `file-read` | `file.yaml` | `file:read` | `file:{directory}` | Wrong, reported `DONE` |
| `file-write` | `file.yaml` | `file:write` | `file:{path}` | Wrong, reported `DONE` |
| `ftp-listener` | `ftp.yaml` | `ftp:listener` | `ftp://{host}:{port}/{directory}` | Breaks the build or startup |
| `ftp-read` | `ftp.yaml` | `ftp:read` | `ftp://{host}:{port}/{directory}` | Wrong, reported `DONE` |
| `ftp-write` | `ftp.yaml` | `ftp:write` | `ftp://{host}:{port}/{path}` | Wrong, reported `DONE` |
| `http-listener` | `http.yaml` | `http:listener` | `platform-http:{path}?httpMethodRestrict={allowedMethods}` | Converted; Breaks the build or startup |
| `http-request` | `http.yaml` | `http:request` | `http:{url}{path}?httpMethod={method}` | Wrong, reported `DONE` |
| `jms-listener` | `jms.yaml` | `jms:listener` | `jms:queue:{destination}` | Breaks the build or startup |
| `jms-publish` | `jms.yaml` | `jms:publish` | `jms:queue:{destination}` | Wrong, reported `DONE` |
| `jms-consume` | `jms.yaml` | `jms:consume` | `jms:queue:{destination}?receiveTimeout=10000` | Wrong, reported `DONE` |
| `kafka-consumer` | `kafka.yaml` | `kafka:consumer` | `kafka:{topic}` | Converted |
| `kafka-listener` | `kafka.yaml` | `kafka:listener` | `kafka:{topic}` | Converted |
| `kafka-publish` | `kafka.yaml` | `kafka:publish` | `kafka:{topic}` | Converted |
| `os-store` | `objectstore.yaml` | `os:store` | `infinispan:objectStore?operation=PUT` | Breaks the build or startup |
| `os-retrieve` | `objectstore.yaml` | `os:retrieve` | `infinispan:objectStore?operation=GET` | Breaks the build or startup |
| `vm-publish` | `objectstore.yaml` | `vm:publish` | `seda:{queueName}` | Breaks the build or startup |
| `vm-listener` | `objectstore.yaml` | `vm:listener` | `seda:{queueName}` | Breaks the build or startup |
| `vm-consume` | `objectstore.yaml` | `vm:consume` | `seda:{queueName}?timeout=10000` | Breaks the build or startup |
| `sfdc-query` | `salesforce.yaml` | `salesforce:query` | `salesforce:query?sObjectQuery={query}` | Breaks the build or startup |
| `sfdc-create` | `salesforce.yaml` | `salesforce:create` | `salesforce:createSObject?sObjectName={type}` | Breaks the build or startup |
| `sfdc-update` | `salesforce.yaml` | `salesforce:update` | `salesforce:updateSObject?sObjectName={type}` | Breaks the build or startup |
| `sfdc-delete` | `salesforce.yaml` | `salesforce:delete` | `salesforce:deleteSObject?sObjectName={type}` | Breaks the build or startup |
| `sfdc-upsert` | `salesforce.yaml` | `salesforce:upsert` | `salesforce:upsertSObject?sObjectName={type}&sObjectIdName={externalIdFieldName}` | Breaks the build or startup |
| `sfdc-subscribe-topic` | `salesforce.yaml` | `salesforce:subscribe-topic` | `salesforce:subscribe?topicName={topic}` | Breaks the build or startup |
| `scheduler-fixed` | `scheduler.yaml` | `scheduler` | `timer:scheduler?period={frequency}` | Wrong, reported `DONE` |
| `scheduler-cron` | `scheduler.yaml` | `scheduling-strategy:cron-scheduler` | `quartz:scheduler?cron={expression}` | Matches nothing. A scheduler with a `cron` strategy is Wrong, reported `DONE`, under `scheduler-fixed` |
| `sftp-listener` | `sftp.yaml` | `sftp:listener` | `sftp:{host}:{port}/{directory}` | Breaks the build or startup |
| `sftp-read` | `sftp.yaml` | `sftp:read` | `sftp:{host}:{port}/{directory}` | Wrong, reported `DONE` |
| `sftp-write` | `sftp.yaml` | `sftp:write` | `sftp:{host}:{port}/{path}` | Wrong, reported `DONE` |
| `wsc-consume` | `soap.yaml` | `wsc:consume` | `cxf://{address}?serviceClass={serviceClass}&wsdlURL={wsdlLocation}` | Breaks the build or startup |
| `ws-consumer` | `soap.yaml` | `ws:consumer` | `cxf://{address}?wsdlURL={wsdlLocation}` | Breaks the build or startup |
| `set-payload` | `transform.yaml` | `set-payload` | none | Converted; Not migrated - fails loudly |
| `set-variable` | `transform.yaml` | `set-variable` | none | Converted |
| `remove-variable` | `transform.yaml` | `remove-variable` | none | Not migrated - flagged only |
| `transform-message` | `transform.yaml` | `ee:transform` | none | Not migrated - flagged only |
| `foreach` | `transform.yaml` | `foreach` | none | Converted, to review; Not migrated - fails loudly |
| `parse-template` | `transform.yaml` | `parse-template` | none | Not migrated - flagged only |

## How this was measured

On `main` at `5be2a0f`, 2026-11-25, with the core binary
(`core/target/debug/muleforge-core`, built from that commit) and the rules in
`mappings/`:

1. **Probes.** 98 Mule applications, one per element or variant of an
   element: 23 with the element as the source of a flow, 70 with it between
   two `logger` steps in a flow that starts with
   `<http:listener path="/probe" allowedMethods="GET"/>`, 5 with an error
   handler on such a flow. Connector elements have a config element with a
   connection, as Anypoint Studio writes them. One more application has a
   Java class, an MUnit suite and a RAML file, for the tables
   [Infrastructure Output](#infrastructure-output) and
   [Project content](#project-content).
2. **Migration.** Each application was migrated with
   `{"method":"migrate","params":{"input_path":...,"output_path":...,"mappings_dir":...}}`
   on the standard input of the binary. Recorded: the rows of
   `MIGRATION_REPORT.md` for the element (status, rule id, rationale) and the
   lines of the route class between the two `logger` steps (or its
   `from(...)`).
3. **Build.** `mvn -B -q -DskipTests package` in each emitted project
   (Maven 3.9, JDK 21, Quarkus 3.33.1, Camel 4.18.0). This compiles the
   generated tests and does not run them. 97 passed. The one that failed
   (`http:listener` without `allowedMethods`) was built again with
   `-Dmaven.test.skip=true`.
4. **Start.** `java -Dquarkus.http.port=<port> -jar target/quarkus-app/quarkus-run.jar`
   for each project that was built, without any broker, database or server
   behind it. "Started" is the line `started in ...s. Listening on:` within
   40 s; otherwise the cause was taken from the log. 69 started, 29 did not
   (among them the one built without its tests).
5. **One request.** `GET /probe` for the applications that started (`POST`
   and `GET /orders` for the `http:listener` probe). Recorded: the status,
   and whether the log line of the `logger` behind the element was written.

The emitted projects were not changed. What a row says about a class is what
its probe showed; a different use of the same element (other attributes,
another place in the flow) may come out differently. Not done: no Mule
runtime was used, no generated test was run, no connector was run against
its broker, database or server, no LLM provider was configured.

Probes per class: Converted 14, Converted, to review 3, Wrong, reported `DONE`
16, Breaks the build or startup 29, Not migrated - fails loudly 5,
Not migrated - flagged only 31.

## Known defects

Found by the measurement above. Each is what was observed, in one line; none
is fixed.

1. A rule with a `camel_uri` leaves every placeholder that is not an attribute of the element in the URI and reports `DONE`: `sql:{query}`, `sql-stored:{procedureName}`, `http:{url}...`, `amqp:queue:{destination}`, `sftp:{host}:{port}/...`, `ftp://{host}:{port}/...`, `imap://{host}:{port}`, `pop3://{host}:{port}`, `smtp://{host}:{port}`, `file:{directory}`, `timer:scheduler?period={frequency}`, `cxf://{address}?...`, `salesforce:query?sObjectQuery={query}`, `...?sObjectName={type}`, `file:in?recursive={recursive}`, `platform-http:/orders?httpMethodRestrict={allowedMethods}`.
2. `http:listener` without `allowedMethods`: the generated smoke test does not compile (`.when().{allowedmethods}("/orders")`), and the application does not start (`No enum constant ...Method.{ALLOWEDMETHODS}`).
3. `logger` with `#[payload.field]` in its message is reported `DONE` and the application does not start: `No language could be found for: bean`.
4. `vm:listener`, `vm:publish`, `vm:consume`: `seda:` endpoints without `camel-quarkus-seda` in the `pom.xml`; the application does not start.
5. `jms:listener`, `jms:publish`, `jms:consume`: no JMS client and no connection factory in the emitted project (`connectionFactory must be specified`); a listener stops the application from starting. A topic listener is emitted as `jms:queue:`.
6. `db:select`, `db:insert`, `db:update`, `db:delete`, `db:stored-procedure`: the SQL text and its parameters are not in the route, and `dataSource=default` stops the application from starting.
7. `scheduler`: the frequency is not read (the timer fired once), a `cron` strategy is ignored, and rule `scheduler-cron` matches nothing.
8. `http:request`: the host and port of its `http:request-config` are not read, and behind an `http:listener` the step fails with `Invalid url: ... enable the bridgeEndpoint option`.
9. `file:read`, `sftp:read`, `ftp:read` and `jms:consume` are emitted as producer steps (`.to(...)`): `file:read` wrote instead of reading.
10. `file:write` writes to a directory named after the file (`out/result.txt/<message id>`); the `workingDir` of a `file:config` is dropped for `file:listener` and `file:write`.
11. `salesforce:*`: nothing of the connector config is carried over and the application does not start (`loginUrl must be specified`); `salesforce:subscribe-topic` gives an endpoint Camel rejects (`topicName must be supplied`).
12. `wsc:consume`, `ws:consumer`, `os:store`, `os:retrieve`, `email:send`, `email:listener-imap`, `email:listener-pop3`, `sftp:listener`, `ftp:listener`: reported `DONE`, and the application does not start.
13. The processors in a scope that is not migrated (`async`, `try`, `until-successful`, `first-successful`, `round-robin`, `parallel-foreach`, `batch:job`, the routes of a `scatter-gather`) are not emitted and have no row in the report.
14. A flow without source: its first processor is taken as the source, reported as `Unknown source` and not emitted; a `set-payload` there also gets a `DONE` row.
15. `ee:transform` as Mule 4 writes it (`ee:message` / `ee:set-payload`): the script is not found, the comment in the route does not quote it and no bean is written. Where a stub bean is written, the route does not reference it.
16. Rules that emit nothing: `remove-variable`, `parse-template` (adds a dependency only), `first-successful`, `round-robin`, `until-successful`, `try-scope`, `batch-job`, `batch-step`, `batch-aggregator`, `scheduler-cron`.
17. The files in `mappings/connectors/` are not loaded.
18. `MIGRATION_REPORT.md`: the `File` and `Flow` columns are empty and `Line` is not a line number.
19. The core binary ignores the `llm` block of a request: the output is the same with and without it. (Read in the code, not run: the DataWeave pattern matcher is only called on the path that has an LLM provider.)
20. `application.properties` gets `quarkus.artemis.*` keys for JMS without an extension that reads them, and nothing for a database or for Kafka.
