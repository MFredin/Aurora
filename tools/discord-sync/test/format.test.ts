import { describe, expect, it } from "vitest";
import {
  closeSummary,
  issueBody,
  isLowSignal,
  sanitizeForDiscord,
  sanitizeForGitHub,
  shouldRelayToDiscord,
  splitMessage,
  threadMarker,
} from "../src/format.js";

const ZWSP = "​";

describe("sanitizeForGitHub", () => {
  it("stops @mentions from pinging GitHub users", () => {
    expect(sanitizeForGitHub("cc @octocat")).toBe(`cc @${ZWSP}octocat`);
  });
  it("stops #123 from cross-referencing issues", () => {
    expect(sanitizeForGitHub("see #12")).toBe(`see #${ZWSP}12`);
    expect(sanitizeForGitHub("#7 at start")).toBe(`#${ZWSP}7 at start`);
  });
  it("leaves HTML entities and channel names alone", () => {
    expect(sanitizeForGitHub("&#39;")).toBe("&#39;");
    expect(sanitizeForGitHub("#general")).toBe("#general");
  });
});

describe("sanitizeForDiscord", () => {
  it("breaks @everyone and @here", () => {
    expect(sanitizeForDiscord("hey @everyone and @here")).toBe(`hey @${ZWSP}everyone and @${ZWSP}here`);
  });
});

describe("splitMessage", () => {
  it("returns short text unchanged", () => {
    expect(splitMessage("hi")).toEqual(["hi"]);
  });
  it("keeps every part within the limit and loses nothing but the break", () => {
    const text = Array.from({ length: 300 }, (_, i) => `line ${i} ${"x".repeat(10)}`).join("\n");
    const parts = splitMessage(text, 2000);
    expect(parts.length).toBeGreaterThan(1);
    for (const p of parts) expect(p.length).toBeLessThanOrEqual(2000);
    expect(parts.join("\n")).toBe(text);
  });
  it("hard-cuts text with no breaks", () => {
    const parts = splitMessage("y".repeat(4500), 2000);
    expect(parts.map((p) => p.length)).toEqual([2000, 2000, 500]);
  });
});

describe("closeSummary", () => {
  it("quotes the closing comment", () => {
    expect(closeSummary("completed", "dev", null, "Fixed in 1.3\nThanks")).toBe("Resolved by dev.\n> Fixed in 1.3\n> Thanks");
  });
  it("points at the original thread when the duplicate target is known", () => {
    expect(closeSummary("duplicate", "dev", "999", null)).toBe("Closed as a duplicate of <#999> by dev.");
  });
});

describe("shouldRelayToDiscord", () => {
  const base = { authorAssociation: "MEMBER", authorIsBot: false, body: "Looking into it", internalMarker: "[internal]" };
  it("relays maintainers", () => {
    expect(shouldRelayToDiscord(base)).toBe(true);
  });
  it("drops bots (loop prevention), outsiders and internal notes", () => {
    expect(shouldRelayToDiscord({ ...base, authorIsBot: true })).toBe(false);
    expect(shouldRelayToDiscord({ ...base, authorAssociation: "NONE" })).toBe(false);
    expect(shouldRelayToDiscord({ ...base, body: "  [internal] root cause is X" })).toBe(false);
  });
});

describe("isLowSignal", () => {
  it.each(["+1", "same", "Same here!", "me too", "👍", "<:pog:123456>", "bump", "  "])("%s is noise", (t) => {
    expect(isLowSignal(t)).toBe(true);
  });
  it.each(["same crash on my 4090", "It happens after alt-tab", "+1, also on Windows 10"])("%s is signal", (t) => {
    expect(isLowSignal(t)).toBe(false);
  });
});

describe("issueBody", () => {
  it("includes the starter, the thread link, a folded digest and the reconcile marker", () => {
    const body = issueBody({
      threadId: "123",
      threadUrl: "https://discord.com/channels/1/123",
      starter: { author: "Ana", content: "Crash on launch @dev", attachments: ["https://cdn/x.png"] },
      replies: [{ author: "Bo", content: "Same with #4", attachments: [] }],
      triagedBy: "Theo",
    });
    expect(body).toContain(`**Ana** (Discord):\nCrash on launch @${ZWSP}dev\n- https://cdn/x.png`);
    expect(body).toContain("[Open the Discord thread](https://discord.com/channels/1/123) · triaged by Theo");
    expect(body).toContain("<summary>1 earlier replies</summary>");
    expect(body).toContain(`#${ZWSP}4`);
    expect(body.endsWith(threadMarker("123"))).toBe(true);
  });
});
