#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const workspaceRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const phase = readOption("--phase");
const findings = [];

checkNoRuntimeCompatModules();

if (phase === "services") {
  checkServiceBoundary();
} else if (phase === "execution-split") {
  checkExecutionSplit();
} else if (phase === "projection-hot-paths") {
  checkProjectionHotPaths();
} else if (phase === "session-pooling") {
  checkSessionPooling();
} else if (phase !== undefined) {
  findings.push(`unknown runtime architecture phase '${phase}'`);
}

if (findings.length > 0) {
  console.error("Runtime architecture boundary check failed:");
  for (const finding of findings) {
    console.error(`- ${finding}`);
  }
  process.exit(1);
}

console.log(phase ? `Runtime architecture boundary check passed for ${phase}.` : "Runtime architecture boundary check passed.");

function readOption(name) {
  const index = process.argv.indexOf(name);
  if (index < 0) {
    return undefined;
  }
  const value = process.argv[index + 1];
  if (!value || value.startsWith("--")) {
    findings.push(`${name} requires a value`);
    return undefined;
  }
  return value;
}

function checkNoRuntimeCompatModules() {
  for (const filePath of rustFiles("crates/runx-runtime/src")) {
    const source = readFileSync(filePath, "utf8");
    const rel = relative(filePath);
    if (/\bmod\s+\w+_(?:legacy|compat)\b/u.test(source)) {
      findings.push(`${rel} declares a legacy/compat runtime module`);
    }
    if (/\b(?:LegacyExecutor|CompatExecutor)\b/u.test(source)) {
      findings.push(`${rel} declares legacy executor compatibility vocabulary`);
    }
  }
}

function checkServiceBoundary() {
  const roots = [
    "crates/runx-runtime/src/adapters",
    "crates/runx-runtime/src/execution",
  ];
  const forbidden = [
    /\bRuntimeReceiptSignatureConfig::from_env\b/u,
    /\bLocalReceiptStore::new\b/u,
    /\bresolve_receipt_path\s*\(/u,
    /\bprepare_process_sandbox\s*\(/u,
    /\bprepare_mcp_process_sandbox\s*\(/u,
    /\bstd::env::(?:var|vars)\s*\(/u,
  ];
  for (const root of roots) {
    for (const filePath of rustFiles(root)) {
      const source = readFileSync(filePath, "utf8");
      for (const pattern of forbidden) {
        if (pattern.test(source)) {
          findings.push(`${relative(filePath)} still constructs env, receipts, or sandbox state outside runtime services`);
        }
      }
    }
  }
}

function checkExecutionSplit() {
  const stepsPath = path.join(workspaceRoot, "crates/runx-runtime/src/execution/runner/steps.rs");
  if (!existsSync(stepsPath)) {
    return;
  }
  const source = readFileSync(stepsPath, "utf8");
  const forbidden = [
    /\bstep_receipt_with\b/u,
    /\bRuntimePaymentSupervisor\b/u,
    /\brequest_approval\b/u,
    /\bSkillAdapter::invoke\b/u,
    /\bresolve_inputs\b/u,
  ];
  for (const pattern of forbidden) {
    if (pattern.test(source)) {
      findings.push(`${relative(stepsPath)} still contains mixed runner responsibility token ${pattern}`);
    }
  }
}

function checkProjectionHotPaths() {
  const runtimeRoot = path.join(workspaceRoot, "crates/runx-runtime/src");
  const internerFound = rustFiles("crates/runx-runtime/src").some((filePath) => {
    const source = readFileSync(filePath, "utf8");
    return /\bstruct\s+\w*(?:Id)?Interner\b/u.test(source);
  });
  if (!internerFound) {
    findings.push(`${relative(runtimeRoot)} has no runtime-local id interner for hot execution/projection paths`);
  }

  const cloneBudget = new Map([
    ["crates/runx-runtime/src/execution/graph_index.rs", 8],
    ["crates/runx-runtime/src/output_projection.rs", 8],
  ]);
  for (const [relPath, maxClones] of cloneBudget) {
    const filePath = path.join(workspaceRoot, relPath);
    if (!existsSync(filePath)) {
      continue;
    }
    const count = countMatches(readFileSync(filePath, "utf8"), /\.clone\s*\(/gu);
    if (count > maxClones) {
      findings.push(`${relPath} has ${count} .clone() calls, above budget ${maxClones}`);
    }
  }
}

function checkSessionPooling() {
  for (const filePath of rustFiles("crates/runx-runtime/src")) {
    const source = readFileSync(filePath, "utf8");
    if (/\b(?:cli.*pool|pool.*cli|user command pool|pooled.*Command|CommandPool)\b/iu.test(source)) {
      findings.push(`${relative(filePath)} appears to pool arbitrary CLI/user commands`);
    }
  }
}

function rustFiles(root) {
  const absoluteRoot = path.join(workspaceRoot, root);
  if (!existsSync(absoluteRoot)) {
    return [];
  }
  return walk(absoluteRoot).filter((filePath) => filePath.endsWith(".rs"));
}

function walk(directory) {
  const entries = readdirSync(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    if (entry.name === "target") {
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

function countMatches(source, pattern) {
  return [...source.matchAll(pattern)].length;
}

function relative(filePath) {
  return path.relative(workspaceRoot, filePath);
}
