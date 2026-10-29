import { createHmac } from "node:crypto";
import { describe, expect, it } from "vitest";
import * as store from "../src/db.js";
import { verifySignature } from "../src/github/signature.js";
import { roleOf } from "../src/roles.js";
import { ALL_TAG_NAMES, MAX_APPLIED_TAGS, MAX_FORUM_TAGS, labelsFor, nextAppliedTags, tagNamesFor } from "../src/tags.js";
import type { Report } from "../src/triage.js";

const report: Report = {
  threadId: "t",
  reporterId: "r",
  status: "acknowledged",
  category: "crash",
  priority: "p1",
  acknowledgedBy: "x",
  issueNumber: null,
};

describe("verifySignature", () => {
  const secret = "s3cret-s3cret-s3cret";
  const body = '{"action":"closed"}';
  const sig = `sha256=${createHmac("sha256", secret).update(body).digest("hex")}`;

  it("accepts a valid signature", () => {
    expect(verifySignature(secret, body, sig)).toBe(true);
    expect(verifySignature(secret, Buffer.from(body), sig)).toBe(true);
  });
  it("rejects a tampered body, wrong secret, missing or malformed header", () => {
    expect(verifySignature(secret, body + " ", sig)).toBe(false);
    expect(verifySignature("other-secret-other", body, sig)).toBe(false);
    expect(verifySignature(secret, body, undefined)).toBe(false);
    expect(verifySignature(secret, body, "sha1=abc")).toBe(false);
    expect(verifySignature(secret, body, "sha256=short")).toBe(false);
  });
});

describe("tags", () => {
  it("fits Discord's forum limits", () => {
    expect(ALL_TAG_NAMES.length).toBeLessThanOrEqual(MAX_FORUM_TAGS);
    expect(new Set(ALL_TAG_NAMES).size).toBe(ALL_TAG_NAMES.length);
    expect(tagNamesFor(report).length).toBeLessThanOrEqual(MAX_APPLIED_TAGS);
  });
  it("swaps our tags and keeps moderators' own tags", () => {
    const ids = new Map(ALL_TAG_NAMES.map((n, i) => [n, `id${i}`]));
    const newId = ids.get("New")!;
    const next = nextAppliedTags([newId, "mod-tag"], report, ids);
    expect(next).not.toContain(newId);
    expect(next).toContain("mod-tag");
    expect(next).toContain(ids.get("Acknowledged"));
    expect(next).toContain(ids.get("Crash"));
  });
  it("labels carry provenance, type and priority", () => {
    expect(labelsFor(report)).toEqual(["discord", "type: crash", "priority: p1"]);
  });
});

describe("roleOf", () => {
  const cfg = { DISCORD_TRIAGER_ROLE_IDS: ["T"], DISCORD_MAINTAINER_ROLE_IDS: ["M"] };
  it("picks the highest role", () => {
    expect(roleOf(["T", "M"], cfg)).toBe("maintainer");
    expect(roleOf(["T"], cfg)).toBe("triager");
    expect(roleOf(["other"], cfg)).toBe("member");
  });
});

describe("db", () => {
  const db = store.openDb(":memory:");
  const base = { threadId: "th", channelId: "c", reporterId: "r", title: "Crash", repo: "o/r" };

  it("inserts a thread once even if threadCreate fires twice", () => {
    expect(store.insertReport(db, base)).toBe(true);
    expect(store.insertReport(db, base)).toBe(false);
    expect(store.getReport(db, "th")?.status).toBe("new");
  });

  it("round-trips a report and finds it by issue", () => {
    store.saveReport(db, { ...report, threadId: "th", reporterId: "r", status: "synced", issueNumber: 4 });
    expect(store.getReportByIssue(db, "o/r", 4)?.category).toBe("crash");
    expect(store.listQueue(db)).toHaveLength(0);
  });

  it("dedupes webhook deliveries", () => {
    expect(store.claimDelivery(db, "d1")).toBe(true);
    expect(store.claimDelivery(db, "d1")).toBe(false);
  });

  it("re-running migrations is a no-op", () => {
    expect(() => store.migrate(db)).not.toThrow();
  });
});
