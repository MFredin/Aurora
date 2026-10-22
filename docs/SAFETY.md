# Safety model and review checklist

Forge changes system settings on people's machines. Trust is the product. This file is the bar every
change that touches the system must clear.

## Threat model (short)

| Threat | Mitigation |
|---|---|
| Compromised or buggy webview asks for an arbitrary change | UI sends only finding ids. The shell resolves the plan from the last scan. Tauri capabilities expose only our own commands (no fs/shell/http plugins). |
| Another local process talks to the helper | Production transport is a named pipe whose ACL admits only the installing user's SID. (Dev stdio transport is not for release.) |
| A rule or DB entry proposes something dangerous | The helper re-validates every step against `ipc::validate`, independent of who produced it. |
| A change breaks the system | Prior state is journaled; reversible steps have an inverse; restore point before first apply (M2). |
| Bad DB entry removes something needed | `keep` verdict for drivers/security components; `curated: false` data cannot ship in a release build (M5). |
| Forge itself looks like malware (AV false positives) | No process injection, no service installs, no obfuscation, signed binaries, stable file paths. |

## Rules for adding a `FixStep`

1. It is a new enum variant with typed fields. No free-form command strings.
2. `ipc::validate` has an allowlist entry that is as narrow as possible (exact key + value name, known GUIDs, strict name checks).
3. The executor records a `Prior` capturing what was there before, so an inverse exists, or the plan is marked `reversible: false` with user-facing wording.
4. Tests: allowed case, rejected case, journal round trip, and inverse.
5. It is not on the never list below.

## Never list

- Anything that disables, excludes, or modifies Windows Defender, SmartScreen, Firewall, UAC, Windows Update, or other security controls.
- Deleting system files, touching the boot configuration, BCD, or firmware settings.
- Registry cleaning or bulk registry edits.
- Services: no disabling services in v0.x. (Revisit with a curated list and a strong reason.)
- Installing drivers or software on the user's behalf.
- Telemetry that leaves the machine without explicit opt-in.

## Review checklist for any PR that touches the system

- [ ] The UI can't cause this step to run with arguments it chose.
- [ ] Allowlist entry is exact and has a rejection test.
- [ ] Prior state captured; undo tested, or "cannot be undone" is shown before the click.
- [ ] Safe on laptops, on battery, on a domain-joined or policy-managed PC (fails cleanly, doesn't half-apply).
- [ ] Failure leaves the journal and the system consistent; UI shows the real outcome.
- [ ] Nothing runs elevated that doesn't have to.
- [ ] Manually verified on a Windows 11 VM, with a snapshot to roll back.

## Testing destructive changes

Use a Windows 11 VM with snapshots. Never develop `RemoveAppx` or registry writes against your daily
machine. Keep a snapshot named "clean" and one per milestone.
