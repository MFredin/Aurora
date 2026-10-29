import { z } from "zod";

const ids = z
  .string()
  .default("")
  .transform((s) =>
    s
      .split(",")
      .map((x) => x.trim())
      .filter(Boolean),
  );

const schema = z.object({
  DISCORD_TOKEN: z.string().min(1),
  DISCORD_APP_ID: z.string().min(1),
  DISCORD_GUILD_ID: z.string().min(1),
  DISCORD_WATCHED_CHANNEL_ID: z.string().min(1),
  DISCORD_LOG_CHANNEL_ID: z.string().optional(),
  DISCORD_TRIAGER_ROLE_IDS: ids,
  DISCORD_MAINTAINER_ROLE_IDS: ids,

  GITHUB_APP_ID: z.string().min(1),
  GITHUB_APP_PRIVATE_KEY: z
    .string()
    .min(1)
    .transform((k) => k.replace(/\\n/g, "\n")),
  GITHUB_INSTALLATION_ID: z.coerce.number().int().positive(),
  GITHUB_REPO: z.string().regex(/^[\w.-]+\/[\w.-]+$/, "expected owner/name"),
  GITHUB_WEBHOOK_SECRET: z.string().min(16),

  ISSUE_LINK_VISIBILITY: z.enum(["public", "triagers", "none"]).default("triagers"),
  INTERNAL_COMMENT_MARKER: z.string().min(1).default("[internal]"),
  NEEDS_INFO_TIMEOUT_DAYS: z.coerce.number().int().positive().default(14),

  DATABASE_PATH: z.string().default("./data/sync.db"),
  PORT: z.coerce.number().int().default(3000),
  LOG_LEVEL: z.enum(["debug", "info", "warn", "error"]).default("info"),
});

export type Config = z.infer<typeof schema>;

/** Fails fast with every missing variable named, but never echoes values. */
export function loadConfig(env: NodeJS.ProcessEnv = process.env): Config {
  const parsed = schema.safeParse(env);
  if (!parsed.success) {
    const problems = parsed.error.issues.map((i) => `${i.path.join(".")}: ${i.message}`);
    throw new Error(`Invalid configuration:\n  ${problems.join("\n  ")}`);
  }
  return parsed.data;
}
