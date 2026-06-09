#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const workspaceRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const repoRoot = path.dirname(workspaceRoot);
const inventoryPath = path.join(workspaceRoot, "docs/runtime-cutover-inventory.json");
const args = process.argv.slice(2);
const finalMode = args.includes("--final");
const findings = [];

if (args[0] === "--fixture") {
  runFixture(args[1]);
} else if (args[0] === "--record-overlap") {
  recordOverlap(args[1]);
} else if (args[0] === "--check-overlap") {
  checkOverlap(args[1], args.includes("--require-resolved"));
} else if (args.includes("--check-tests-disposition")) {
  checkTestsDisposition();
} else if (args.includes("--check-external-adapter-session-policy")) {
  checkExternalAdapterSessionPolicy();
} else {
  runCutoverCheck();
}

if (findings.length > 0) {
  console.error("Runtime cutover legacy check failed:");
  for (const finding of findings) {
    console.error(`- ${finding}`);
  }
  process.exit(1);
}

console.log(finalMode ? "Runtime cutover final legacy check passed." : "Runtime cutover legacy check passed.");

function runFixture(name) {
  if (name !== "hidden-runtime-local-import") {
    findings.push(`unknown fixture '${name ?? ""}'`);
    return;
  }
  const fixtureFindings = [];
  checkImportText(
    "fixtures/hidden-runtime-local-import.ts",
    "import { runLocalSkill } from '@runxhq/runtime-local';\n",
    { final: true, inventory: emptyInventory(), findings: fixtureFindings },
  );
  if (fixtureFindings.length === 0) {
    findings.push("hidden runtime-local import fixture was not detected");
    return;
  }
  console.error("Fixture produced expected finding:");
  for (const finding of fixtureFindings) {
    console.error(`- ${finding}`);
  }
  process.exit(1);
}

function recordOverlap(taskId) {
  if (!taskId) {
    findings.push("--record-overlap requires a task id");
    return;
  }
  const status = scafldStatus(taskId);
  if (!status) {
    return;
  }
  const inventory = readInventory();
  const recorded = inventory?.coordination?.overlap_tasks?.[taskId];
  if (!recorded) {
    findings.push(`runtime-cutover-inventory.json is missing coordination.overlap_tasks.${taskId}`);
    return;
  }
  process.stdout.write(`${JSON.stringify({
    task_id: taskId,
    current_status: status.status,
    recorded_status: recorded.status_at_phase1 ?? recorded.status,
  }, null, 2)}\n`);
}

function checkOverlap(taskId, requireResolved) {
  if (!taskId) {
    findings.push("--check-overlap requires a task id");
    return;
  }
  const status = scafldStatus(taskId);
  if (!status || !requireResolved) {
    return;
  }
  const activeStatuses = new Set(["active", "approved", "draft"]);
  if (activeStatuses.has(status.status)) {
    findings.push(`${taskId} is still ${status.status}; finish, cancel, or explicitly supersede it before overlapping Rust edits`);
  }
}

function checkTestsDisposition() {
  const inventory = readInventory();
  const disposition = inventory?.tests_disposition ?? {};
  const missing = importingTestFiles().filter((filePath) => !disposition[filePath]);
  for (const filePath of missing) {
    findings.push(`${filePath} imports a sunset package but has no tests_disposition entry`);
  }
}

function checkExternalAdapterSessionPolicy() {
  const sourceHits = rustSourceContains(/\b(?:ExternalAdapterSessionPool|external_adapter_session_reuse)\b/u);
  const inventory = readInventory();
  const policy = inventory?.session_policy?.external_adapter;
  if (sourceHits && policy?.status !== "reset_proven") {
    findings.push("external adapter session reuse appears in source without reset_proven inventory policy");
  }
  if (!sourceHits && policy?.status !== "one_shot_until_reset_protocol") {
    findings.push("external adapter session policy must explicitly record one_shot_until_reset_protocol while no reset-proven reuse exists");
  }
}

function runCutoverCheck() {
  const inventory = readInventory();
  checkInventoryShape(inventory);
  checkPackageManifests(inventory);
  checkWorkspaceFiles(inventory);
  checkSourceImports(inventory);
  checkRuntimeCompatModules();
  checkEffectKernelPhase2NoDualPath();
  if (finalMode) {
    checkFinalPackageDirectories();
    checkFinalRustKernelDomainFree();
    checkFinalRetiredWireNames();
    checkFinalRuntimeCargoEdges();
    checkFinalNoInKernelGithubProviderClients();
  }
}

function checkInventoryShape(inventory) {
  if (!inventory || inventory.schema !== "runx.runtime_cutover_inventory.v1") {
    findings.push("docs/runtime-cutover-inventory.json must use schema runx.runtime_cutover_inventory.v1");
    return;
  }
  for (const name of ["@runxhq/runtime-local", "@runxhq/adapters"]) {
    if (!inventory.packages?.some((entry) => entry.name === name)) {
      findings.push(`runtime-cutover-inventory.json is missing package entry ${name}`);
    }
    const npmDisposition = inventory.npm_disposition?.[name];
    if (!npmDisposition?.final_published_name || !npmDisposition?.deprecate_message || !npmDisposition?.migration_doc || !npmDisposition?.sunset_version) {
      findings.push(`runtime-cutover-inventory.json is missing complete npm_disposition for ${name}`);
    }
  }
}

function checkPackageManifests(inventory) {
  for (const manifestPath of findFiles(workspaceRoot, "package.json")) {
    if (manifestPath.includes(`${path.sep}node_modules${path.sep}`)) {
      continue;
    }
    const rel = relative(manifestPath);
    const manifest = readJson(manifestPath);
    if (isSunsetPackageName(manifest.name) && !isInventoryAllowedPackage(inventory, manifest.name) || (finalMode && isSunsetPackageName(manifest.name))) {
      findings.push(`${rel} keeps sunset package name ${manifest.name}`);
    }
    for (const field of ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]) {
      const deps = manifest[field] ?? {};
      for (const dependencyName of Object.keys(deps)) {
        if (!isSunsetPackageName(dependencyName)) {
          continue;
        }
        if (finalMode || !isInventoryAllowedPackage(inventory, dependencyName)) {
          findings.push(`${rel} declares sunset dependency ${dependencyName} in ${field}`);
        }
      }
    }
  }
}

function checkWorkspaceFiles(inventory) {
  const files = [
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "tsconfig.base.json",
    "vitest.workspace-aliases.ts",
    "docs/api-surface.md",
    "docs/ts-interop-boundary.md",
  ];
  for (const relPath of files) {
    const absolutePath = path.join(workspaceRoot, relPath);
    if (!existsSync(absolutePath)) {
      continue;
    }
    const source = readFileSync(absolutePath, "utf8");
    for (const token of ["@runxhq/runtime-local", "@runxhq/adapters", "packages/runtime-local", "packages/adapters"]) {
      if (!source.includes(token)) {
        continue;
      }
      if (finalMode || !isInventoryAllowedToken(inventory, relPath, token)) {
        findings.push(`${relPath} still references sunset executor token ${token}`);
      }
    }
    if (/temporary fallback/iu.test(source)) {
      findings.push(`${relPath} contains temporary fallback cutover language`);
    }
  }
}

function checkSourceImports(inventory) {
  for (const filePath of sourceFiles(["packages", "tests", "scripts"])) {
    const rel = relative(filePath);
    if (rel === "scripts/check-runtime-cutover-legacy.mjs") {
      continue;
    }
    const source = readFileSync(filePath, "utf8");
    checkImportText(rel, source, { final: finalMode, inventory, findings });
  }
}

function checkImportText(rel, source, context) {
  const sunsetImport = /(?:from\s+["']|import\s*\(\s*["']|require\s*\(\s*["'])(@runxhq\/(?:runtime-local|adapters)(?:\/[^"']*)?)/gu;
  for (const match of source.matchAll(sunsetImport)) {
    const specifier = match[1];
    if (context.final || !isInventoryAllowedImport(context.inventory, rel, specifier)) {
      context.findings.push(`${rel} imports sunset executor package ${specifier}`);
    }
  }
  const relativeInternal = /(?:from\s+["']|import\s*\(\s*["']|require\s*\(\s*["'])([^"']*packages\/(?:runtime-local|adapters)\/src[^"']*)/gu;
  for (const match of source.matchAll(relativeInternal)) {
    if (context.final || !isInventoryAllowedImport(context.inventory, rel, match[1])) {
      context.findings.push(`${rel} imports sunset package internals through ${match[1]}`);
    }
  }
}

function checkRuntimeCompatModules() {
  for (const filePath of sourceFiles(["crates/runx-runtime/src"], [".rs"])) {
    const rel = relative(filePath);
    const source = readFileSync(filePath, "utf8");
    if (/\bmod\s+\w+_(?:legacy|compat)\b/u.test(source)) {
      findings.push(`${rel} declares a legacy/compat runtime module`);
    }
    if (/\b(?:LegacyExecutor|CompatExecutor)\b/u.test(source)) {
      findings.push(`${rel} declares legacy executor compatibility vocabulary`);
    }
  }
}

function checkEffectKernelPhase2NoDualPath() {
  const runnerFiles = sourceFiles(["crates/runx-runtime/src/execution/runner"], [".rs"]);
  const runnerRoot = path.join(workspaceRoot, "crates/runx-runtime/src/execution/runner.rs");
  if (existsSync(runnerRoot)) {
    runnerFiles.push(runnerRoot);
  }
  const retiredSnake = /\bpayment_supervisor\b/u;
  const retiredStateImport = /\b(?:crate|runx_runtime)::payment::state\b/u;
  const paymentModuleImport = /\b(?:use\s+)?crate::payment::/u;
  for (const filePath of runnerFiles) {
    const rel = relative(filePath);
    const source = readFileSync(filePath, "utf8");
    if (retiredSnake.test(source)) {
      findings.push(`${rel} still references retired payment supervisor orchestration symbols`);
    }
    if (retiredStateImport.test(source)) {
      findings.push(`${rel} imports retired payment state instead of effect state`);
    }
    if (paymentModuleImport.test(source)) {
      findings.push(`${rel} imports payment modules directly instead of the effect registry boundary`);
    }
  }
}

function checkFinalPackageDirectories() {
  for (const relPath of ["packages/core", "packages/runtime-local", "packages/adapters"]) {
    if (existsSync(path.join(workspaceRoot, relPath))) {
      findings.push(`${relPath} remains in final cutover mode`);
    }
  }
}

function checkFinalRustKernelDomainFree() {
  const sourceRoots = ["crates/runx-runtime/src", "crates/runx-core/src", "crates/runx-contracts/src"];
  const manifestFiles = ["crates/runx-core/Cargo.toml", "crates/runx-contracts/Cargo.toml"];
  const files = [
    ...sourceFiles(sourceRoots, [".rs"]),
    ...manifestFiles.map((relPath) => path.join(workspaceRoot, relPath)).filter(existsSync),
  ];
  const bannedParts = new Set(["payment", "settlement", "spend", "x402", "rail"]);
  for (const filePath of files) {
    const rel = relative(filePath);
    const lines = readFileSync(filePath, "utf8").split(/\r?\n/u);
    lines.forEach((line, index) => {
      for (const token of line.matchAll(/[A-Za-z_][A-Za-z0-9_]*/gu)) {
        const parts = splitIdentifierParts(token[0]);
        const banned = parts.find((part) => bannedParts.has(part));
        if (banned) {
          findings.push(`${rel}:${index + 1} contains final-cutover domain token '${banned}' in '${token[0]}'`);
        }
      }
    });
  }
}

function checkFinalRuntimeCargoEdges() {
  const result = spawnSync("cargo", [
    "tree",
    "--manifest-path",
    "crates/Cargo.toml",
    "-p",
    "runx-runtime",
    "--edges",
    "normal",
  ], {
    cwd: workspaceRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    findings.push(`cargo tree failed for runx-runtime: ${result.stderr || result.stdout}`);
    return;
  }
  if (/\brunx-pay\b|\brunx_pay\b/u.test(result.stdout)) {
    findings.push("runx-runtime has a normal Cargo edge to runx-pay in final cutover mode");
  }
}

function checkFinalNoInKernelGithubProviderClients() {
  const retiredProviderLanePaths = [
    "crates/runx-runtime/src/execution/target_runner.rs",
    "crates/runx-runtime/src/execution/target_runner",
    "crates/runx-runtime/src/post_merge_observer.rs",
    "crates/runx-runtime/src/post_merge_observer",
    "crates/runx-contracts/src/target_runner.rs",
    "crates/runx-contracts/src/target_runner",
    "crates/runx-contracts/src/post_merge_observer.rs",
    "crates/runx-contracts/src/post_merge_observer",
  ];
  for (const relPath of retiredProviderLanePaths) {
    if (existsSync(path.join(workspaceRoot, relPath))) {
      findings.push(`${relPath} reintroduces the retired provider orchestration lane`);
    }
  }
  const providerClientMarkers = [
    /\breqwest\b/u,
    /\bapi\.github\.com\b/u,
    /\bGITHUB_TOKEN\b/u,
    /\bbearer_auth\b/u,
  ];
  const files = [
    ...sourceFiles(["crates/runx-runtime/src/adapters", "crates/runx-runtime/src/outbox_provider"], [".rs"]),
  ].filter(existsSync);
  for (const filePath of files) {
    const rel = relative(filePath);
    const source = readFileSync(filePath, "utf8");
    const marker = providerClientMarkers.find((pattern) => pattern.test(source));
    if (marker) {
      findings.push(`${rel} contains outbound GitHub provider client marker ${marker}`);
    }
  }
}

function checkFinalRetiredWireNames() {
  const scanFiles = sourceFiles(
    [
      "crates/runx-contracts/src",
      "crates/runx-contracts/tests",
      "packages/contracts/src",
      "schemas",
      "fixtures",
      "skills",
      "examples",
      "scripts",
      "docs",
    ],
    [".rs", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".json", ".yaml", ".yml", ".md"],
  ).filter((filePath) => relative(filePath) !== "scripts/check-runtime-cutover-legacy.mjs");
  const patterns = retiredWirePatterns();
  for (const filePath of scanFiles) {
    const rel = relative(filePath);
    const source = readFileSync(filePath, "utf8");
    for (const pattern of patterns) {
      if (pattern.test(source)) {
        findings.push(`${rel} contains retired generic-contract wire name ${pattern}`);
      }
    }
  }
}

function retiredWirePatterns() {
  const literal = (parts) => new RegExp(parts.join(""), "u");
  return [
    literal(["Payment", "Authority", "Bounds"]),
    literal(["Payment", "Credential", "Form"]),
    /\bbounds\.payment\b/u,
    literal(["max_", "spend_", "usd"]),
    literal(["max_", "per_", "call_", "minor"]),
    literal(["max_", "per_", "run_", "minor"]),
    literal(["max_", "per_", "period_", "minor"]),
    literal(["payment_", "single_", "use_", "spend"]),
    literal(["single_", "use_", "spend_", "capability"]),
    literal(["ProofKind", "::", "Payment", "Rail"]),
    literal(['"', "payment_", "rail", '"']),
    /\bpayment_rail\b/u,
    literal(["Effect", "Settlement", "Receipt"]),
    literal(["\\b", "effect", "_", "settlement", "\\b"]),
    literal(["\\b", "effect", "-", "settlement", "\\b"]),
    /\bpayment_required\b/u,
    literal(["payment_", "rail_", "packet"]),
    literal(["runx", "\\.", "payment", "\\.", "rail", "\\.", "v1"]),
    /\bquote_required\b/u,
    /\breservation_required\b/u,
    /\bcredential_form\b/u,
    /\bsingle_use_spend\b/u,
    /resource_family:\s*payment/u,
    literal(['"', "resource_family", '"', "\\s*:\\s*", '"', "payment", '"']),
  ];
}

function splitIdentifierParts(token) {
  return token
    .replace(/([a-z0-9])([A-Z])/gu, "$1_$2")
    .replace(/([A-Z]+)([A-Z][a-z])/gu, "$1_$2")
    .toLowerCase()
    .split(/_+/u)
    .filter(Boolean);
}

function importingTestFiles() {
  return sourceFiles(["tests"]).filter((filePath) => {
    const source = readFileSync(filePath, "utf8");
    return /@runxhq\/(?:runtime-local|adapters)\b|packages\/(?:runtime-local|adapters)\/src/u.test(source);
  }).map(relative);
}

function scafldStatus(taskId) {
  const result = spawnSync("scafld", ["--root", workspaceRoot, "status", taskId, "--json"], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    findings.push(`scafld status failed for ${taskId}: ${result.stderr || result.stdout}`);
    return undefined;
  }
  try {
    return JSON.parse(result.stdout).result;
  } catch {
    findings.push(`could not parse scafld status JSON for ${taskId}`);
    return undefined;
  }
}

function readInventory() {
  if (!existsSync(inventoryPath)) {
    findings.push("docs/runtime-cutover-inventory.json is missing");
    return emptyInventory();
  }
  return readJson(inventoryPath);
}

function emptyInventory() {
  return {
    packages: [],
    npm_disposition: {},
    tests_disposition: {},
    legacy_allowlist: [],
  };
}

function isInventoryAllowedPackage(inventory, name) {
  return inventory?.packages?.some((entry) => entry.name === name && entry.disposition === "sunset");
}

function isInventoryAllowedToken(inventory, relPath, token) {
  return inventory?.legacy_allowlist?.some((entry) => {
    if (!entry.token || !entry.paths) {
      return false;
    }
    return token.startsWith(entry.token) || entry.token.startsWith(token)
      ? entry.paths.some((allowedPath) => relPath === allowedPath || relPath.startsWith(`${allowedPath}/`))
      : false;
  });
}

function isInventoryAllowedImport(inventory, relPath, specifier) {
  if (relPath.startsWith("packages/runtime-local/") || relPath.startsWith("packages/adapters/")) {
    return true;
  }
  if (relPath.startsWith("tests/")) {
    return Boolean(inventory?.tests_disposition?.[relPath]);
  }
  return inventory?.legacy_allowlist?.some((entry) => specifier.startsWith(entry.token) && entry.paths?.some((allowedPath) => relPath === allowedPath || relPath.startsWith(`${allowedPath}/`)));
}

function isSunsetPackageName(name) {
  return name === "@runxhq/runtime-local" || name === "@runxhq/adapters";
}

function sourceFiles(roots, extensions = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"]) {
  const files = [];
  for (const root of roots) {
    const absoluteRoot = path.join(workspaceRoot, root);
    if (!existsSync(absoluteRoot)) {
      continue;
    }
    for (const filePath of walk(absoluteRoot)) {
      if (extensions.includes(path.extname(filePath))) {
        files.push(filePath);
      }
    }
  }
  return files;
}

function findFiles(root, fileName) {
  return walk(root).filter((filePath) => path.basename(filePath) === fileName);
}

function walk(directory) {
  const entries = readdirSync(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    if (["node_modules", "dist", ".build", "coverage", "target"].includes(entry.name)) {
      continue;
    }
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...walk(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files;
}

function rustSourceContains(pattern) {
  return sourceFiles(["crates/runx-runtime/src"], [".rs"]).some((filePath) => pattern.test(readFileSync(filePath, "utf8")));
}

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, "utf8"));
}

function relative(filePath) {
  return path.relative(workspaceRoot, filePath);
}
