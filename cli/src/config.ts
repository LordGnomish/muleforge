/**
 * Loads and validates muleforge.config.yaml.
 */

import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import YAML from "yaml";
import { z } from "zod";

const LlmSchema = z.object({
  provider: z.enum(["claude", "openai", "gemini", "azure", "ollama"]),
  model: z.string(),
  api_key_env: z.string().optional(),
  host: z.string().optional(),
  temperature: z.number().min(0).max(2).default(0),
  fallback: z
    .object({
      provider: z.enum(["claude", "openai", "gemini", "azure", "ollama"]),
      model: z.string(),
      host: z.string().optional(),
    })
    .optional(),
});

const DocgenSchema = z.object({
  enabled: z.boolean().default(true),
  generate: z.array(z.string()).optional(),
  style: z.enum(["technical", "accessible"]).default("technical"),
});

const ConfigSchema = z.object({
  llm: LlmSchema.optional(),
  docgen: DocgenSchema.optional(),
});

export type MuleForgeConfig = z.infer<typeof ConfigSchema>;

export async function loadConfig(path: string): Promise<MuleForgeConfig> {
  if (!existsSync(path)) {
    return ConfigSchema.parse({});
  }
  const raw = await readFile(path, "utf8");
  const parsed = YAML.parse(raw);
  return ConfigSchema.parse(parsed ?? {});
}
