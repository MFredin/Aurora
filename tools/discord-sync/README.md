# Discord ↔ GitHub sync

Turns bug reports in a Discord forum into issues in a private GitHub repo, **after** a triager has
acknowledged, categorised and prioritised them. Developer comments flow back to the thread, and closing the
issue closes the thread. See [DESIGN.md](DESIGN.md) for the workflow, commands and open decisions.

## Develop

```bash
cd tools/discord-sync
npm install
npm test            # unit tests, no Discord or GitHub needed
npm run typecheck
cp .env.example .env  # then fill it in
npm run register-commands
npm run dev
```

Needs Node 22.13 or later (uses the built-in `node:sqlite`).

## Set up Discord

1. Developer portal → **New application** → Bot. Copy the token (`DISCORD_TOKEN`) and the application ID (`DISCORD_APP_ID`).
2. Bot → **Privileged gateway intents** → turn on **Message Content**.
3. OAuth2 → URL generator: scopes `bot` and `applications.commands`; permissions View Channel,
   Read Message History, Send Messages, Send Messages in Threads, Embed Links, **Manage Threads**
   (and Manage Webhooks later, if decision 3 goes that way). Invite it to the server.
4. Create a **forum** channel for bug reports. Copy its ID (`DISCORD_WATCHED_CHANNEL_ID`; turn on Developer Mode to copy IDs).
5. In the forum's settings, create these tags exactly (the bot matches by name):
   New, Acknowledged, Needs info, Tracked, Resolved, Won't fix, Duplicate, Closed,
   Crash, Bug, Performance, UI, Install, Feature request, P0 critical, P1 high, P2 normal, P3 low.
   On each tag, turn on the moderators-only option so players can't set their own status or priority.
   The bot needs Manage Threads to apply them.
6. Create a **Triager** role (and optionally **Maintainer**). Put their IDs in the role env vars.
7. Server settings → Integrations → the bot → `/triage`: allow the Triager and Maintainer roles.
   `/triage` is hidden from everyone without Manage Threads by default; the bot also checks roles itself.
8. Optional: a private `#triage-log` channel (`DISCORD_LOG_CHANNEL_ID`) for the audit trail.

## Set up GitHub

1. Settings → Developer settings → **GitHub Apps** → New. Repository permissions: **Issues: read and write**,
   Metadata: read. Subscribe to events **Issues** and **Issue comment**.
2. Webhook URL: `https://<your-host>/github/webhook`. Generate a long random secret
   (`openssl rand -hex 32`) and set it as the webhook secret and `GITHUB_WEBHOOK_SECRET`.
3. Generate a private key; put the PEM in `GITHUB_APP_PRIVATE_KEY` (newlines as `\n` are fine).
4. Install the app on the private repo only. The installation ID is the number at the end of the
   installation's settings URL (`GITHUB_INSTALLATION_ID`).

## Deploy (Railway)

1. New service from this repo, root directory `tools/discord-sync` (it builds from the `Dockerfile`).
2. Add a **volume** mounted at `/app/data` so the SQLite database survives deploys.
3. Set the env vars from `.env.example`. Generate a public domain and use it for the GitHub webhook URL.
4. Keep it at **one replica**: two would both hold a gateway connection and double-post.
5. Run `npm run register-commands` once locally (or after any command change).

## Privacy note for the team

The repo is private but comments from collaborators are posted publicly in Discord. Start a comment with
`[internal]` to keep it on GitHub only.
