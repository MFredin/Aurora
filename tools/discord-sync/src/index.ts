import { loadConfig } from "./config.js";
import { openDb } from "./db.js";
import { createBot, type Deps } from "./discord/bot.js";
import { createGitHub } from "./github/client.js";
import { createServer } from "./server.js";

const LEVELS = ["debug", "info", "warn", "error"] as const;

async function main(): Promise<void> {
  const cfg = loadConfig();
  const min = LEVELS.indexOf(cfg.LOG_LEVEL);
  const log: Deps["log"] = (level, msg, extra) => {
    if (LEVELS.indexOf(level) < min) return;
    // Never log config or payloads: they can carry tokens.
    const detail = extra instanceof Error ? ` ${extra.message}` : "";
    console[level === "info" ? "log" : level](`[${level}] ${msg}${detail}`);
  };

  const db = openDb(cfg.DATABASE_PATH);
  const gh = await createGitHub(cfg);
  const deps: Deps = { cfg, db, gh, log };

  const discord = createBot(deps);
  const server = createServer(deps, discord);

  await discord.login(cfg.DISCORD_TOKEN);
  await server.listen({ host: "0.0.0.0", port: cfg.PORT });
  log("info", `listening on :${cfg.PORT}`);

  const shutdown = async () => {
    await server.close();
    await discord.destroy();
    db.close();
    process.exit(0);
  };
  process.on("SIGTERM", shutdown);
  process.on("SIGINT", shutdown);
}

main().catch((e) => {
  console.error(e instanceof Error ? e.message : e);
  process.exit(1);
});
