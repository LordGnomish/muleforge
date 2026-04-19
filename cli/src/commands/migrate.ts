/**
 * `muleforge migrate` command.
 *
 * Resolves input and output, loads config, invokes the Rust core binary via
 * a JSON-RPC-over-stdio protocol, streams progress to the terminal, and
 * summarizes the result.
 */

import chalk from "chalk";
import ora from "ora";

import { loadConfig } from "../config.js";
import { runCore } from "../core/runner.js";

interface MigrateOptions {
  from?: string;
  to?: string;
  initGit: boolean;
  git: boolean;
  incrementalCommits: boolean;
  pushTo?: string;
  force: boolean;
  emitKongConfig: boolean;
  k8s: boolean;
  llmProvider: string;
  llmModel?: string;
  llm: boolean;
  config: string;
  interactive: boolean;
  dryRun: boolean;
  verbose: boolean;
}

export async function migrateCommand(
  fromArg: string | undefined,
  toArg: string | undefined,
  opts: MigrateOptions,
): Promise<void> {
  const from = opts.from ?? fromArg;
  const to = opts.to ?? toArg;

  if (!from || !to) {
    throw new Error(
      "usage: muleforge migrate <from> <to>  (or use --from and --to)",
    );
  }

  const config = await loadConfig(opts.config);

  // TODO(MVP-0):
  //   - Resolve `from`: local path (as-is) or remote URL (pass-through to core).
  //   - Validate `to`: refuse non-empty unless --force.
  //   - Merge CLI flags into config (CLI wins).
  //   - Call runCore() which spawns the Rust binary.
  //   - Render a summary: N flows migrated, M manual review items, etc.

  const spinner = ora("Running migration…").start();
  try {
    const result = await runCore({
      from,
      to,
      config,
      flags: opts,
    });
    spinner.succeed(
      `Migration complete: ${result.done} done, ${result.manualReview} manual review, ${result.skipped} skipped`,
    );
    console.log(
      chalk.gray(
        `Output repository: ${to}\nSee ${to}/MIGRATION_REPORT.md for details.`,
      ),
    );
  } catch (err) {
    spinner.fail("Migration failed");
    throw err;
  }
}
