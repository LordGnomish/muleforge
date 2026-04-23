# MuleForge Roadmap

## MVP-0 (Current)

- [x] Mule 4 XML parser (flows, sub-flows, configs, error handlers)
- [x] Property placeholder resolution and config-ref linking
- [x] Mule AST -> Camel IR mapper (15+ connector types)
- [x] Java RouteBuilder code generation
- [x] pom.xml generation with Quarkus + Camel BOM
- [x] Dockerfile (JVM + native variants)
- [x] Kubernetes manifests (Deployment, Service, ConfigMap, HPA)
- [x] Kustomize overlays (dev / prod)
- [x] Kong Gateway declarative config (decK)
- [x] GitHub Actions CI/CD workflows
- [x] application.properties with component auto-detection
- [x] Smoke test generation (JUnit 5 / REST Assured)
- [x] Migration report (Markdown)
- [x] 19 mapping rule YAML files (40+ unique element mappings)
- [x] LLM provider integration (Claude, OpenAI, Ollama)
- [x] Pattern-based DataWeave converter (8 common patterns, no LLM needed)
- [x] LLM-assisted DataWeave → Java bean conversion
- [x] Structured TODO stub fallback (no LLM mode)
- [x] TypeScript CLI shell
- [x] Apache 2.0 license, CONTRIBUTING, SECURITY

## MVP-1 (Next)

- [ ] Remote Git clone (`--from git@...`)
- [ ] Incremental commit strategy (`--incremental-commits`)
- [ ] LLM provider implementations (Claude, OpenAI, Ollama)
- [ ] DataWeave -> Java bean conversion (LLM-assisted)
- [ ] Documentation generation (architecture, per-flow, runbook)
- [ ] Golden file test suite (input XML -> expected Java output)
- [ ] `muleforge analyze` dry-run command
- [ ] Secret scrubbing (`--scrub-secrets`)
- [ ] YAML properties file support
- [ ] Sub-flow inlining toggle

## MVP-1.5 (Server Mode)

- [ ] HTTP server mode (`muleforge serve --port 8080`)
- [ ] `POST /api/migrate` — submit migration job (Mule repo URL → Camel repo)
- [ ] `POST /api/analyze` — dry-run analysis
- [ ] `GET /api/jobs/{id}` — job status and progress
- [ ] `GET /api/health` — health check
- [ ] Job queue for async migration (large projects)
- [ ] Dockerfile + Helm chart for Kubernetes deployment
- [ ] Web UI for migration status and report viewing
- [ ] Multi-tenant support (team-shared instance)

## MVP-2

- [ ] Additional connectors: AMQP, Salesforce, SAP, SOAP, email
- [ ] Spring Boot target (alternative to Quarkus)
- [ ] Custom emitter plugin API
- [ ] DataWeave unit test migration
- [ ] Performance benchmarks (large Mule projects)
- [ ] VS Code extension for interactive migration
- [ ] Gravitee / Envoy gateway config (in addition to Kong)

## Long-term

- [ ] Mule 3 support
- [ ] Bidirectional sync (watch Mule repo, update Camel repo)
- [ ] Migration complexity scoring
- [ ] Enterprise dashboard for batch migrations
