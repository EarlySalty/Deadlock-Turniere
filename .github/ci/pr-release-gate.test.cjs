"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

// Den tatsächlich ausgeführten Workflow prüfen, keine zweite Gate-Implementierung.
const workflow = fs.readFileSync(
  path.join(__dirname, "../workflows/pr-release-gate.yml"),
  "utf8",
);
const lines = workflow.split("\n");
const start = lines.findIndex((line) => /^\s+script: \|\s*$/.test(line));
assert.notEqual(start, -1, "Der Release-Workflow muss ein prüfbares Skript enthalten");
const indent = lines[start].match(/^ */)[0].length + 2;
const scriptLines = [];
for (const line of lines.slice(start + 1)) {
  if (line.trim() && line.match(/^ */)[0].length < indent) break;
  scriptLines.push(line.slice(indent));
}
const script = new vm.Script(`(async () => {\n${scriptLines.join("\n")}\n})()`, {
  filename: "pr-release-gate.yml",
});

const HEAD = "a".repeat(40);
const BASE = "b".repeat(40);
const REQUIRED_CHECKS = [
  "Semantic review",
  "Frontend CI",
  "Rust workspace and isolated PostgreSQL",
  "Secret Detection",
  "JavaScript Dependency Audit",
  "Semgrep SAST",
  "CodeQL Analysis (javascript-typescript)",
  "Trivy Filesystem Scan",
];

async function reconcile(change = () => {}) {
  const fixture = {
    pr: {
      number: 10,
      state: "open",
      draft: false,
      changed_files: 1,
      base: { ref: "main", sha: BASE },
      head: { ref: "feature", sha: HEAD, repo: { full_name: "test/repository" } },
      user: { login: "maintainer" },
      mergeable: true,
      mergeable_state: "clean",
    },
    files: [{ filename: "src/feature.rs", status: "modified" }],
    checks: REQUIRED_CHECKS.map((name) => ({
      name,
      head_sha: HEAD,
      status: "completed",
      conclusion: "success",
      app: { slug: "github-actions" },
    })),
    statuses: [],
    permission: "write",
    behindBy: 0,
    finalPr: null,
  };
  change(fixture);
  const merges = [];
  const updates = [];
  const notices = [];
  let reads = 0;
  const pulls = {
    list: async () => ({ data: [structuredClone(fixture.pr)] }),
    get: async () => ({
      data: structuredClone(++reads > 1 && fixture.finalPr ? fixture.finalPr : fixture.pr),
    }),
    listFiles: async () => ({ data: fixture.files }),
    updateBranch: async (request) => { updates.push(request); return { data: {} }; },
    merge: async (request) => { merges.push(request); return { data: { merged: true } }; },
  };
  const github = {
    paginate: async (method, args) => (await method(args)).data,
    request: async () => ({ data: { behind_by: fixture.behindBy } }),
    rest: {
      pulls,
      repos: {
        getCollaboratorPermissionLevel: async () => ({ data: { permission: fixture.permission } }),
        getCombinedStatusForRef: async () => ({ data: { statuses: fixture.statuses } }),
      },
      checks: { listForRef: async () => ({ data: { check_runs: fixture.checks } }) },
    },
  };
  // Nur In-Memory-API-Doubles: kein GitHub-Token, Netzwerk oder echter Merge.
  await script.runInNewContext({
    github,
    context: { repo: { owner: "test", repo: "repository" } },
    core: { notice: (message) => notices.push(message), warning: (message) => notices.push(message) },
    process: { env: { INPUT_PR_NUMBER: "" } },
  }, { timeout: 1000 });
  return { merges, updates, notices };
}

async function blocked(change) {
  const result = await reconcile(change);
  assert.equal(result.merges.length, 0, "Ohne vollständige Freigabe darf kein Merge angefordert werden");
  return result;
}

test("Aktueller Head mit vollständigen grünen Gates wird genau einmal SHA-gebunden gemergt", async () => {
  const result = await reconcile();
  assert.equal(result.merges.length, 1);
  assert.equal(result.merges[0].sha, HEAD);
  assert.equal(result.merges[0].pull_number, 10);
});

for (const name of REQUIRED_CHECKS) {
  test(`Fehlender Pflichtcheck blockiert: ${name}`, async () => {
    await blocked((f) => { f.checks = f.checks.filter((check) => check.name !== name); });
  });
  for (const conclusion of ["failure", "cancelled", "skipped", "neutral", "timed_out"]) {
    test(`Pflichtcheck ${name} mit ${conclusion} blockiert`, async () => {
      await blocked((f) => { f.checks.find((check) => check.name === name).conclusion = conclusion; });
    });
  }
}

test("Noch laufender Pflichtcheck blockiert", async () => {
  await blocked((f) => { f.checks[0].status = "in_progress"; f.checks[0].conclusion = null; });
});
test("Grüne Checks eines alten Heads blockieren", async () => {
  await blocked((f) => { f.checks.forEach((check) => { check.head_sha = "c".repeat(40); }); });
});
test("Gleichnamige Checks einer anderen App ersetzen keine Actions-Prüfungen", async () => {
  await blocked((f) => { f.checks.forEach((check) => { check.app.slug = "untrusted-app"; }); });
});
test("Ein fehlgeschlagener zusätzlicher Check blockiert", async () => {
  await blocked((f) => { f.checks.push({ ...f.checks[0], name: "Additional check", conclusion: "failure" }); });
});
test("Ein roter Commit-Status blockiert", async () => {
  await blocked((f) => { f.statuses = [{ context: "external", state: "failure", created_at: "2026-09-24T00:00:00Z" }]; });
});
test("Bewegte Basis aktualisiert nur den Branch und verlangt neue Gates", async () => {
  const result = await blocked((f) => { f.behindBy = 1; });
  assert.equal(result.updates.length, 1);
  assert.equal(result.updates[0].expected_head_sha, HEAD);
});
test("Während der Prüfung geänderter Head blockiert", async () => {
  await blocked((f) => { f.finalPr = structuredClone(f.pr); f.finalPr.head.sha = "c".repeat(40); });
});
test("Während der Prüfung geänderte Basis blockiert", async () => {
  await blocked((f) => { f.finalPr = structuredClone(f.pr); f.finalPr.base.sha = "c".repeat(40); });
});
test("Drafts blockieren", async () => {
  await blocked((f) => { f.pr.draft = true; });
});
test("Forks blockieren", async () => {
  await blocked((f) => { f.pr.head.repo.full_name = "outsider/repository"; });
});
test("Autoren ohne Schreibrecht blockieren", async () => {
  await blocked((f) => { f.permission = "read"; });
});

for (const protectedPath of [
  ".github/workflows/pr-release-gate.yml",
  "scripts/ci/check.sh",
  "rust/scripts/central_ci.sh",
]) {
  test(`Geänderte Policy braucht manuelle Freigabe: ${protectedPath}`, async () => {
    await blocked((f) => { f.files = [{ filename: protectedPath, status: "modified" }]; });
  });
  test(`Auch wegbenannte Policy braucht manuelle Freigabe: ${protectedPath}`, async () => {
    await blocked((f) => {
      f.files = [{ filename: "notes/retired-policy.txt", previous_filename: protectedPath, status: "renamed" }];
    });
  });
}
test("Unvollständige Umbenennungsdaten werden nicht automatisch freigegeben", async () => {
  await blocked((f) => { f.files = [{ filename: "src/renamed.rs", status: "renamed" }]; });
});
test("Gewöhnliche Umbenennung bleibt mit vollständigen Gates mergefähig", async () => {
  const result = await reconcile((f) => {
    f.files = [{ filename: "src/new.rs", previous_filename: "src/old.rs", status: "renamed" }];
  });
  assert.equal(result.merges.length, 1);
});
