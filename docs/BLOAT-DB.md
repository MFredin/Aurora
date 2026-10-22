# Bloat database: schema and curation

File: `crates/forge-core/data/bloat.json`, embedded at build time. **Current content is seed data
(`"curated": false`).** Do not ship a release until entries are curated.

## Schema

```json
{ "id": "appx.bingnews", "kind": "appx" | "startup",
  "match": ["lowercase substrings"], "verdict": "safe" | "caution" | "keep",
  "reason": "one plain sentence shown to the user" }
```

- `kind: appx` matches the package name (e.g. `Microsoft.BingNews`). `kind: startup` matches `name + command` of a Run entry.
- Matching is case-insensitive substring. Keep patterns specific: a loose pattern is a false positive on someone's machine.
- A `keep` match always wins, whatever the order. Otherwise the first match wins; order entries from specific to general.
- The helper refuses to disable anything that matches a `keep` entry, by name and by the live command line.

## Verdicts

| Verdict | Meaning | Behaviour |
|---|---|---|
| `safe` | No system component depends on it; removing or disabling has no side effects for most users | Flagged, low risk, included in "Apply safe fixes" (startup items only; appx removal is never bulk-applied) |
| `caution` | Depends on the user's habits (chat apps, sync clients, work tools) | Flagged as "Check first", never bulk-applied |
| `keep` | Drivers, security components, or anything that breaks features if removed | Never flagged. Listed so we don't flag it by accident through a broad pattern |

## Curation process (per entry)

1. Identify the item on a clean Windows 11 install and on at least one OEM image (Dell, HP, Lenovo, ASUS).
2. In a VM snapshot, disable/remove it. Reboot. Check: sign-in, Start menu, Settings, Store, Xbox/Game Pass, Windows Update, audio, display, sleep/wake, any OEM hotkeys.
3. Confirm the reason sentence is true and understandable by a non-expert.
4. Record: Windows build tested, date, source (vendor docs / testing), then set `curated: true` for that entry (add the field to the schema when curating starts).
5. For `keep` entries, record what breaks and why.

## Things to add to the schema when curation starts

Per-entry `curated`, `tested_on` (build + date), `source`, `oem` (optional), `reinstall_hint`
(Store link or installer) for appx removals.

## Quality gates (CI, M5)

- A test that fails a release build if any entry has `curated: false`.
- ~~A test that no pattern is shorter than 4 characters and no two entries with different verdicts match the same sample string.~~ Done (`bloat_patterns_are_specific_and_unambiguous`).
- A test corpus of real startup/appx names from several OEM images to catch false positives.
