/**
 * `muleforge analyze` command. Read-only: reports what a migration would do.
 */

import chalk from "chalk";
import { loadConfig } from "../config.js";
import { runCore } from "../core/runner.js";

interface AnalyzeOptions {
  config?: string;
  output?: string;
}

export async function analyzeCommand(
  path: string,
  opts: AnalyzeOptions,
): Promise<void> {
  const config = await loadConfig(opts.config ?? "./muleforge.config.yaml");
  const result = await runCore({
    from: path,
    to: undefined,
    config,
    flags: { dryRun: true },
  });
  console.log(
    chalk.bold("Analysis:"),
    `${result.flowCount ?? 0} flow(s), ${result.totalElements ?? 0} element(s) inspected.`,
  );
  // TODO: pretty-print the per-element decisions; write to opts.output if set.
}
