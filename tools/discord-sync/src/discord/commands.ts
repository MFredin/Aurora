import { InteractionContextType, PermissionFlagsBits, SlashCommandBuilder } from "discord.js";
import { CATEGORIES, PRIORITIES, REJECT_REASONS } from "../triage.js";
import { CATEGORY_TAGS, PRIORITY_TAGS } from "../tags.js";

const REJECT_LABELS: Record<(typeof REJECT_REASONS)[number], string> = {
  not_a_bug: "Not a bug",
  cant_reproduce: "Can't reproduce",
  wont_fix: "Won't fix",
  spam: "Spam or off-topic",
};

// Hidden from members by default (Manage Threads). Server admins can grant it per role under
// Server settings > Integrations; the bot also checks the configured triager roles itself.
export const triageCommand = new SlashCommandBuilder()
  .setName("triage")
  .setDescription("Triage the bug report in this thread")
  .setContexts(InteractionContextType.Guild)
  .setDefaultMemberPermissions(PermissionFlagsBits.ManageThreads)
  .addSubcommand((s) => s.setName("ack").setDescription("Acknowledge this report and take ownership of triage"))
  .addSubcommand((s) =>
    s
      .setName("set")
      .setDescription("Set category and/or priority")
      .addStringOption((o) =>
        o
          .setName("category")
          .setDescription("What kind of report this is")
          .addChoices(...CATEGORIES.map((c) => ({ name: CATEGORY_TAGS[c], value: c }))),
      )
      .addStringOption((o) =>
        o
          .setName("priority")
          .setDescription("How urgent it is")
          .addChoices(...PRIORITIES.map((p) => ({ name: PRIORITY_TAGS[p], value: p }))),
      ),
  )
  .addSubcommand((s) =>
    s
      .setName("need-info")
      .setDescription("Ask the reporter for more detail; their reply puts it back in the queue")
      .addStringOption((o) => o.setName("note").setDescription("What you need from them").setRequired(true).setMaxLength(500)),
  )
  .addSubcommand((s) => s.setName("sync").setDescription("Create the GitHub issue (needs acknowledged, category and priority)"))
  .addSubcommand((s) =>
    s
      .setName("duplicate")
      .setDescription("Close as a duplicate of another report or issue")
      .addChannelOption((o) => o.setName("thread").setDescription("The original Discord thread"))
      .addIntegerOption((o) => o.setName("issue").setDescription("The original GitHub issue number").setMinValue(1)),
  )
  .addSubcommand((s) =>
    s
      .setName("reject")
      .setDescription("Close without creating an issue")
      .addStringOption((o) =>
        o
          .setName("reason")
          .setDescription("Why")
          .setRequired(true)
          .addChoices(...REJECT_REASONS.map((r) => ({ name: REJECT_LABELS[r], value: r }))),
      )
      .addStringOption((o) => o.setName("note").setDescription("Optional note for the reporter").setMaxLength(500)),
  )
  .addSubcommand((s) => s.setName("reopen").setDescription("Reopen a rejected or duplicate report"))
  .addSubcommand((s) => s.setName("queue").setDescription("List reports waiting for triage (only you see it)"))
  .addSubcommand((s) =>
    s
      .setName("link")
      .setDescription("Maintainers: link this thread to an existing issue")
      .addIntegerOption((o) => o.setName("issue").setDescription("Issue number").setRequired(true).setMinValue(1)),
  )
  .addSubcommand((s) => s.setName("unlink").setDescription("Maintainers: unlink this thread from its issue"));

/** Anyone can check where their report stands. */
export const statusCommand = new SlashCommandBuilder()
  .setName("status")
  .setDescription("Show the triage status of this report")
  .setContexts(InteractionContextType.Guild);

export const allCommands = [triageCommand, statusCommand];
