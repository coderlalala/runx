#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const workspaceRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const scanRoots = [
  "crates/runx-receipts/src",
  "crates/runx-runtime/src/receipts",
];
const findings = [];

for (const root of scanRoots) {
  const absoluteRoot = path.join(workspaceRoot, root);
  if (!existsSync(absoluteRoot)) {
    continue;
  }
  for (const filePath of walk(absoluteRoot)) {
    if (!filePath.endsWith(".rs") || isTestOnlyPath(filePath)) {
      continue;
    }
    const source = stripTestOnlyModules(readFileSync(filePath, "utf8"));
    if (/\bserde_json::to_value\s*\(/u.test(source) || /\bserde_json::Value\b/u.test(source) || /\bjson::Value\b/u.test(source)) {
      findings.push(`${path.relative(workspaceRoot, filePath)} serializes the production canonical receipt path through serde_json::Value`);
    }
  }
}

if (findings.length > 0) {
  console.error("Receipt canonical production path check failed:");
  for (const finding of findings) {
    console.error(`- ${finding}`);
  }
  process.exit(1);
}

console.log("Receipt canonical production path check passed.");

function walk(directory) {
  const entries = readdirSync(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...walk(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files;
}

function isTestOnlyPath(filePath) {
  const normalized = filePath.split(path.sep).join("/");
  return normalized.includes("/tests/") || normalized.endsWith("_test.rs");
}

function stripTestOnlyModules(source) {
  return source.replaceAll(/#\[cfg\(test\)\][\s\S]*?mod\s+tests\s*\{[\s\S]*?\n\}/gu, "");
}
