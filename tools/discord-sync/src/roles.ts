import type { Role } from "./triage.js";

export function roleOf(
  memberRoleIds: Iterable<string>,
  cfg: { DISCORD_TRIAGER_ROLE_IDS: string[]; DISCORD_MAINTAINER_ROLE_IDS: string[] },
): Role {
  const ids = new Set(memberRoleIds);
  if (cfg.DISCORD_MAINTAINER_ROLE_IDS.some((id) => ids.has(id))) return "maintainer";
  if (cfg.DISCORD_TRIAGER_ROLE_IDS.some((id) => ids.has(id))) return "triager";
  return "member";
}
