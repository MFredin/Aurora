# Discord ↔ GitHub sync: design and workflow

Status: **scaffold**. The triage logic, formatting, signature check and DB layer are written and unit-tested.
The Discord and GitHub adapters are wired and typecheck, but nobody has run them against a real server yet.

## 1. What changed from the original spec

The spec creates an issue for every new thread. With a gamer community that floods the private repo
with duplicates, "+1"s and support questions. This design adds a **triage gate**:

| Spec | This design |
|---|---|
| Thread created → issue created | Thread created → report goes into a **triage queue**. No issue yet. |
| Every Discord message → comment | Before sync: nothing goes to GitHub. At sync: one issue whose body holds a folded digest of the thread so far. After sync: replies become comments, except low-signal ones ("+1", "same", emoji). |
| Bot posts status messages | One **status card** per thread, edited in place, plus **forum tags**. Command replies are ephemeral. Audit trail goes to a private log channel. |
| Duplicates become new issues | `/triage duplicate` closes the thread and adds a single "also reported" comment to the existing issue. |

## 2. Workflow

```
              /triage ack or /triage set
   ┌─────┐  ─────────────────────────────▶  ┌──────────────┐   /triage sync    ┌─────────┐
   │ New │                                  │ Acknowledged │ ────────────────▶ │ Tracked │──┐
   └─────┘                                  └──────────────┘  (needs category  └─────────┘  │ issue closed
      │                              /need-info ▲  │  ▲          and priority)       ▲        │ on GitHub
      │                                         │  ▼  │ reporter replies            │        ▼
      │                                     ┌────────────┐                    reopened  Resolved / Won't fix
      │                                     │ Needs info │                    on GitHub  / Duplicate
      │                                     └────────────┘                              (thread locked)
      │   /triage reject or /triage duplicate (from any untracked state)
      └──────────────────────────────────────────────────▶ Closed / Duplicate (thread locked, no issue)
                                                            /triage reopen brings it back
```

Who does what:

| Role | Configured by | Can |
|---|---|---|
| Member | everyone | post reports, reply, `/status` |
| Triager | `DISCORD_TRIAGER_ROLE_IDS` | `ack`, `set`, `need-info`, `sync`, `duplicate`, `reject`, `reopen`, `queue` |
| Maintainer | `DISCORD_MAINTAINER_ROLE_IDS` | everything above, plus `link` and `unlink` to attach existing issues |
| GitHub collaborators | repo access | comment (relayed to Discord unless prefixed `[internal]`), close, reopen |

### Commands

All `/triage` replies are ephemeral (only the person who ran the command sees them), so the thread stays clean.

| Command | Effect |
|---|---|
| `/triage ack` | Claim the report. Tag goes New → Acknowledged. |
| `/triage set category: priority:` | Set either or both. On a New report this also acknowledges it, so one command often does it all. On a tracked report it updates the GitHub labels. |
| `/triage need-info note:` | Pings the reporter with the note (the only visible bot message apart from the card). Their next reply puts it back in the queue automatically. |
| `/triage sync` | Creates the issue. Refused unless acknowledged, categorised and prioritised, and not waiting on the reporter. Can't double-create. |
| `/triage duplicate thread: issue:` | Closes as a duplicate. With `issue`, it adds an "also reported on Discord" comment to that issue instead of opening a new one. |
| `/triage reject reason: note:` | Not a bug / Can't reproduce / Won't fix / Spam. Posts a short canned reason, locks the thread. |
| `/triage reopen` | Undo a reject or duplicate. Tracked reports are reopened on GitHub instead. |
| `/triage queue` | Lists untriaged reports, oldest first, to you only. |
| `/triage link issue:` / `unlink` | Maintainers: attach a thread to an existing issue, or detach it. |
| `/status` | Anyone: where this report stands. Shows the issue link only if `ISSUE_LINK_VISIBILITY` allows it for your role. |

### Categories, priorities and tags

Forum tags carry the state so nobody needs to read bot messages. Discord allows 20 tags per forum and 5 applied
per post; this uses 18 and applies at most 3 (status + category + priority). Moderators' own tags are kept.

| Group | Tags | GitHub label |
|---|---|---|
| Status | New, Acknowledged, Needs info, Tracked, Resolved, Won't fix, Duplicate, Closed | (issue state) |
| Category | Crash, Bug, Performance, UI, Install, Feature request | `type: crash` … |
| Priority | P0 critical, P1 high, P2 normal, P3 low | `priority: p0` … |

Every synced issue also gets `discord`.

Suggested meanings for the triage team (change freely):

- **P0**: crash, data loss, or a fix that damages the PC. Drop everything.
- **P1**: a core feature is broken for many people with no workaround.
- **P2**: broken with a workaround, or affects few people.
- **P3**: cosmetic or minor.

## 3. Feasibility (spec section 9)

| Question | Finding |
|---|---|
| Archive **and** lock a thread | Yes, with Manage Threads: `thread.edit({ locked: true, archived: true })`. An archived thread must be unarchived before any edit or message, so the bot unarchives first. |
| Forum tags on close | Yes, `setAppliedTags`. Limits: 20 tags per forum, 5 applied per post. The tags must already exist on the forum (created by an admin; the bot matches by name). |
| Message Content intent | Needed to read replies. Privileged: toggle it in the developer portal. Verification only applies at 100+ servers, which a single-server bot won't hit. |
| `state_reason` on `issues.closed` | Present: `completed`, `not_planned`, `duplicate` (null on old issues; treated as completed). Confirmed `DUPLICATE` in GitHub's GraphQL schema. |
| Duplicate target | **Not in the webhook payload.** Only via GraphQL: the issue timeline's `MarkedAsDuplicateEvent.canonical`. Implemented in `github/client.ts#duplicateTarget`. The bot names the original only if it's also a Discord thread, so private issue numbers don't leak. |
| Posting as the GitHub user | Embed or a thread webhook. Webhooks need Manage Webhooks and one webhook per forum (use `thread_id` when executing). Scaffold posts plain text "**login** (developer):" for now. See decisions. |
| Rate limits | GitHub: content creation is limited to roughly 80 requests/min and 500/hour per installation (secondary limits). The triage gate keeps volume far below that. Discord: per-route limits handled by discord.js's built-in queue. |
| Hosting | Needs a persistent process (gateway) and public HTTPS (webhook). Railway does both from one service; SQLite needs a volume. `railway.json` + `Dockerfile` included; Railway rejects the Dockerfile `VOLUME` keyword so the volume is configured in the dashboard. |

Not verified here: GitHub's rate-limit numbers above come from memory; docs.github.com wasn't reachable from this
session. Check them before relying on exact figures.

## 4. Stack

TypeScript on Node 22.13+, `discord.js` 14, `@octokit/app` 16 (REST + GraphQL), Fastify 5, `zod` for config,
SQLite via the **built-in `node:sqlite`** (no native build, one less dependency; it prints an experimental
warning, which `start` silences), Vitest. No ORM: four tables and hand-written SQL migrations.

## 5. Reliability

- **Exactly one issue per thread**: the report is saved as `synced` *before* the create call. A crash between
  the two leaves `synced` with no issue number; reconcile finds the issue by the hidden
  `<!-- discord-thread:ID -->` marker in its body instead of creating another. *(reconcile: TODO)*
- **Webhook redelivery**: `X-GitHub-Delivery` IDs are claimed in the `deliveries` table; repeats are dropped.
  State transitions are also idempotent (closing a closed report is a no-op).
- **Loops**: Discord side ignores bot and webhook authors; GitHub side ignores `Bot` users and any comment ID
  already in the `messages` mapping.
- **Mentions**: Discord → GitHub escapes `@user` and `#123` (no pings, no backlink noise on other issues).
  GitHub → Discord sends with `allowedMentions: { parse: [] }` and breaks `@everyone`/`@here`.
- **Secrets**: config errors name the variable, never the value. The logger never prints payloads.

## 6. Decisions needed

Recommendations first; say the word and I'll change them.

1. **Issue link visibility in Discord.** Repo is private, so a public link is a dead end for players.
   *Recommend `triagers`*: card shows "Tracked", triagers see the link via `/status`.
2. **Relaying developer comments.** Spec relays all. *Recommend relay by default, skip comments starting with
   `[internal]`*, so devs can discuss root causes privately on the same issue.
3. **Developer identity in Discord.** *Recommend a forum webhook* (GitHub name + avatar, looks native) once the
   core works; plain text until then. Needs Manage Webhooks.
4. **Attachments.** Discord CDN links now expire. *Recommend link-only for now*; re-host screenshots and
   logs to the issue later if triagers find dead links a problem.
5. **Channel type.** *Forum*, required for tags; the scaffold assumes it.
6. **Auto-sync for P0.** Should setting P0 sync immediately, skipping the explicit `/triage sync`?
   *Recommend no*: one deliberate step is cheap and avoids mistakes.
7. **Needs-info timeout.** Close reports that sit in Needs info for `NEEDS_INFO_TIMEOUT_DAYS` (default 14)
   with a "reply and we'll reopen" note? *Recommend yes.* (Sweep not written yet.)
8. **Where this lives.** Scaffolded in `tools/discord-sync` of this repo. It can move to its own repo
   if you'd rather keep Forge's history clean; nothing depends on the Rust workspace.

## 7. Not done yet

- Reconcile on startup (missed events, crash mid-create) and the runbook for it.
- Needs-info timeout sweep.
- Closing comment in the close summary (fetch the closer's comment made at close time).
- Edits and deletes in either direction; label ↔ tag sync for labels changed on GitHub.
- Paging thread history past 100 messages for the sync digest.
- Webhook posting with GitHub avatars.
- A real run against a test Discord server and a test repo.
