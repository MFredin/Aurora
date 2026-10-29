// Forum tags are the visible triage state, so the thread needs no status chatter.
// Discord allows 20 tags per forum and 5 applied per post; we use 18 and apply at most 3.

import type { Category, Priority, Report, Status } from "./triage.js";

export const STATUS_TAGS: Record<Status, string> = {
  new: "New",
  acknowledged: "Acknowledged",
  needs_info: "Needs info",
  synced: "Tracked",
  resolved: "Resolved",
  not_planned: "Won't fix",
  duplicate: "Duplicate",
  rejected: "Closed",
};

export const CATEGORY_TAGS: Record<Category, string> = {
  crash: "Crash",
  bug: "Bug",
  performance: "Performance",
  ui: "UI",
  install: "Install",
  feature: "Feature request",
};

export const PRIORITY_TAGS: Record<Priority, string> = {
  p0: "P0 critical",
  p1: "P1 high",
  p2: "P2 normal",
  p3: "P3 low",
};

export const ALL_TAG_NAMES: string[] = [
  ...Object.values(STATUS_TAGS),
  ...Object.values(CATEGORY_TAGS),
  ...Object.values(PRIORITY_TAGS),
];

export const MAX_FORUM_TAGS = 20;
export const MAX_APPLIED_TAGS = 5;

export function tagNamesFor(r: Report): string[] {
  const names = [STATUS_TAGS[r.status]];
  if (r.category) names.push(CATEGORY_TAGS[r.category]);
  if (r.priority) names.push(PRIORITY_TAGS[r.priority]);
  return names;
}

/** GitHub labels for a tracked report. `discord` marks provenance. */
export function labelsFor(r: Report): string[] {
  const labels = ["discord"];
  if (r.category) labels.push(`type: ${r.category}`);
  if (r.priority) labels.push(`priority: ${r.priority}`);
  return labels;
}

/**
 * Keep tags the bot doesn't own (moderators may add their own) and swap ours.
 * `tagIdsByName` comes from the forum's `availableTags`.
 */
export function nextAppliedTags(
  current: string[],
  r: Report,
  tagIdsByName: Map<string, string>,
): string[] {
  const ours = new Set(ALL_TAG_NAMES.map((n) => tagIdsByName.get(n)).filter((id): id is string => !!id));
  const foreign = current.filter((id) => !ours.has(id));
  const wanted = tagNamesFor(r)
    .map((n) => tagIdsByName.get(n))
    .filter((id): id is string => !!id);
  return [...wanted, ...foreign].slice(0, MAX_APPLIED_TAGS);
}
