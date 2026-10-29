import { describe, expect, it } from "vitest";
import { canPerform, syncBlockers, transition, type Actor, type Report } from "../src/triage.js";

const member: Actor = { id: "m1", name: "Mia", role: "member" };
const triager: Actor = { id: "t1", name: "Theo", role: "triager" };
const maintainer: Actor = { id: "x1", name: "Max", role: "maintainer" };
const github: Actor = { id: "github:dev", name: "dev", role: "maintainer" };

const fresh = (over: Partial<Report> = {}): Report => ({
  threadId: "th1",
  reporterId: "r1",
  status: "new",
  category: null,
  priority: null,
  acknowledgedBy: null,
  issueNumber: null,
  ...over,
});

const ready = (over: Partial<Report> = {}) =>
  fresh({ status: "acknowledged", category: "bug", priority: "p2", acknowledgedBy: "t1", ...over });

function okOf(out: ReturnType<typeof transition>) {
  if (!out.ok) throw new Error(`expected ok, got: ${out.error}`);
  return out;
}

describe("permissions", () => {
  it("members can't triage", () => {
    for (const type of ["ack", "set", "need_info", "sync", "duplicate", "reject", "reopen"] as const) {
      expect(canPerform("member", type)).toBe(false);
    }
    const out = transition(fresh(), { type: "ack" }, member);
    expect(out.ok).toBe(false);
  });

  it("only maintainers can link and unlink", () => {
    expect(canPerform("triager", "link")).toBe(false);
    expect(canPerform("maintainer", "link")).toBe(true);
    expect(transition(fresh(), { type: "link", issueNumber: 3 }, triager).ok).toBe(false);
  });

  it("system events need no role", () => {
    expect(canPerform("member", "reporter_replied")).toBe(true);
    expect(canPerform("member", "issue_closed")).toBe(true);
  });
});

describe("triage gate", () => {
  it("a new report can't be synced", () => {
    const out = transition(fresh(), { type: "sync" }, triager);
    expect(out.ok).toBe(false);
    expect(syncBlockers(fresh())).toEqual(["acknowledged", "category", "priority"]);
  });

  it("acknowledged but uncategorised can't be synced", () => {
    const r = fresh({ status: "acknowledged" });
    expect(transition(r, { type: "sync" }, triager).ok).toBe(false);
    expect(syncBlockers(r)).toEqual(["category", "priority"]);
  });

  it("needs_info blocks sync even when categorised", () => {
    const r = ready({ status: "needs_info" });
    expect(syncBlockers(r)).toEqual(["waiting on the reporter"]);
    expect(transition(r, { type: "sync" }, triager).ok).toBe(false);
  });

  it("ack, set, sync creates exactly one issue", () => {
    let r = okOf(transition(fresh(), { type: "ack" }, triager)).report;
    expect(r.status).toBe("acknowledged");
    expect(r.acknowledgedBy).toBe("t1");
    r = okOf(transition(r, { type: "set", category: "crash", priority: "p1" }, triager)).report;
    const out = okOf(transition(r, { type: "sync" }, triager));
    expect(out.report.status).toBe("synced");
    expect(out.effects.filter((e) => e.type === "create_issue")).toHaveLength(1);
    // A second sync (double click, redelivered interaction) is refused.
    expect(transition({ ...out.report, issueNumber: 7 }, { type: "sync" }, triager).ok).toBe(false);
  });

  it("setting a category on a new report acknowledges it", () => {
    const out = okOf(transition(fresh(), { type: "set", category: "ui" }, triager));
    expect(out.report.status).toBe("acknowledged");
    expect(out.report.acknowledgedBy).toBe("t1");
    expect(out.report.priority).toBeNull();
  });

  it("set needs at least one field", () => {
    expect(transition(fresh(), { type: "set" }, triager).ok).toBe(false);
  });

  it("re-prioritising a tracked report syncs labels", () => {
    const r = ready({ status: "synced", issueNumber: 9 });
    const out = okOf(transition(r, { type: "set", priority: "p0" }, triager));
    expect(out.report.priority).toBe("p0");
    expect(out.effects).toContainEqual({ type: "sync_labels", issueNumber: 9 });
  });

  it("acknowledging twice is refused", () => {
    expect(transition(ready(), { type: "ack" }, triager).ok).toBe(false);
  });
});

describe("needs info", () => {
  it("pings the reporter and the reporter's reply re-queues it", () => {
    const asked = okOf(transition(ready(), { type: "need_info", note: "Which GPU?" }, triager));
    expect(asked.report.status).toBe("needs_info");
    expect(asked.effects[0]).toEqual({ type: "post_in_thread", text: "<@r1> Which GPU?" });
    const back = okOf(transition(asked.report, { type: "reporter_replied" }, triager));
    expect(back.report.status).toBe("acknowledged");
  });

  it("a reply outside needs_info changes nothing", () => {
    const out = okOf(transition(ready(), { type: "reporter_replied" }, triager));
    expect(out.report).toEqual(ready());
    expect(out.effects).toEqual([]);
  });
});

describe("closing without an issue", () => {
  it("duplicate of an issue folds into it instead of opening another", () => {
    const out = okOf(transition(fresh(), { type: "duplicate", issueNumber: 42, threadId: null }, triager));
    expect(out.report.status).toBe("duplicate");
    expect(out.effects.some((e) => e.type === "create_issue")).toBe(false);
    expect(out.effects).toContainEqual(expect.objectContaining({ type: "comment_on_issue", issueNumber: 42 }));
    expect(out.effects).toContainEqual({ type: "close_thread" });
  });

  it("duplicate needs a target", () => {
    expect(transition(fresh(), { type: "duplicate", issueNumber: null, threadId: null }, triager).ok).toBe(false);
  });

  it("a tracked report can't be rejected from Discord", () => {
    const r = ready({ status: "synced", issueNumber: 3 });
    expect(transition(r, { type: "reject", reason: "not_a_bug" }, triager).ok).toBe(false);
    expect(transition(r, { type: "duplicate", issueNumber: 1, threadId: null }, triager).ok).toBe(false);
  });

  it("reject closes and reopen brings it back", () => {
    const rejected = okOf(transition(fresh(), { type: "reject", reason: "cant_reproduce", note: "Try 1.2" }, triager));
    expect(rejected.report.status).toBe("rejected");
    expect(rejected.effects).toContainEqual({ type: "close_thread" });
    const reopened = okOf(transition(rejected.report, { type: "reopen" }, triager));
    expect(reopened.report.status).toBe("acknowledged");
    expect(reopened.effects).toContainEqual({ type: "reopen_thread" });
  });

  it("reopen is refused for open or tracked reports", () => {
    expect(transition(ready(), { type: "reopen" }, triager).ok).toBe(false);
    expect(transition(ready({ status: "resolved", issueNumber: 2 }), { type: "reopen" }, triager).ok).toBe(false);
  });
});

describe("GitHub close and reopen", () => {
  const synced = ready({ status: "synced", issueNumber: 5 });
  const close = (reason: "completed" | "not_planned" | "duplicate", duplicateThreadId: string | null = null) =>
    okOf(transition(synced, { type: "issue_closed", reason, by: "dev", duplicateThreadId, comment: null }, github));

  it.each([
    ["completed", "resolved", "Resolved by dev."],
    ["not_planned", "not_planned", "Closed as not planned by dev."],
    ["duplicate", "duplicate", "Closed as a duplicate by dev."],
  ] as const)("%s -> %s, posts the outcome and closes the thread", (reason, status, text) => {
    const out = close(reason);
    expect(out.report.status).toBe(status);
    expect(out.effects[0]).toEqual({ type: "post_in_thread", text });
    expect(out.effects).toContainEqual({ type: "close_thread" });
  });

  it("a redelivered close is a no-op", () => {
    const once = close("completed");
    const twice = okOf(
      transition(once.report, { type: "issue_closed", reason: "completed", by: "dev", duplicateThreadId: null, comment: null }, github),
    );
    expect(twice.effects).toEqual([]);
  });

  it("reopen on GitHub reopens the thread", () => {
    const out = okOf(transition(close("not_planned").report, { type: "issue_reopened" }, github));
    expect(out.report.status).toBe("synced");
    expect(out.effects[0]).toEqual({ type: "reopen_thread" });
  });

  it("reopen of a report that isn't closed by GitHub is a no-op", () => {
    expect(okOf(transition(synced, { type: "issue_reopened" }, github)).effects).toEqual([]);
  });
});

describe("link and unlink", () => {
  it("links an untracked thread to an existing issue without creating one", () => {
    const out = okOf(transition(fresh(), { type: "link", issueNumber: 11 }, maintainer));
    expect(out.report).toMatchObject({ status: "synced", issueNumber: 11 });
    expect(out.effects.some((e) => e.type === "create_issue")).toBe(false);
  });

  it("refuses to link twice and to unlink nothing", () => {
    expect(transition(ready({ issueNumber: 1 }), { type: "link", issueNumber: 2 }, maintainer).ok).toBe(false);
    expect(transition(fresh(), { type: "unlink" }, maintainer).ok).toBe(false);
  });
});
