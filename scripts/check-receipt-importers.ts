import { readdir, readFile, stat } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export type ReceiptAuditClassification =
  | "active_blocker"
  | "fixture_archive"
  | "generated_stale_artifact"
  | "false_positive"
  | "migrated";

export type ReceiptAuditKind =
  | "retired_core_receipts_export"
  | "retired_contract_export"
  | "retired_receipt_import"
  | "retired_receipt_type"
  | "retired_receipt_shape"
  | "legacy_receipt_id_prefix"
  | "runtime_pseudo_signature"
  | "harness_receipt_shape";

export interface ReceiptAuditFinding {
  readonly file: string;
  readonly line: number;
  readonly kind: ReceiptAuditKind;
  readonly classification: ReceiptAuditClassification;
  readonly token: string;
  readonly text: string;
}

export interface ReceiptAuditReport {
  readonly workspaceRoot: string;
  readonly scannedFiles: number;
  readonly findings: readonly ReceiptAuditFinding[];
  readonly cloudSibling: "not_found" | "scanned";
}

export interface ReceiptAuditOptions {
  readonly workspaceRoot?: string;
  readonly roots?: readonly string[];
  readonly includeCloudSibling?: boolean;
}

const workspaceRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));

const defaultRoots = [
  "apps",
  "crates",
  "fixtures",
  "packages",
  "plugins",
  "scripts",
  "tests",
] as const;

const sourceExtensions = new Set([
  ".cts",
  ".js",
  ".json",
  ".mjs",
  ".mts",
  ".rs",
  ".ts",
  ".tsx",
  ".yaml",
  ".yml",
]);

const ignoredDirectoryNames = new Set([
  ".build",
  ".data",
  ".git",
  ".turbo",
  "coverage",
  "dist",
  "node_modules",
  "target",
]);

const ignoredFiles = new Set([
  "scripts/check-receipt-importers.ts",
  "tests/check-receipt-importers.test.ts",
]);

const migratedHarnessPattern = /\brunx\.harness_receipt\.v1\b|\bHarnessReceipt\b|\bharness receipt\b/iu;
const runtimePseudoSignaturePattern = /\bsig:\{digest\}\b|\bsig:pending\b|\bruntime-skeleton\b|\bLocalHarnessSignatureVerifier\b/u;
const legacyIdPrefixPattern = /\.startsWith\(["']gx_["']\)|\.startsWith\(["']rx_["']\)/u;
const retiredReceiptTypePattern =
  /\bLocalSkillReceipt\b|\bLocalGraphReceipt\b|\bLocalReceiptContract\b|\bLocalSkillReceiptContract\b|\bLocalGraphReceiptContract\b/u;
const retiredReceiptShapePattern =
  /\bskill_execution\b|\bgraph_execution\b|\bskill_name\b|\bgraph_name\b|\bchildReceipts\b|\breceiptPath\b|\breceipt_path\b/u;
const retiredReceiptImportPattern =
  /\b(?:import|export)\s+(?:type\s+)?(?:[^'";]*?\s+from\s+)?["']([^"']+)["']|import\s*\(\s*["']([^"']+)["']\s*\)/u;

export async function scanReceiptImporters(options: ReceiptAuditOptions = {}): Promise<ReceiptAuditReport> {
  const root = path.resolve(options.workspaceRoot ?? workspaceRoot);
  const roots = options.roots ?? defaultRoots;
  const files: string[] = [];
  let cloudSibling: ReceiptAuditReport["cloudSibling"] = "not_found";

  for (const scanRoot of roots) {
    const absolute = path.resolve(root, scanRoot);
    if (await isDirectory(absolute)) {
      files.push(...await collectFiles(absolute, root));
    }
  }

  if (options.includeCloudSibling ?? true) {
    const cloudRoot = path.resolve(root, "..", "cloud");
    if (await isDirectory(cloudRoot)) {
      cloudSibling = "scanned";
      files.push(...await collectFiles(cloudRoot, root));
    }
  }

  files.sort();

  const findings: ReceiptAuditFinding[] = [];
  for (const file of files) {
    const rel = toPosix(path.relative(root, file));
    if (ignoredFiles.has(rel)) {
      continue;
    }
    const text = await readFile(file, "utf8");
    findings.push(...scanFile(rel, text));
  }

  return {
    workspaceRoot: root,
    scannedFiles: files.length,
    findings: findings.sort(compareFindings),
    cloudSibling,
  };
}

export function scanFile(file: string, source: string): readonly ReceiptAuditFinding[] {
  const findings: ReceiptAuditFinding[] = [];
  const lines = source.split(/\r?\n/u);

  for (const [index, line] of lines.entries()) {
    const lineNumber = index + 1;
    const trimmed = line.trim();

    if (file === "packages/core/package.json" && trimmed.startsWith('"./receipts"')) {
      findings.push(finding(file, lineNumber, "retired_core_receipts_export", "./receipts", line));
    }

    if (
      file === "packages/contracts/src/index.ts"
      && /\b(?:validateLocalSkillReceiptContract|validateLocalGraphReceiptContract|LocalSkillReceiptContract|LocalGraphReceiptContract)\b/u.test(line)
    ) {
      findings.push(finding(file, lineNumber, "retired_contract_export", "local receipt contract export", line));
    }

    const importMatch = line.match(retiredReceiptImportPattern);
    const specifier = importMatch?.[1] ?? importMatch?.[2];
    if (specifier && isRetiredReceiptImport(file, specifier)) {
      findings.push(finding(file, lineNumber, "retired_receipt_import", specifier, line));
    }

    const retiredType = line.match(retiredReceiptTypePattern)?.[0];
    if (retiredType) {
      findings.push(finding(file, lineNumber, "retired_receipt_type", retiredType, line));
    }

    const retiredShape = line.match(retiredReceiptShapePattern)?.[0];
    if (retiredShape && isReceiptShapeContext(file, line)) {
      findings.push(finding(file, lineNumber, "retired_receipt_shape", retiredShape, line));
    }

    const legacyIdPrefix = line.match(legacyIdPrefixPattern)?.[0];
    if (legacyIdPrefix && isLegacyReceiptIdContext(file, line)) {
      findings.push(finding(file, lineNumber, "legacy_receipt_id_prefix", legacyIdPrefix, line));
    }

    const pseudoSignature = line.match(runtimePseudoSignaturePattern)?.[0];
    if (pseudoSignature) {
      findings.push(finding(file, lineNumber, "runtime_pseudo_signature", pseudoSignature, line));
    }

    const harnessShape = line.match(migratedHarnessPattern)?.[0];
    if (harnessShape) {
      findings.push(finding(file, lineNumber, "harness_receipt_shape", harnessShape, line));
    }
  }

  return findings;
}

export function summarizeReceiptAudit(report: ReceiptAuditReport): Record<ReceiptAuditClassification, number> {
  const summary: Record<ReceiptAuditClassification, number> = {
    active_blocker: 0,
    fixture_archive: 0,
    generated_stale_artifact: 0,
    false_positive: 0,
    migrated: 0,
  };
  for (const finding of report.findings) {
    summary[finding.classification] += 1;
  }
  return summary;
}

export function hasDeletionBlockers(report: ReceiptAuditReport): boolean {
  return report.findings.some((finding) => finding.classification === "active_blocker");
}

function finding(
  file: string,
  line: number,
  kind: ReceiptAuditKind,
  token: string,
  text: string,
): ReceiptAuditFinding {
  return {
    file,
    line,
    kind,
    classification: classifyFinding(file, kind),
    token,
    text: text.trim(),
  };
}

function classifyFinding(file: string, kind: ReceiptAuditKind): ReceiptAuditClassification {
  if (kind === "harness_receipt_shape") {
    return "migrated";
  }

  if (isGeneratedStaleArtifact(file)) {
    return "generated_stale_artifact";
  }

  if (file.startsWith("fixtures/")) {
    return "fixture_archive";
  }

  if (kind === "legacy_receipt_id_prefix" && file.startsWith("packages/core/src/state-machine/")) {
    return "false_positive";
  }

  return "active_blocker";
}

function isGeneratedStaleArtifact(file: string): boolean {
  return (
    file.startsWith("scripts/generate-")
    || file.startsWith("fixtures/contracts/")
    || file.startsWith("fixtures/kernel/")
    || file.startsWith("fixtures/parser/")
    || file.startsWith("fixtures/sdk-rust/")
    || file.startsWith("fixtures/cli-parity/")
  );
}

function isRetiredReceiptImport(file: string, specifier: string): boolean {
  if (specifier === "@runxhq/core/receipts" || specifier.startsWith("@runxhq/core/receipts/")) {
    return true;
  }
  if (specifier.includes("packages/core/src/receipts")) {
    return true;
  }
  if (!specifier.startsWith(".")) {
    return false;
  }
  const target = toPosix(path.normalize(path.join(path.dirname(file), specifier)));
  if (file.startsWith("packages/core/src/receipts/") && target.startsWith("packages/core/src/receipts/")) {
    return false;
  }
  return target.includes("/receipts/") || target.endsWith("/receipts/index.js") || target.endsWith("/receipts");
}

function isReceiptShapeContext(file: string, line: string): boolean {
  if (file.startsWith("fixtures/")) {
    return true;
  }
  if (file.includes("/receipts/") || file.includes("receipt") || file.includes("history") || file.includes("harness")) {
    return true;
  }
  return /\breceipt\b|\bLocalReceipt\b|\bkind\b|\bsource_type\b/u.test(line);
}

function isLegacyReceiptIdContext(file: string, line: string): boolean {
  if (file.includes("receipt") || file.includes("history") || file.includes("journal") || file.includes("host-protocol")) {
    return true;
  }
  return /\breceipt(?:Id|_id)?\b|\brunId\b|\bgraphId\b|\bsourceReceiptId\b/u.test(line);
}

async function collectFiles(root: string, workspace: string): Promise<readonly string[]> {
  const files: string[] = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    if (ignoredDirectoryNames.has(entry.name)) {
      continue;
    }
    const entryPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      files.push(...await collectFiles(entryPath, workspace));
      continue;
    }
    if (!entry.isFile()) {
      continue;
    }
    const extension = path.extname(entry.name);
    if (!sourceExtensions.has(extension)) {
      continue;
    }
    const rel = toPosix(path.relative(workspace, entryPath));
    if (ignoredFiles.has(rel)) {
      continue;
    }
    files.push(entryPath);
  }
  return files;
}

async function isDirectory(filePath: string): Promise<boolean> {
  try {
    return (await stat(filePath)).isDirectory();
  } catch {
    return false;
  }
}

function compareFindings(left: ReceiptAuditFinding, right: ReceiptAuditFinding): number {
  return (
    left.classification.localeCompare(right.classification)
    || left.kind.localeCompare(right.kind)
    || left.file.localeCompare(right.file)
    || left.line - right.line
    || left.token.localeCompare(right.token)
  );
}

function toPosix(value: string): string {
  return value.split(path.sep).join("/");
}

function parseArgs(argv: readonly string[]): {
  readonly json: boolean;
  readonly failOnBlockers: boolean;
  readonly verbose: boolean;
} {
  return {
    json: argv.includes("--json"),
    failOnBlockers: argv.includes("--fail-on-active-blockers"),
    verbose: argv.includes("--verbose"),
  };
}

function printTextReport(report: ReceiptAuditReport, verbose: boolean): void {
  const summary = summarizeReceiptAudit(report);
  console.log(`Receipt importer audit scanned ${report.scannedFiles} files.`);
  console.log(`Cloud sibling: ${report.cloudSibling}.`);
  console.log(
    [
      `active_blocker=${summary.active_blocker}`,
      `fixture_archive=${summary.fixture_archive}`,
      `generated_stale_artifact=${summary.generated_stale_artifact}`,
      `migrated=${summary.migrated}`,
      `false_positive=${summary.false_positive}`,
    ].join(" "),
  );

  for (const classification of ["active_blocker", "generated_stale_artifact", "fixture_archive", "migrated", "false_positive"] as const) {
    const classified = report.findings.filter((finding) => finding.classification === classification);
    if (classified.length === 0) {
      continue;
    }
    const byKind = countBy(classified.map((finding) => finding.kind));
    console.log(`\n${classification} kinds: ${formatCounts(byKind)}`);
    if (!verbose) {
      const files = [...new Set(classified.map((finding) => finding.file))].sort();
      const preview = files.slice(0, 20);
      for (const file of preview) {
        console.log(`- ${file}`);
      }
      if (files.length > preview.length) {
        console.log(`- ... ${files.length - preview.length} more file(s); rerun with --verbose for line-level findings`);
      }
      continue;
    }
    console.log(`\n${classification}:`);
    for (const finding of classified) {
      console.log(`- ${finding.file}:${finding.line} ${finding.kind} ${JSON.stringify(finding.token)}`);
    }
  }
}

function countBy(values: readonly string[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const value of values) {
    counts[value] = (counts[value] ?? 0) + 1;
  }
  return counts;
}

function formatCounts(counts: Record<string, number>): string {
  return Object.entries(counts)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => `${key}=${value}`)
    .join(" ");
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv.slice(2));
  const report = await scanReceiptImporters();
  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    printTextReport(report, args.verbose);
  }
  if (args.failOnBlockers && hasDeletionBlockers(report)) {
    process.exitCode = 1;
  }
}

const mainUrl = process.argv[1] ? pathToFileUrl(process.argv[1]) : undefined;
if (mainUrl === import.meta.url) {
  await main();
}

function pathToFileUrl(filePath: string): string {
  let resolved = path.resolve(filePath).split(path.sep).join("/");
  if (!resolved.startsWith("/")) {
    resolved = `/${resolved}`;
  }
  return `file://${resolved}`;
}
