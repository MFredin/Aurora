// The triage gate. Pure: (report, action, actor) -> next report + effects.
// The Discord and GitHub adapters execute the effects; nothing here does I/O.

import { closeSummary } from "./format.js";

export const STATUSES = [
  "new",
  "acknowledged",
  "needs_info",
  "synced",
  "resolved",
  "not_planned",
  "duplicate",
  "rejected",
] as const;
export type Status = (typeof STATUSES)[number];

export const CATEGORIES = ["crash", "bug", "performance", "ui", "install", "feature"] as const;
export type Category = (typeof CATEGORIES)[number];

export const PRIORITIES = ["p0", "p1", "p2", "p3"] as const;
export type Priority = (typeof PRIORITIES)[number];

export const REJECT_REASONS = ["not_a_bug", "cant_reproduce", "wont_fix", "spam"] as const;
export type RejectReason = (typeof REJECT_REASONS)[number];

export type CloseReason = "completed" | "not_planned" | "duplicate";

/** Everyone is a member; roles come from the configured Discord role IDs. */
export type Role = "member" | "triager" | "maintainer";

export interface Actor {
  id: string;
  name: string;
  role: Role;
}

export interface Report {
  threadId: string;
  reporterId: string;
  status: Status;
  category: Category | null;
  priority: Priority | null;
  acknowledgedBy: string | null;
  issueNumber: number | null;
}

export type Action =
  | { type: "ack" }
  | { type: "set"; category?: Category; priority?: Priority }
  | { type: "need_info"; note: string }
  | { type: "reporter_replied" }
  | { type: "sync" }
  | { type: "duplicate"; issueNumber: number | null; threadId: string | null }
  | { type: "reject"; reason: RejectReason; note?: string }
  | { type: "reopen" }
  | { type: "link"; issueNumber: number }
  | { type: "unlink" }
  | { type: "issue_closed"; reason: CloseReason; by: string; duplicateThreadId: string | null; comment: string | null }
  | { type: "issue_reopened" };

export type Effect =
  /** Create the issue: labels from category/priority, body with a digest of the thread so far. */
  | { type: "create_issue" }
  | { type: "sync_labels"; issueNumber: number }
  | { type: "comment_on_issue"; issueNumber: number; body: string }
  | { type: "post_in_thread"; text: string }
  | { type: "close_thread" }
  | { type: "reopen_thread" }
  /** Re-render the status card and forum tags from the new report. */
  | { type: "refresh_card" }
  | { type: "audit"; text: string };

export type Outcome = { ok: true; report: Report; effects: Effect[] } | { ok: false; error: string };

const PRE_SYNC: readonly Status[] = ["new", "acknowledged", "needs_info"];
const CLOSED_BY_GITHUB: readonly Status[] = ["resolved", "not_planned", "duplicate"];

const RANK: Record<Role, number> = { member: 0, triager: 1, maintainer: 2 };

/** Minimum role per action. `null` means the bot itself raises it (gateway or webhook event). */
const REQUIRED: Record<Action["type"], Role | null> = {
  ack: "triager",
  set: "triager",
  need_info: "triager",
  sync: "triager",
  duplicate: "triager",
  reject: "triager",
  reopen: "triager",
  link: "maintainer",
  unlink: "maintainer",
  reporter_replied: null,
  issue_closed: null,
  issue_reopened: null,
};

export const REJECT_TEXT: Record<RejectReason, string> = {
  not_a_bug: "This is working as intended, so it won't be tracked as a bug.",
  cant_reproduce: "We couldn't reproduce this. Reply with more detail and a triager can reopen it.",
  wont_fix: "We've decided not to fix this.",
  spam: "Closed.",
};

export function canPerform(role: Role, action: Action["type"]): boolean {
  const need = REQUIRED[action];
  return need === null || RANK[role] >= RANK[need];
}

/** Missing fields that block `sync`, empty when the report is ready. */
export function syncBlockers(r: Report): string[] {
  const missing: string[] = [];
  if (r.status === "needs_info") missing.push("waiting on the reporter");
  else if (r.status !== "acknowledged") missing.push("acknowledged");
  if (!r.category) missing.push("category");
  if (!r.priority) missing.push("priority");
  return missing;
}

export function transition(r: Report, action: Action, actor: Actor): Outcome {
  if (!canPerform(actor.role, action.type)) {
    return fail("You don't have permission to do that. Ask a triager.");
  }
  const by = actor.name;

  switch (action.type) {
    case "ack": {
      if (r.status === "acknowledged") return fail("Already acknowledged.");
      if (!PRE_SYNC.includes(r.status)) return fail(`Can't acknowledge a report that is ${r.status}.`);
      return ok({ ...r, status: "acknowledged", acknowledgedBy: actor.id }, [
        { type: "refresh_card" },
        { type: "audit", text: `${by} acknowledged` },
      ]);
    }

    case "set": {
      if (!action.category && !action.priority) return fail("Give a category, a priority, or both.");
      if (!PRE_SYNC.includes(r.status) && r.status !== "synced") {
        return fail(`Can't change a report that is ${r.status}.`);
      }
      const next: Report = {
        ...r,
        category: action.category ?? r.category,
        priority: action.priority ?? r.priority,
      };
      // Categorising a new report counts as acknowledging it.
      if (r.status === "new") {
        next.status = "acknowledged";
        next.acknowledgedBy = actor.id;
      }
      const effects: Effect[] = [{ type: "refresh_card" }];
      if (r.status === "synced" && r.issueNumber !== null) {
        effects.push({ type: "sync_labels", issueNumber: r.issueNumber });
      }
      effects.push({ type: "audit", text: `${by} set ${describe(next)}` });
      return ok(next, effects);
    }

    case "need_info": {
      if (!PRE_SYNC.includes(r.status)) return fail(`Can't ask for info on a report that is ${r.status}.`);
      return ok({ ...r, status: "needs_info", acknowledgedBy: r.acknowledgedBy ?? actor.id }, [
        { type: "post_in_thread", text: `<@${r.reporterId}> ${action.note}` },
        { type: "refresh_card" },
        { type: "audit", text: `${by} asked for more info` },
      ]);
    }

    case "reporter_replied": {
      if (r.status !== "needs_info") return ok(r, []);
      return ok({ ...r, status: "acknowledged" }, [
        { type: "refresh_card" },
        { type: "audit", text: "Reporter replied; back in the queue" },
      ]);
    }

    case "sync": {
      if (r.status === "synced") return fail(`Already tracked as #${r.issueNumber ?? "?"}.`);
      const missing = syncBlockers(r);
      if (missing.length > 0) return fail(`Not ready to sync. Missing: ${missing.join(", ")}.`);
      return ok({ ...r, status: "synced" }, [
        { type: "create_issue" },
        { type: "refresh_card" },
        { type: "audit", text: `${by} synced to GitHub (${describe(r)})` },
      ]);
    }

    case "duplicate": {
      if (!PRE_SYNC.includes(r.status)) {
        return fail("Only untracked reports can be marked duplicate here. Close tracked ones on GitHub.");
      }
      if (action.issueNumber === null && action.threadId === null) {
        return fail("Say which issue or thread this duplicates.");
      }
      const effects: Effect[] = [];
      // Fold the report into the existing issue instead of opening another one.
      if (action.issueNumber !== null) {
        effects.push({
          type: "comment_on_issue",
          issueNumber: action.issueNumber,
          body: `Also reported on Discord: thread ${r.threadId}`,
        });
      }
      const target = action.threadId !== null ? `<#${action.threadId}>` : "an existing report";
      effects.push(
        { type: "post_in_thread", text: `Closed as a duplicate of ${target}. Follow that one for updates.` },
        { type: "refresh_card" },
        { type: "close_thread" },
        { type: "audit", text: `${by} closed as duplicate` },
      );
      return ok({ ...r, status: "duplicate" }, effects);
    }

    case "reject": {
      if (!PRE_SYNC.includes(r.status)) return fail("Only untracked reports can be rejected. Close tracked ones on GitHub.");
      const note = action.note ? `\n> ${action.note}` : "";
      return ok({ ...r, status: "rejected" }, [
        { type: "post_in_thread", text: REJECT_TEXT[action.reason] + note },
        { type: "refresh_card" },
        { type: "close_thread" },
        { type: "audit", text: `${by} rejected (${action.reason})` },
      ]);
    }

    case "reopen": {
      if (r.issueNumber !== null) return fail(`This is tracked as #${r.issueNumber}. Reopen it on GitHub.`);
      if (r.status !== "rejected" && r.status !== "duplicate") return fail("This report is already open.");
      return ok({ ...r, status: "acknowledged" }, [
        { type: "reopen_thread" },
        { type: "refresh_card" },
        { type: "audit", text: `${by} reopened` },
      ]);
    }

    case "link": {
      if (r.issueNumber !== null) return fail(`Already linked to #${r.issueNumber}. Unlink first.`);
      return ok({ ...r, status: "synced", issueNumber: action.issueNumber }, [
        { type: "refresh_card" },
        { type: "audit", text: `${by} linked to #${action.issueNumber}` },
      ]);
    }

    case "unlink": {
      if (r.issueNumber === null) return fail("Not linked to an issue.");
      return ok({ ...r, status: "acknowledged", issueNumber: null }, [
        { type: "refresh_card" },
        { type: "audit", text: `${by} unlinked from #${r.issueNumber}` },
      ]);
    }

    case "issue_closed": {
      if (r.status !== "synced") return ok(r, []); // redelivery or already closed
      const status: Status = action.reason === "completed" ? "resolved" : action.reason;
      return ok({ ...r, status }, [
        { type: "post_in_thread", text: closeSummary(action.reason, action.by, action.duplicateThreadId, action.comment) },
        { type: "refresh_card" },
        { type: "close_thread" },
      ]);
    }

    case "issue_reopened": {
      if (!CLOSED_BY_GITHUB.includes(r.status)) return ok(r, []);
      return ok({ ...r, status: "synced" }, [
        { type: "reopen_thread" },
        { type: "post_in_thread", text: "This was reopened by the developers." },
        { type: "refresh_card" },
      ]);
    }
  }
}

function describe(r: Report): string {
  return `category ${r.category ?? "unset"}, priority ${r.priority ?? "unset"}`;
}

function ok(report: Report, effects: Effect[]): Outcome {
  return { ok: true, report, effects };
}

function fail(error: string): Outcome {
  return { ok: false, error };
}
