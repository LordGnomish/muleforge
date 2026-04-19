/**
 * Spawns the Rust core binary and invokes migration via JSON-RPC over stdio.
 *
 * Protocol:
 *   → { method: "migrate", params: { ... } }
 *   ← { event: "progress", stage: "parse", done: 3, total: 12 }
 *   ← { event: "done", summary: { done: 42, manualReview: 3, skipped: 0 } }
 */

import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import type { MuleForgeConfig } from "../config.js";

export interface RunInput {
  from: string;
  to: string | undefined;
  config: MuleForgeConfig;
  flags: Record<string, unknown>;
}

export interface RunResult {
  done: number;
  manualReview: number;
  skipped: number;
  flowCount?: number;
  totalElements?: number;
}

/**
 * Find the muleforge-core binary. Search order:
 * 1. $MULEFORGE_CORE environment variable
 * 2. Bundled binary next to CLI (../core/target/release/muleforge-core)
 * 3. cargo build output (../core/target/debug/muleforge-core)
 * 4. System PATH
 */
function findCoreBinary(): string {
  const envPath = process.env.MULEFORGE_CORE;
  if (envPath && existsSync(envPath)) return envPath;

  const __dirname = dirname(fileURLToPath(import.meta.url));
  const candidates = [
    join(__dirname, "..", "..", "..", "core", "target", "release", "muleforge-core"),
    join(__dirname, "..", "..", "..", "core", "target", "debug", "muleforge-core"),
  ];

  for (const candidate of candidates) {
    if (existsSync(candidate)) return candidate;
  }

  // Fall back to PATH
  return "muleforge-core";
}

export async function runCore(input: RunInput): Promise<RunResult> {
  // For MVP-0, we call the Rust library directly via a simple stdin/stdout protocol.
  // The Rust binary reads a JSON config from stdin, runs the migration, and writes
  // the result as JSON to stdout.
  //
  // Since the Rust binary is a thin wrapper, we use cargo run as fallback.

  const coreBin = findCoreBinary();

  const request = {
    method: input.flags.dryRun ? "analyze" : "migrate",
    params: {
      input_path: input.from,
      output_path: input.to,
      mappings_dir: join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "mappings"),
      force: input.flags.force ?? false,
      emit_k8s: input.flags.k8s ?? true,
      emit_kong_config: input.flags.emitKongConfig ?? false,
      no_llm: !(input.flags.llm ?? true),
      incremental_commits: input.flags.incrementalCommits ?? false,
      llm: input.config.llm ?? null,
      docgen: input.config.docgen ?? { enabled: true },
    },
  };

  return new Promise<RunResult>((resolve, reject) => {
    let proc: ChildProcess;

    try {
      proc = spawn(coreBin, [], {
        stdio: ["pipe", "pipe", "pipe"],
        env: { ...process.env },
      });
    } catch (err) {
      // If binary not found, provide helpful message
      reject(
        new Error(
          `Could not find muleforge-core binary.\n` +
            `Build it first: cd core && cargo build --release\n` +
            `Or set MULEFORGE_CORE=/path/to/binary`
        )
      );
      return;
    }

    let stdout = "";
    let stderr = "";

    proc.stdout?.on("data", (data: Buffer) => {
      const text = data.toString();
      stdout += text;

      // Parse line-by-line for streaming events
      for (const line of text.split("\n")) {
        if (!line.trim()) continue;
        try {
          const event = JSON.parse(line);
          if (event.event === "progress") {
            process.stderr.write(
              `\r  ${event.stage}: ${event.done}/${event.total}`
            );
          }
        } catch {
          // Not JSON — might be log output
        }
      }
    });

    proc.stderr?.on("data", (data: Buffer) => {
      stderr += data.toString();
    });

    proc.on("close", (code: number | null) => {
      if (code !== 0) {
        reject(
          new Error(
            `muleforge-core exited with code ${code}\n${stderr}`
          )
        );
        return;
      }

      // Parse the last JSON line from stdout as the result
      const lines = stdout.trim().split("\n").filter(Boolean);
      for (let i = lines.length - 1; i >= 0; i--) {
        try {
          const result = JSON.parse(lines[i]);
          if (result.summary || result.done !== undefined) {
            resolve({
              done: result.summary?.done ?? result.done ?? 0,
              manualReview:
                result.summary?.manual_review ?? result.manualReview ?? 0,
              skipped: result.summary?.skipped ?? result.skipped ?? 0,
              flowCount: result.flow_count ?? result.flowCount,
              totalElements:
                result.total_elements ?? result.totalElements,
            });
            return;
          }
        } catch {
          continue;
        }
      }

      // If no JSON result found, return empty result
      resolve({ done: 0, manualReview: 0, skipped: 0 });
    });

    proc.on("error", (err: Error) => {
      reject(
        new Error(
          `Failed to spawn muleforge-core: ${err.message}\n` +
            `Build it first: cd core && cargo build --release`
        )
      );
    });

    // Send the request
    proc.stdin?.write(JSON.stringify(request) + "\n");
    proc.stdin?.end();
  });
}
