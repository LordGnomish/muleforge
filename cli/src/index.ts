#!/usr/bin/env node
/**
 * MuleForge CLI entry point.
 *
 * Parses command-line options, loads the configuration, spawns the Rust core
 * binary, and manages user-facing UX (progress, prompts, diff previews).
 */

import { Command } from "commander";
import chalk from "chalk";

import { migrateCommand } from "./commands/migrate.js";
import { analyzeCommand } from "./commands/analyze.js";
import { version } from "./version.js";

const program = new Command();

program
  .name("muleforge")
  .description(
    "Transform MuleSoft repositories into Apache Camel Quarkus repositories.",
  )
  .version(version);

program
  .command("migrate")
  .description("Migrate a Mule repository to a Camel Quarkus repository.")
  .argument("[from]", "Local path to the Mule repo, or omit if --from is used")
  .argument("[to]", "Local path for the new Camel Quarkus repo")
  .option("--from <source>", "Input: local path or remote Git URL")
  .option("--to <dir>", "Output directory")
  .option("--init-git", "Initialize a Git repo in the output directory", true)
  .option("--no-git", "Do not initialize Git (output is a plain directory)")
  .option(
    "--incremental-commits",
    "Write the output in logical commits instead of a single commit",
    false,
  )
  .option("--push-to <remote>", "Add this as origin and push after committing")
  .option("--force", "Overwrite output directory if non-empty", false)
  .option(
    "--emit-kong-config",
    "Also emit Kong Konnect declarative config",
    false,
  )
  .option("--no-k8s", "Do not emit Kubernetes manifests")
  .option(
    "--llm-provider <provider>",
    "LLM provider: claude | openai | gemini | azure | ollama",
    "claude",
  )
  .option("--llm-model <model>", "LLM model name")
  .option("--no-llm", "Disable LLM entirely (rule-based only)")
  .option(
    "--config <path>",
    "Path to muleforge.config.yaml",
    "./muleforge.config.yaml",
  )
  .option("--interactive", "Prompt to approve LLM outputs before writing", false)
  .option("--dry-run", "Print what would happen without writing files", false)
  .option("--verbose", "Verbose logging", false)
  .action(migrateCommand);

program
  .command("analyze")
  .description("Analyze a Mule repo and report what a migration would do.")
  .argument("<path>", "Path to the Mule repo")
  .option("--config <path>", "Path to muleforge.config.yaml")
  .option("--output <path>", "Write analysis report to this file (Markdown)")
  .action(analyzeCommand);

program.on("command:*", () => {
  console.error(
    chalk.red(`Unknown command: ${program.args.join(" ")}`),
  );
  program.outputHelp();
  process.exit(1);
});

program.parseAsync(process.argv).catch((err) => {
  console.error(chalk.red("MuleForge error:"), err?.message ?? err);
  if (process.env.MULEFORGE_DEBUG) {
    console.error(err?.stack);
  }
  process.exit(1);
});
