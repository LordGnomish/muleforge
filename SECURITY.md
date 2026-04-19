# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.x     | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in MuleForge, please report it responsibly.

**Do NOT open a public GitHub issue for security vulnerabilities.**

Instead, please email: **security@muleforge.dev**

You will receive a response within 48 hours acknowledging receipt. We will work with you to understand and address the issue before any public disclosure.

### What to include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Scope

MuleForge is a code transformation tool. Security concerns include:

- **Code injection**: Malicious Mule XML that produces unsafe Camel routes
- **Path traversal**: Input paths that escape the expected directory
- **Credential leakage**: Mule properties containing secrets appearing in output
- **LLM prompt injection**: Crafted DataWeave that manipulates the LLM backend
- **Supply chain**: Compromised mapping rules injecting malicious dependencies

### Out of scope

- Vulnerabilities in upstream dependencies (report to the upstream project)
- Issues in the generated output that stem from the original Mule application logic

## Credential Handling

MuleForge processes Mule applications that may contain credentials in property files. By design:

- Credentials detected in input properties are flagged in the migration report
- Output `application.properties` uses environment variable references (`${...}`) instead of literal values
- The `--scrub-secrets` flag (planned) will redact detected credentials from all output files
