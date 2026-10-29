import { App } from "@octokit/app";
import type { Config } from "../config.js";

export type GitHub = Awaited<ReturnType<typeof createGitHub>>;

const LABEL_COLOURS: Record<string, string> = {
  discord: "5865F2",
  "priority: p0": "B60205",
  "priority: p1": "D93F0B",
  "priority: p2": "FBCA04",
  "priority: p3": "C2E0C6",
};

export async function createGitHub(cfg: Config) {
  const app = new App({ appId: cfg.GITHUB_APP_ID, privateKey: cfg.GITHUB_APP_PRIVATE_KEY });
  const octokit = await app.getInstallationOctokit(cfg.GITHUB_INSTALLATION_ID);
  const [owner, repo] = cfg.GITHUB_REPO.split("/") as [string, string];
  const known = new Set<string>();

  async function ensureLabels(names: string[]): Promise<void> {
    for (const name of names) {
      if (known.has(name)) continue;
      try {
        await octokit.request("POST /repos/{owner}/{repo}/labels", {
          owner,
          repo,
          name,
          color: LABEL_COLOURS[name] ?? "EDEDED",
        });
      } catch (e) {
        if ((e as { status?: number }).status !== 422) throw e; // 422 = already exists
      }
      known.add(name);
    }
  }

  return {
    ensureLabels,

    async createIssue(title: string, body: string, labels: string[]): Promise<number> {
      await ensureLabels(labels);
      const res = await octokit.request("POST /repos/{owner}/{repo}/issues", { owner, repo, title, body, labels });
      return res.data.number;
    },

    async setLabels(issueNumber: number, keep: (name: string) => boolean, add: string[]): Promise<void> {
      await ensureLabels(add);
      const res = await octokit.request("GET /repos/{owner}/{repo}/issues/{issue_number}/labels", {
        owner,
        repo,
        issue_number: issueNumber,
      });
      const labels = [...new Set([...res.data.map((l) => l.name).filter(keep), ...add])];
      await octokit.request("PUT /repos/{owner}/{repo}/issues/{issue_number}/labels", {
        owner,
        repo,
        issue_number: issueNumber,
        labels,
      });
    },

    async comment(issueNumber: number, body: string): Promise<number> {
      const res = await octokit.request("POST /repos/{owner}/{repo}/issues/{issue_number}/comments", {
        owner,
        repo,
        issue_number: issueNumber,
        body,
      });
      return Number(res.data.id);
    },

    /**
     * Which issue a duplicate points at. Not in the webhook payload; only the GraphQL
     * timeline has it (MarkedAsDuplicateEvent.canonical).
     */
    async duplicateTarget(issueNumber: number): Promise<number | null> {
      const res = await octokit.graphql<{
        repository: { issue: { timelineItems: { nodes: { canonical: { number: number } | null }[] } } };
      }>(
        `query($owner: String!, $repo: String!, $n: Int!) {
          repository(owner: $owner, name: $repo) {
            issue(number: $n) {
              timelineItems(last: 5, itemTypes: [MARKED_AS_DUPLICATE_EVENT]) {
                nodes { ... on MarkedAsDuplicateEvent { canonical { ... on Issue { number } } } }
              }
            }
          }
        }`,
        { owner, repo, n: issueNumber },
      );
      const nodes = res.repository.issue.timelineItems.nodes;
      return nodes.at(-1)?.canonical?.number ?? null;
    },

    issueUrl(n: number): string {
      return `https://github.com/${owner}/${repo}/issues/${n}`;
    },
  };
}
