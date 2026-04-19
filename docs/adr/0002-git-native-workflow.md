# ADR-0002: Git-native input and output

**Status:** Accepted
**Date:** 2026-04-19

## Context

MuleForge operates on source code that lives in Git. Users expect to point a tool at a repository and receive a repository back — not a pile of files in a directory.

Alternatives considered before settling:

- **Directory in, directory out.** Simple, but forces the user to `git init && git add . && git commit` themselves every time. Hides useful provenance (source commit SHA).
- **Git in, directory out.** Inconsistent; the user still has to initialize Git on the output side.
- **Git in, Git out.** Matches how developers actually work.

## Decision

MuleForge treats Git as a first-class input and output:

- **Input** can be a local path or a remote Git URL (`https://`, `git@...`). Remote URLs are shallow-cloned into a temp directory. If the input is a Git repo, the HEAD commit SHA is recorded in the migration report.
- **Output** is initialized as a fresh Git repository by default. The user can request `--no-git` to get a plain directory, but that is opt-out, not opt-in.
- **Commit strategy** is selectable. Single-commit is the default ("Initial migration via MuleForge"). `--incremental-commits` produces a series of logical commits mirroring the pipeline stages, which gives users a `git log` of the migration itself and makes code review tractable on large projects.
- **Push** is optional. `--push-to <remote>` adds origin and pushes the default branch.

Git operations use `git2` (libgit2 bindings) so we do not require the `git` binary to be installed on the user's machine.

## Consequences

**Positive:**
- Zero-friction path from "I have a Mule repo" to "I have a Camel Quarkus repo I can review on GitHub."
- Provenance is captured: the migration report links the output to a specific input commit.
- Incremental commits make large migrations reviewable.
- Works offline (no GitHub/GitLab assumed — any remote, or no remote at all).

**Negative:**
- Git operations add complexity and failure modes (auth, conflicts, push rejection).
- The `git2` dependency pulls in libgit2, increasing binary size.
- We must be careful never to accept credentials on the command line; SSH agent or `GIT_ASKPASS` only.

## Out of scope

- MuleForge does not host repositories.
- MuleForge does not open pull requests. An integration for that belongs in a separate GitHub Action or CI wrapper.
- MuleForge does not mutate the input repository. Ever.
