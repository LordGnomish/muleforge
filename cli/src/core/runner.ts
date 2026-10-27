/**
 * Spawns the Rust core binary and exchanges JSON-RPC messages over stdio.
 *
 * For MVP-0, this is a stub. The protocol will look like:
 *
 *   → { method: "migrate", params: { ... } }
 *   ← { event: "progress", stage: "parse", done: 3, total: 12 }
 *   ← { event: "decision", mule_element: "...", status: "DONE", rule_id: "..." }
 *   ← { event: "llm_request", task: "DataWeaveToCamel", prompt: "..." }
 *   → { event: "llm_response", output: "...", confidence: 0.92 }
 *   ← { event: "done", summary: { done: 42, manualReview: 3, skipped: 0 } }
 */

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

export async function runCore(_input: RunInput): Promise<RunResult> {
  // TODO(MVP-0):
  //   - Locate the muleforge-core binary (bundled or via $MULEFORGE_CORE env).
  //   - spawn() it with the JSON-RPC request on stdin.
  //   - Stream stdout line-by-line, parse JSON events.
  //   - Emit progress to the configured spinner.
  //   - If an llm_request event arrives, hand it to the TS-side LLM dispatcher
  //     (so we can use the mature Anthropic/OpenAI SDKs) and send the response
  //     back over stdin.
  //   - Return the summary from the "done" event.
  return { done: 0, manualReview: 0, skipped: 0 };
}
