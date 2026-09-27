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

test("Audit hat nur Leserechte und keinen Schreibpfad", () => {
  assert.match(workflow, /^  contents: read$/m);
  assert.match(workflow, /^  pull-requests: read$/m);
  assert.doesNotMatch(workflow, /github\.rest\.pulls\.(?:merge|updateBranch|update)\s*\(/);
  assert.doesNotMatch(workflow, /github\.rest\.git\.(?:updateRef|createCommit)\s*\(/);
  assert.doesNotMatch(workflow, /github\.graphql\s*\(/);
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
    suites: [{ id: 1 }],
    statuses: [],
    permission: "write",
    behindBy: 0,
    mainBefore: BASE,
    mainAfter: BASE,
    finalPr: null,
  };
  change(fixture);
  const notices = [];
  let reads = 0;
  let mainReads = 0;
  const pulls = {
    list: async () => ({ data: [structuredClone(fixture.pr)] }),
    get: async () => ({
      data: structuredClone(++reads > 1 && fixture.finalPr ? fixture.finalPr : fixture.pr),
    }),
    listFiles: async ({ page, per_page }) => ({
      data: fixture.files.slice((page - 1) * per_page, page * per_page),
    }),
    updateBranch: async () => { throw new Error("read-only audit attempted branch update"); },
    merge: async () => { throw new Error("read-only audit attempted merge"); },
  };
  const github = {
    paginate: async (method, args) => {
      const results = [];
      for (let page = 1; ; page++) {
        const response = await method({ ...args, page });
        const items = Array.isArray(response.data) ? response.data
          : response.data.check_runs || response.data.check_suites;
        results.push(...items);
        if (items.length < args.per_page) return results;
      }
    },
    request: async () => ({ data: { behind_by: fixture.behindBy } }),
    rest: {
      pulls,
      repos: {
        getBranchProtection: async () => { throw new Error("read-only audit queried merge protection"); },
        getCollaboratorPermissionLevel: async () => ({ data: { permission: fixture.permission } }),
        listCommitStatusesForRef: async ({ page, per_page }) => ({
          data: fixture.statuses.slice((page - 1) * per_page, page * per_page),
        }),
      },
      git: {
        getRef: async () => ({ data: { object: { sha: ++mainReads > 1
          ? fixture.mainAfter : fixture.mainBefore } } }),
      },
      checks: {
        listSuitesForRef: async ({ page, per_page }) => ({ data: {
          check_suites: fixture.suites.slice((page - 1) * per_page, page * per_page),
        } }),
        listForRef: async ({ page, per_page }) => ({ data: {
          check_runs: fixture.checks.slice((page - 1) * per_page, page * per_page),
        } }),
      },
    },
  };
  // Nur In-Memory-API-Doubles: kein GitHub-Token, Netzwerk oder echter Merge.
  await script.runInNewContext({
    github,
    context: { repo: { owner: "test", repo: "repository" } },
    core: { notice: (message) => notices.push(message), warning: (message) => notices.push(message) },
  }, { timeout: 1000 });
  return { notices, ready: notices.some((notice) => notice.includes("independent manual review and merge are required")) };
}

async function blocked(change) {
  const result = await reconcile(change);
  assert.equal(result.ready, false, "Unvollständige Gates dürfen keine manuelle Bereitschaft melden");
  return result;
}

test("Aktueller Head mit vollständigen grünen Gates wird nur für manuelle Abnahme gemeldet", async () => {
  const result = await reconcile();
  assert.equal(result.ready, true);
  assert.match(result.notices.at(-1), new RegExp(HEAD));
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
test("Ein fehlgeschlagener Check auf Seite 2 blockiert", async () => {
  await blocked((f) => {
    f.checks.push(...Array.from({ length: 92 }, (_, index) => ({
      ...f.checks[0], name: `Additional green ${index}`,
    })));
    f.checks.push({ ...f.checks[0], name: "Hidden failure", conclusion: "failure" });
  });
});
test("1000 Check-Suites blockieren trotz sichtbarer grüner Runs", async () => {
  await blocked((f) => {
    f.suites = Array.from({ length: 1000 }, (_, index) => ({ id: index + 1 }));
  });
});
test("999 Check-Suites bleiben vollständig auswertbar", async () => {
  const result = await reconcile((f) => {
    f.suites = Array.from({ length: 999 }, (_, index) => ({ id: index + 1 }));
  });
  assert.equal(result.ready, true);
});
test("Ein roter Commit-Status blockiert", async () => {
  await blocked((f) => { f.statuses = [{ context: "external", state: "failure", created_at: "2026-09-24T00:00:00Z" }]; });
});
test("Ein roter Commit-Status auf Seite 2 blockiert", async () => {
  await blocked((f) => {
    f.statuses = Array.from({ length: 100 }, (_, index) => ({
      context: `green-${index}`, state: "success", created_at: "2026-09-24T00:00:00Z",
    }));
    f.statuses.push({ context: "hidden-failure", state: "failure", created_at: "2026-09-24T00:00:00Z" });
  });
});
test("Aktueller Status derselben Context nach Seite 1 gewinnt", async () => {
  await blocked((f) => {
    f.statuses = Array.from({ length: 100 }, (_, index) => ({
      context: `green-${index}`, state: "success", created_at: "2026-09-24T00:00:00Z",
    }));
    f.statuses.push({ context: "green-0", state: "failure", created_at: "2026-09-24T00:01:00Z" });
  });
});
test("Bewegte Basis verlangt manuelles Branch-Update und neue Gates", async () => {
  const result = await blocked((f) => { f.behindBy = 1; });
  assert.match(result.notices.at(-1), /update it and rerun all checks/);
});
test("Während der Prüfung geänderter Head blockiert", async () => {
  await blocked((f) => { f.finalPr = structuredClone(f.pr); f.finalPr.head.sha = "c".repeat(40); });
});
test("Während der Prüfung geänderte Basis blockiert", async () => {
  await blocked((f) => { f.finalPr = structuredClone(f.pr); f.finalPr.base.sha = "c".repeat(40); });
});
test("Vor der Prüfung verschobener main-Ref blockiert", async () => {
  await blocked((f) => { f.mainBefore = "c".repeat(40); });
});
test("Nach der Prüfung verschobener main-Ref blockiert auch bei unveränderter PR-Antwort", async () => {
  await blocked((f) => { f.mainAfter = "c".repeat(40); });
});
test("Während der Prüfung geschlossener PR bekommt keine Bereitschaftsmeldung", async () => {
  await blocked((f) => { f.finalPr = structuredClone(f.pr); f.finalPr.state = "closed"; });
});
test("Während der Prüfung zum Draft gewordener PR bekommt keine Bereitschaftsmeldung", async () => {
  await blocked((f) => { f.finalPr = structuredClone(f.pr); f.finalPr.draft = true; });
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
test("Unvollständige Dateiliste blockiert ohne sichtbare Policy-Datei", async () => {
  await blocked((f) => { f.pr.changed_files = 2; });
});
test("GitHubs 3000-Dateien-Kappung blockiert PR mit 3001 Dateien", async () => {
  await blocked((f) => {
    f.pr.changed_files = 3001;
    f.files = Array.from({ length: 3000 }, (_, index) => ({
      filename: `src/file-${index}.rs`, status: "modified",
    }));
  });
});
test("Fehlende Gesamtzahl geänderter Dateien blockiert", async () => {
  await blocked((f) => { delete f.pr.changed_files; });
});
test("Gewöhnliche Umbenennung bleibt mit vollständigen Gates prüfbereit", async () => {
  const result = await reconcile((f) => {
    f.files = [{ filename: "src/new.rs", previous_filename: "src/old.rs", status: "renamed" }];
  });
  assert.equal(result.ready, true);
});
