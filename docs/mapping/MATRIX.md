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
| `choice` | `choice()` | ⚠️ Partial | Every `when` and the `otherwise` are kept, in order. Auto (`DONE`) when every condition is one of the converted shapes under [Expressions](#expressions) and every variable it compares is only set from literals; otherwise the choice is flagged `MANUAL_REVIEW`: an unconverted condition fails when it is evaluated, a comparison of a variable that may hold something else is emitted with a `TODO`. The choice is closed with `.end()`, so the steps after `</choice>` follow it for every message; a `choice` in a `when` or `otherwise` of another one is closed with `.end().endChoice()`, see [Where a `choice` ends](#where-a-choice-ends) |
| `scatter-gather` | `multicast()` | ✅ Auto | |
| `split` | `split()` | ✅ Auto | Parallel flag preserved |
| `foreach` | `split()` | ✅ Auto | Sequential split |
| `first-successful` | Failover LB | ✅ Auto | Via mapping YAML |
| `round-robin` | Round-robin LB | ✅ Auto | Via mapping YAML |
| `until-successful` | Error handler retry | ✅ Auto | maxRetries + delay |
| `async` | `wireTap()` | ✅ Auto | Fire-and-forget |
| `idempotent-message-validator` | `idempotentConsumer()` | ✅ Auto | |

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
the same value and gives the same result **whatever the payload and the values
of the variables are**. Anything else is `MANUAL_REVIEW`, with the Mule
expression kept in a `TODO` in the generated route. No expression is ever
passed on to Camel as it was written (`simple("#[...]")` does not parse:
"Unexpected token # at location 0", and the application does not start).

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
`NOT MIGRATED: setBody (fails here)`.

The generated smoke test of such a route (`testRoute...Returns200`) fails
until the expression is replaced. That is intended: the route is not migrated
yet.

### In the report

A choice is one row: `DONE` if every condition is converted and none rests on
an assumption (see "When a comparison is `DONE`"), otherwise `MANUAL_REVIEW`
with the unconverted conditions and the reasons, and with the conditions that
are converted on an assumption and the assignments they are about. A
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
