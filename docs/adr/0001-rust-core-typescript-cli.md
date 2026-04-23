# ADR-0001: Rust core + TypeScript CLI

**Status:** Accepted
**Date:** 2026-04-19

## Context

MuleForge must do two very different things well:

1. **Deterministic code transformation.** Parse Mule XML, walk ASTs, apply mapping rules, emit Java/YAML source. This is a compiler-style task: performance, correctness, and type safety matter; non-determinism is a bug.
2. **Ergonomic developer experience.** Good CLI UX, interactive prompts, progress spinners, and a mature LLM SDK ecosystem for orchestrating calls to Claude, OpenAI, and Ollama.

Doing both well in a single language is hard. Java has LLM SDKs but is not a great fit for a portable CLI. Python has LLM SDKs but is weak at XML/AST manipulation at scale. Rust is excellent for compilers but its LLM SDK ecosystem is immature. TypeScript is excellent for CLIs and LLMs but weaker for AST work.

## Decision

Split the project into two components:

- **Core:** Rust. Owns parsing, normalization, mapping, emitters, and Git operations via `git2`. Produces a standalone binary.
- **CLI:** TypeScript (Node ≥ 20). Owns user-facing UX, configuration loading, and LLM orchestration. Spawns the core binary as a child process and communicates via JSON-RPC over stdio.

## Consequences

**Positive:**
- Each component is in its best-fit language.
- Contributors can add mapping rules (YAML) or docgen generators (Rust) without touching the CLI.
- The core binary is distributable on its own for integration into other pipelines (CI agents, IDE plugins).
- LLM provider updates happen in TypeScript where the SDKs are best maintained.

**Negative:**
- Two build toolchains (cargo + npm).
- A narrow but stable stdio protocol must be versioned and tested.
- Binary distribution requires a release pipeline that produces both the Rust binary (per-OS) and the npm package.

## Alternatives considered

- **Pure Rust.** Rejected for MVP: LLM SDKs and CLI UX libraries are less mature, and we would be reinventing too much.
- **Pure TypeScript.** Rejected: XML parsing at scale and deterministic compiler-style work are not TypeScript's strengths, and Node startup time hurts CLI responsiveness.
- **Pure Python.** Rejected: distribution story for a binary CLI is worse than npm + a static Rust binary, and typing discipline for AST work is weaker.
- **Java (JVM) for both.** Rejected: the output of MuleForge is a Java project; that doesn't obligate MuleForge itself to be Java, and JVM startup time + packaging hurt CLI UX.
