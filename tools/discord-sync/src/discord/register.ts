// Registers slash commands for the configured guild (instant, unlike global commands).
// Run once after changing commands: npm run register-commands

import { REST, Routes } from "discord.js";
import { loadConfig } from "../config.js";
import { allCommands } from "./commands.js";

const cfg = loadConfig();
const rest = new REST().setToken(cfg.DISCORD_TOKEN);
await rest.put(Routes.applicationGuildCommands(cfg.DISCORD_APP_ID, cfg.DISCORD_GUILD_ID), {
  body: allCommands.map((c) => c.toJSON()),
});
console.log(`Registered ${allCommands.length} commands in guild ${cfg.DISCORD_GUILD_ID}`);
