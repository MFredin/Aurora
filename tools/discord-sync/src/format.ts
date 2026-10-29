// Text shaping between Discord and GitHub. Pure functions only.

import type { CloseReason } from "./triage.js";

export const DISCORD_MAX = 2000;
export const GITHUB_MAX = 65_536;

const ZWSP = "​";

/**
 * Stop Discord text from pinging GitHub users or cross-referencing issues.
 * `@name` would notify a real GitHub user; `#12` adds a backlink event to issue 12.
 */
export function sanitizeForGitHub(text: string): string {
  return text
    .replace(/@(?=[A-Za-z0-9-])/g, `@${ZWSP}`)
    .replace(/(^|[^\w&])#(?=\d)/g, `$1#${ZWSP}`);
}

/**
 * GitHub text is posted with `allowedMentions: { parse: [] }` so nothing pings,
 * but `@everyone` still renders and alarms people, so break it visually too.
 */
export function sanitizeForDiscord(text: string): string {
  return text.replace(/@(everyone|here)/g, `@${ZWSP}$1`);
}

/** Split on line breaks where possible so code blocks and lists survive. */
export function splitMessage(text: string, max = DISCORD_MAX): string[] {
  const parts: string[] = [];
  let rest = text;
  while (rest.length > max) {
    let cut = rest.lastIndexOf("\n", max);
    if (cut < max / 2) cut = rest.lastIndexOf(" ", max);
    if (cut < max / 2) cut = max;
    parts.push(rest.slice(0, cut));
    rest = rest.slice(cut).replace(/^\n/, "");
  }
  if (rest.length > 0) parts.push(rest);
  return parts;
}

export function truncate(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max - 20)}\n\n_(truncated)_`;
}

export function closeSummary(
  reason: CloseReason,
  by: string,
  duplicateThreadId: string | null,
  comment: string | null,
): string {
  const head =
    reason === "completed"
      ? `Resolved by ${by}.`
      : reason === "not_planned"
        ? `Closed as not planned by ${by}.`
        : duplicateThreadId !== null
          ? `Closed as a duplicate of <#${duplicateThreadId}> by ${by}.`
          : `Closed as a duplicate by ${by}.`;
  const body = comment ? `\n> ${comment.trim().split("\n").join("\n> ")}` : "";
  return sanitizeForDiscord(head + body);
}

export interface RelayCheck {
  authorAssociation: string;
  authorIsBot: boolean;
  body: string;
  internalMarker: string;
}

/** Only maintainers' comments reach Discord, and never ones marked internal. */
export function shouldRelayToDiscord(c: RelayCheck): boolean {
  if (c.authorIsBot) return false;
  if (!["OWNER", "MEMBER", "COLLABORATOR"].includes(c.authorAssociation)) return false;
  return !c.body.trimStart().startsWith(c.internalMarker);
}

/** "+1", "same", "me too" or emoji-only replies don't become GitHub comments. */
export function isLowSignal(text: string): boolean {
  const t = text
    .trim()
    .toLowerCase()
    .replace(/<a?:\w+:\d+>/g, "") // custom emoji
    .replace(/[\p{Extended_Pictographic}‍️\s!.]/gu, "");
  return t === "" || /^(\+1|same|sameissue|sametome|metoo|samehere|bump|this)$/.test(t);
}

export interface DigestMessage {
  author: string;
  content: string;
  attachments: string[];
}

/** Hidden marker that lets reconcile find the issue for a thread after a crash mid-create. */
export function threadMarker(threadId: string): string {
  return `<!-- discord-thread:${threadId} -->`;
}

export function issueBody(opts: {
  threadId: string;
  threadUrl: string;
  starter: DigestMessage;
  replies: DigestMessage[];
  triagedBy: string;
}): string {
  const block = (m: DigestMessage) =>
    [
      `**${m.author}** (Discord):`,
      sanitizeForGitHub(m.content),
      ...m.attachments.map((u) => `- ${u}`),
    ].join("\n");
  const sections = [
    block(opts.starter),
    `[Open the Discord thread](${opts.threadUrl}) · triaged by ${opts.triagedBy}`,
  ];
  if (opts.replies.length > 0) {
    sections.push(
      `<details><summary>${opts.replies.length} earlier replies</summary>\n\n${opts.replies.map(block).join("\n\n")}\n\n</details>`,
    );
  }
  sections.push(threadMarker(opts.threadId));
  return truncate(sections.join("\n\n"), GITHUB_MAX);
}

export function commentBody(m: DigestMessage): string {
  const lines = [`**${m.author}** (Discord): ${sanitizeForGitHub(m.content)}`, ...m.attachments.map((u) => `- ${u}`)];
  return truncate(lines.join("\n"), GITHUB_MAX);
}
