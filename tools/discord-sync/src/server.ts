// GitHub webhook endpoint. Verifies the signature on the raw body before parsing anything.

import type { Client } from "discord.js";
import Fastify, { type FastifyInstance } from "fastify";
import { apply, type Deps } from "./discord/bot.js";
import * as store from "./db.js";
import { sanitizeForDiscord, shouldRelayToDiscord, splitMessage } from "./format.js";
import { verifySignature } from "./github/signature.js";
import type { CloseReason } from "./triage.js";

interface IssuePayload {
  action: string;
  issue: { number: number; state_reason?: string | null };
  sender: { login: string; type: string };
  comment?: { id: number; body: string; author_association: string; user: { login: string; type: string } };
}

export function createServer(deps: Deps, discord: Client): FastifyInstance {
  const app = Fastify({ logger: false, bodyLimit: 1024 * 1024 });

  // Keep the raw bytes: the HMAC is over the exact body GitHub sent.
  app.addContentTypeParser("application/json", { parseAs: "buffer" }, (_req, body, done) => done(null, body));

  app.get("/healthz", async () => ({ ok: true }));

  app.post("/github/webhook", async (req, reply) => {
    const raw = req.body as Buffer;
    if (!verifySignature(deps.cfg.GITHUB_WEBHOOK_SECRET, raw, req.headers["x-hub-signature-256"] as string | undefined)) {
      return reply.code(401).send();
    }
    const delivery = req.headers["x-github-delivery"] as string | undefined;
    const event = req.headers["x-github-event"] as string | undefined;
    if (!delivery || !event) return reply.code(400).send();
    if (!store.claimDelivery(deps.db, delivery)) return reply.code(200).send({ duplicate: true });

    // Ack fast (GitHub times out at 10s); handle after replying.
    reply.code(202).send();
    const payload = JSON.parse(raw.toString("utf8")) as IssuePayload;
    void handle(deps, discord, event, payload).catch((e) => deps.log("error", `webhook ${event} failed`, e));
  });

  return app;
}

async function handle(deps: Deps, discord: Client, event: string, p: IssuePayload): Promise<void> {
  const report = store.getReportByIssue(deps.db, deps.cfg.GITHUB_REPO, p.issue.number);
  if (!report) return;
  const thread = await discord.channels.fetch(report.threadId).catch(() => null);
  if (!thread?.isThread()) {
    deps.log("warn", `thread ${report.threadId} for #${p.issue.number} is gone`);
    return; // TODO: mark orphaned
  }
  const system = { id: `github:${p.sender.login}`, name: p.sender.login, role: "maintainer" as const };

  if (event === "issues" && p.action === "closed") {
    const reason = toCloseReason(p.issue.state_reason);
    const target = reason === "duplicate" ? await deps.gh.duplicateTarget(p.issue.number).catch(() => null) : null;
    // Only name the original if it's a Discord thread too; private issue numbers mean nothing there.
    const duplicateThreadId =
      target !== null ? (store.getReportByIssue(deps.db, deps.cfg.GITHUB_REPO, target)?.threadId ?? null) : null;
    // TODO: fetch the closing comment (latest comment by the closer within a few seconds of close).
    await apply(
      deps,
      thread,
      report,
      { type: "issue_closed", reason, by: p.sender.login, duplicateThreadId, comment: null },
      system,
    );
    return;
  }

  if (event === "issues" && p.action === "reopened") {
    await apply(deps, thread, report, { type: "issue_reopened" }, system);
    return;
  }

  if (event === "issue_comment" && p.action === "created" && p.comment) {
    const c = p.comment;
    const relay = shouldRelayToDiscord({
      authorAssociation: c.author_association,
      authorIsBot: c.user.type === "Bot",
      body: c.body,
      internalMarker: deps.cfg.INTERNAL_COMMENT_MARKER,
    });
    const echoed = deps.db.prepare("SELECT 1 FROM messages WHERE github_comment_id = ?").get(c.id);
    if (!relay || echoed) return;

    if (thread.archived) await thread.setArchived(false);
    // TODO(decision): embed vs. per-thread webhook with the GitHub avatar.
    const parts = splitMessage(`**${c.user.login}** (developer): ${sanitizeForDiscord(c.body)}`);
    for (const [n, part] of parts.entries()) {
      const sent = await thread.send({ content: part, allowedMentions: { parse: [] } });
      // The comment ID goes on the first part only; it is unique in the mapping.
      deps.db
        .prepare(
          "INSERT INTO messages (thread_id, discord_message_id, github_comment_id, direction, content_hash) VALUES (?, ?, ?, 'github_to_discord', ?)",
        )
        .run(report.threadId, sent.id, n === 0 ? c.id : null, String(c.body.length));
    }
  }
}

function toCloseReason(stateReason: string | null | undefined): CloseReason {
  if (stateReason === "not_planned" || stateReason === "duplicate") return stateReason;
  return "completed";
}
