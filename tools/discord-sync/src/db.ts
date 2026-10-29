// SQLite via the built-in node:sqlite (no native dependency). Migrations are ordered
// SQL strings tracked with PRAGMA user_version.

import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { DatabaseSync } from "node:sqlite";
import type { Category, Priority, Report, Status } from "./triage.js";

const MIGRATIONS: string[] = [
  `
  CREATE TABLE reports (
    thread_id        TEXT PRIMARY KEY,
    channel_id       TEXT NOT NULL,
    reporter_id      TEXT NOT NULL,
    title            TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'new',
    category         TEXT,
    priority         TEXT,
    acknowledged_by  TEXT,
    github_repo      TEXT NOT NULL,
    issue_number     INTEGER,
    card_message_id  TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    closed_at        TEXT
  );
  CREATE UNIQUE INDEX reports_issue ON reports (github_repo, issue_number) WHERE issue_number IS NOT NULL;
  CREATE INDEX reports_status ON reports (status);

  CREATE TABLE messages (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id          TEXT NOT NULL REFERENCES reports (thread_id),
    discord_message_id TEXT UNIQUE,
    github_comment_id  INTEGER UNIQUE,
    direction          TEXT NOT NULL CHECK (direction IN ('discord_to_github', 'github_to_discord')),
    content_hash       TEXT NOT NULL,
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
  );

  -- X-GitHub-Delivery IDs already handled, for redelivery dedupe.
  CREATE TABLE deliveries (
    delivery_id TEXT PRIMARY KEY,
    received_at TEXT NOT NULL DEFAULT (datetime('now'))
  );

  CREATE TABLE audit (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id  TEXT NOT NULL,
    actor_id   TEXT,
    text       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
  );
  `,
];

export function openDb(path: string): DatabaseSync {
  if (path !== ":memory:") mkdirSync(dirname(path), { recursive: true });
  const db = new DatabaseSync(path);
  db.exec("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;");
  migrate(db);
  return db;
}

export function migrate(db: DatabaseSync): void {
  const row = db.prepare("PRAGMA user_version").get() as { user_version: number };
  for (let v = row.user_version; v < MIGRATIONS.length; v++) {
    db.exec("BEGIN");
    try {
      db.exec(MIGRATIONS[v]!);
      db.exec(`PRAGMA user_version = ${v + 1}`);
      db.exec("COMMIT");
    } catch (e) {
      db.exec("ROLLBACK");
      throw e;
    }
  }
}

interface ReportRow {
  thread_id: string;
  reporter_id: string;
  status: string;
  category: string | null;
  priority: string | null;
  acknowledged_by: string | null;
  issue_number: number | null;
}

export function getReport(db: DatabaseSync, threadId: string): Report | null {
  const row = db.prepare("SELECT * FROM reports WHERE thread_id = ?").get(threadId) as ReportRow | undefined;
  return row ? toReport(row) : null;
}

export function getReportByIssue(db: DatabaseSync, repo: string, issueNumber: number): Report | null {
  const row = db
    .prepare("SELECT * FROM reports WHERE github_repo = ? AND issue_number = ?")
    .get(repo, issueNumber) as ReportRow | undefined;
  return row ? toReport(row) : null;
}

/** Returns false if the thread is already known (threadCreate can fire more than once). */
export function insertReport(
  db: DatabaseSync,
  r: { threadId: string; channelId: string; reporterId: string; title: string; repo: string },
): boolean {
  const res = db
    .prepare(
      `INSERT INTO reports (thread_id, channel_id, reporter_id, title, github_repo)
       VALUES (?, ?, ?, ?, ?) ON CONFLICT (thread_id) DO NOTHING`,
    )
    .run(r.threadId, r.channelId, r.reporterId, r.title, r.repo);
  return res.changes === 1;
}

export function saveReport(db: DatabaseSync, r: Report): void {
  db.prepare(
    `UPDATE reports SET status = ?, category = ?, priority = ?, acknowledged_by = ?, issue_number = ?,
       updated_at = datetime('now'),
       closed_at = CASE WHEN ? IN ('resolved','not_planned','duplicate','rejected') THEN datetime('now') ELSE NULL END
     WHERE thread_id = ?`,
  ).run(r.status, r.category, r.priority, r.acknowledgedBy, r.issueNumber, r.status, r.threadId);
}

/** True the first time a delivery ID is seen. */
export function claimDelivery(db: DatabaseSync, deliveryId: string): boolean {
  return db.prepare("INSERT INTO deliveries (delivery_id) VALUES (?) ON CONFLICT DO NOTHING").run(deliveryId).changes === 1;
}

export function listQueue(db: DatabaseSync, limit = 15): { thread_id: string; title: string; status: Status; created_at: string }[] {
  return db
    .prepare(
      `SELECT thread_id, title, status, created_at FROM reports
       WHERE status IN ('new', 'acknowledged', 'needs_info')
       ORDER BY CASE status WHEN 'new' THEN 0 WHEN 'acknowledged' THEN 1 ELSE 2 END, created_at
       LIMIT ?`,
    )
    .all(limit) as { thread_id: string; title: string; status: Status; created_at: string }[];
}

function toReport(row: ReportRow): Report {
  return {
    threadId: row.thread_id,
    reporterId: row.reporter_id,
    status: row.status as Status,
    category: row.category as Category | null,
    priority: row.priority as Priority | null,
    acknowledgedBy: row.acknowledged_by,
    issueNumber: row.issue_number,
  };
}
