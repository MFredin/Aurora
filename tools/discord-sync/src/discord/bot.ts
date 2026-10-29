import {
  ChannelType,
  Client,
  EmbedBuilder,
  Events,
  GatewayIntentBits,
  MessageFlags,
  type AnyThreadChannel,
  type ChatInputCommandInteraction,
  type Message,
} from "discord.js";
import type { DatabaseSync } from "node:sqlite";
import type { Config } from "../config.js";
import { commentBody, issueBody, isLowSignal, splitMessage, type DigestMessage } from "../format.js";
import type { GitHub } from "../github/client.js";
import * as store from "../db.js";
import { roleOf } from "../roles.js";
import { CATEGORY_TAGS, labelsFor, nextAppliedTags, PRIORITY_TAGS, STATUS_TAGS } from "../tags.js";
import {
  transition,
  type Action,
  type Actor,
  type Category,
  type Effect,
  type Priority,
  type RejectReason,
  type Report,
} from "../triage.js";

export interface Deps {
  cfg: Config;
  db: DatabaseSync;
  gh: GitHub;
  log: (level: "info" | "warn" | "error", msg: string, extra?: unknown) => void;
}

export function createBot(deps: Deps): Client {
  const client = new Client({
    intents: [GatewayIntentBits.Guilds, GatewayIntentBits.GuildMessages, GatewayIntentBits.MessageContent],
  });

  client.on(Events.ThreadCreate, (thread, newlyCreated) => {
    if (!newlyCreated || thread.parentId !== deps.cfg.DISCORD_WATCHED_CHANNEL_ID) return;
    void onThreadCreate(deps, thread).catch((e) => deps.log("error", "threadCreate failed", e));
  });

  client.on(Events.MessageCreate, (msg) => {
    if (msg.author.bot || msg.webhookId || !msg.channel.isThread()) return;
    if (msg.channel.parentId !== deps.cfg.DISCORD_WATCHED_CHANNEL_ID) return;
    if (msg.id === msg.channel.id) return; // forum starter message, already in the issue body
    void onMessage(deps, msg).catch((e) => deps.log("error", "messageCreate failed", e));
  });

  client.on(Events.InteractionCreate, (i) => {
    if (!i.isChatInputCommand()) return;
    void onCommand(deps, i).catch(async (e) => {
      deps.log("error", "command failed", e);
      const content = "Something went wrong and nothing was changed. Try again, or check the log channel.";
      if (i.replied || i.deferred) await i.followUp({ content, flags: MessageFlags.Ephemeral }).catch(() => {});
      else await i.reply({ content, flags: MessageFlags.Ephemeral }).catch(() => {});
    });
  });

  // TODO(reconcile): on ClientReady, compare open reports with GitHub issue state and backfill.
  return client;
}

// Flow 1, gated: a new post becomes a report in the queue. No issue yet.
async function onThreadCreate(deps: Deps, thread: AnyThreadChannel): Promise<void> {
  const inserted = store.insertReport(deps.db, {
    threadId: thread.id,
    channelId: thread.parentId ?? "",
    reporterId: thread.ownerId ?? "",
    title: thread.name,
    repo: deps.cfg.GITHUB_REPO,
  });
  if (!inserted) return;
  const report = store.getReport(deps.db, thread.id);
  if (report) await refreshCard(deps, thread, report);
}

// Flow 2: after sync, replies become comments. Before sync they wait for the digest.
async function onMessage(deps: Deps, msg: Message<boolean>): Promise<void> {
  const report = store.getReport(deps.db, msg.channelId);
  if (!report || !msg.channel.isThread()) return;

  if (report.status === "needs_info" && msg.author.id === report.reporterId) {
    await apply(deps, msg.channel, report, { type: "reporter_replied" }, systemActor);
    return;
  }
  if (report.status !== "synced" || report.issueNumber === null) return;
  if (isLowSignal(msg.content) && msg.attachments.size === 0) return; // "+1", "same", emoji only

  const commentId = await deps.gh.comment(report.issueNumber, commentBody(toDigest(msg)));
  deps.db
    .prepare(
      "INSERT INTO messages (thread_id, discord_message_id, github_comment_id, direction, content_hash) VALUES (?, ?, ?, 'discord_to_github', ?)",
    )
    .run(report.threadId, msg.id, commentId, String(msg.content.length));
}

const systemActor: Actor = { id: "system", name: "Forge bot", role: "maintainer" };

async function onCommand(deps: Deps, i: ChatInputCommandInteraction): Promise<void> {
  const ephemeral = (content: string) => i.reply({ content, flags: MessageFlags.Ephemeral });
  if (!i.inCachedGuild()) return void ephemeral("Use this in the server.");

  const role = roleOf(i.member.roles.cache.keys(), deps.cfg);
  const sub = i.commandName === "triage" ? i.options.getSubcommand() : "status";

  if (sub === "queue") {
    const rows = store.listQueue(deps.db);
    const lines = rows.map((r) => `- ${STATUS_TAGS[r.status]}: <#${r.thread_id}> (${r.created_at.slice(0, 10)})`);
    return void ephemeral(lines.length ? lines.join("\n") : "Nothing waiting for triage.");
  }

  const thread = i.channel;
  if (!thread?.isThread() || thread.parentId !== deps.cfg.DISCORD_WATCHED_CHANNEL_ID) {
    return void ephemeral("Run this inside a bug report thread.");
  }
  const report = store.getReport(deps.db, thread.id);
  if (!report) return void ephemeral("This thread isn't tracked. A maintainer can add it with /triage link.");

  if (sub === "status") {
    const showLink =
      report.issueNumber !== null &&
      (deps.cfg.ISSUE_LINK_VISIBILITY === "public" ||
        (deps.cfg.ISSUE_LINK_VISIBILITY === "triagers" && role !== "member"));
    const link = showLink ? `\nIssue: ${deps.gh.issueUrl(report.issueNumber!)}` : "";
    return void ephemeral(`${statusLine(report)}${link}`);
  }

  const action = parseAction(sub, i);
  if (!action) return void ephemeral("Unknown command.");
  await i.deferReply({ flags: MessageFlags.Ephemeral });
  const error = await apply(deps, thread, report, action, { id: i.user.id, name: i.member.displayName, role });
  await i.editReply(error ?? "Done.");
}

function parseAction(sub: string, i: ChatInputCommandInteraction): Action | null {
  const o = i.options;
  switch (sub) {
    case "ack":
      return { type: "ack" };
    case "set": {
      const category = (o.getString("category") ?? undefined) as Category | undefined;
      const priority = (o.getString("priority") ?? undefined) as Priority | undefined;
      return { type: "set", category, priority };
    }
    case "need-info":
      return { type: "need_info", note: o.getString("note", true) };
    case "sync":
      return { type: "sync" };
    case "duplicate":
      return { type: "duplicate", issueNumber: o.getInteger("issue"), threadId: o.getChannel("thread")?.id ?? null };
    case "reject":
      return { type: "reject", reason: o.getString("reason", true) as RejectReason, note: o.getString("note") ?? undefined };
    case "reopen":
      return { type: "reopen" };
    case "link":
      return { type: "link", issueNumber: o.getInteger("issue", true) };
    case "unlink":
      return { type: "unlink" };
    default:
      return null;
  }
}

/** Run a transition, persist it, then execute its effects. Returns an error message or null. */
export async function apply(
  deps: Deps,
  thread: AnyThreadChannel,
  report: Report,
  action: Action,
  actor: Actor,
): Promise<string | null> {
  const out = transition(report, action, actor);
  if (!out.ok) return out.error;

  // Persist before side effects: a crash mid-create leaves status=synced with no issue number,
  // which reconcile repairs by searching for the thread marker instead of creating a second issue.
  let next = out.report;
  store.saveReport(deps.db, next);
  for (const effect of out.effects) {
    next = await execute(deps, thread, next, effect, actor);
  }
  return null;
}

async function execute(deps: Deps, thread: AnyThreadChannel, r: Report, e: Effect, actor: Actor): Promise<Report> {
  switch (e.type) {
    case "create_issue": {
      const number = await deps.gh.createIssue(thread.name, await buildIssueBody(thread, actor), labelsFor(r));
      const next = { ...r, issueNumber: number };
      store.saveReport(deps.db, next);
      return next;
    }
    case "sync_labels":
      await deps.gh.setLabels(e.issueNumber, (n) => !n.startsWith("type: ") && !n.startsWith("priority: "), labelsFor(r));
      return r;
    case "comment_on_issue":
      await deps.gh.comment(e.issueNumber, e.body);
      return r;
    case "post_in_thread":
      if (thread.archived) await thread.setArchived(false);
      for (const part of splitMessage(e.text)) {
        await thread.send({ content: part, allowedMentions: { users: [r.reporterId] } });
      }
      return r;
    case "close_thread":
      await thread.edit({ locked: true, archived: true });
      return r;
    case "reopen_thread":
      await thread.edit({ archived: false, locked: false });
      return r;
    case "refresh_card":
      await refreshCard(deps, thread, r);
      return r;
    case "audit":
      deps.db.prepare("INSERT INTO audit (thread_id, actor_id, text) VALUES (?, ?, ?)").run(r.threadId, actor.id, e.text);
      await postToLog(deps, thread, `<#${r.threadId}>: ${e.text}`);
      return r;
  }
}

/** One status card per thread, edited in place, plus forum tags. No status chatter. */
async function refreshCard(deps: Deps, thread: AnyThreadChannel, r: Report): Promise<void> {
  const parent = thread.parent;
  if (parent?.type === ChannelType.GuildForum) {
    const ids = new Map(parent.availableTags.map((t) => [t.name, t.id]));
    const wanted = nextAppliedTags(thread.appliedTags, r, ids);
    if (wanted.join() !== thread.appliedTags.join()) {
      if (thread.archived) await thread.setArchived(false);
      await thread.setAppliedTags(wanted);
    }
  }

  const embed = new EmbedBuilder().setDescription(statusLine(r)).setColor(0x5865f2);
  if (r.issueNumber !== null && deps.cfg.ISSUE_LINK_VISIBILITY === "public") {
    embed.setURL(deps.gh.issueUrl(r.issueNumber)).setTitle(`Tracked as #${r.issueNumber}`);
  }

  const row = deps.db.prepare("SELECT card_message_id FROM reports WHERE thread_id = ?").get(r.threadId) as
    | { card_message_id: string | null }
    | undefined;
  if (row?.card_message_id) {
    const existing = await thread.messages.fetch(row.card_message_id).catch(() => null);
    if (existing) {
      await existing.edit({ embeds: [embed] });
      return;
    }
  }
  if (thread.archived) return; // don't reopen a closed thread just to post a card
  const sent = await thread.send({ embeds: [embed], allowedMentions: { parse: [] } });
  deps.db.prepare("UPDATE reports SET card_message_id = ? WHERE thread_id = ?").run(sent.id, r.threadId);
}

function statusLine(r: Report): string {
  const parts = [`Status: ${STATUS_TAGS[r.status]}`];
  if (r.category) parts.push(`Category: ${CATEGORY_TAGS[r.category]}`);
  if (r.priority) parts.push(`Priority: ${PRIORITY_TAGS[r.priority]}`);
  return parts.join(" · ");
}

async function buildIssueBody(thread: AnyThreadChannel, actor: Actor): Promise<string> {
  const starter = await thread.fetchStarterMessage().catch(() => null);
  // TODO: page past 100 for long threads.
  const fetched = await thread.messages.fetch({ limit: 100 });
  const replies = [...fetched.values()]
    .filter((m) => !m.author.bot && m.id !== thread.id && !isLowSignal(m.content))
    .sort((a, b) => a.createdTimestamp - b.createdTimestamp)
    .map(toDigest);
  return issueBody({
    threadId: thread.id,
    threadUrl: thread.url,
    starter: starter ? toDigest(starter) : { author: "unknown", content: "(starter message deleted)", attachments: [] },
    replies,
    triagedBy: actor.name,
  });
}

function toDigest(m: Message): DigestMessage {
  return {
    author: m.member?.displayName ?? m.author.username,
    content: m.content,
    // TODO(decision): Discord CDN links expire; re-host if we want them to last.
    attachments: [...m.attachments.values()].map((a) => a.url),
  };
}

async function postToLog(deps: Deps, thread: AnyThreadChannel, text: string): Promise<void> {
  if (!deps.cfg.DISCORD_LOG_CHANNEL_ID) return;
  const ch = await thread.client.channels.fetch(deps.cfg.DISCORD_LOG_CHANNEL_ID).catch(() => null);
  if (ch?.isSendable()) await ch.send({ content: text, allowedMentions: { parse: [] } });
}
