import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const workspaceRoot = path.resolve(fileURLToPath(new URL("..", import.meta.url)));
const defaultFixturePath = path.join(workspaceRoot, "tests", "fixtures", "clean-kernel-prs.json");

export type CleanKernelPrReason =
  | "ts_kernel"
  | "kernel_fixture_refresh"
  | "rust_only"
  | "parser_only"
  | "missing_passing_evidence"
  | "outside_kernel_promotion_scope";

export interface CleanKernelPrReportEntry {
  readonly number?: number;
  readonly title: string;
  readonly reason: CleanKernelPrReason;
  readonly files: readonly string[];
  readonly passing_evidence: boolean;
}

export interface CleanKernelPrReport {
  readonly advisory_start: unknown;
  readonly advisory_start_source: "cli" | "fixture";
  readonly minimum: number;
  readonly count: number;
  readonly meets_minimum: boolean;
  readonly counting: readonly CleanKernelPrReportEntry[];
  readonly non_counting: readonly CleanKernelPrReportEntry[];
}

interface CliOptions {
  readonly fixturePath: string;
  readonly advisoryStart?: string;
  readonly minimum: number;
}

interface CliRunOptions {
  readonly cwd?: string;
}

interface CliRunResult {
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
}

export async function runCountCleanKernelPrsCli(
  args: readonly string[],
  options: CliRunOptions = {},
): Promise<CliRunResult> {
  try {
    const parsed = parseCliArgs(args, options.cwd ?? workspaceRoot);
    const fixture = await readJsonFile(parsed.fixturePath);
    const report = analyzeCleanKernelPrs(fixture, {
      advisoryStart: parsed.advisoryStart,
      minimum: parsed.minimum,
    });
    return {
      exitCode: report.meets_minimum ? 0 : 1,
      stdout: `${JSON.stringify(report, null, 2)}\n`,
      stderr: report.meets_minimum
        ? ""
        : `clean kernel PR count ${report.count} is below required minimum ${report.minimum}\n`,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return {
      exitCode: 1,
      stdout: "",
      stderr: `${message}\n`,
    };
  }
}

export function analyzeCleanKernelPrs(
  fixture: unknown,
  options: {
    readonly advisoryStart?: unknown;
    readonly minimum?: number;
  } = {},
): CleanKernelPrReport {
  const fixtureObject = asRecord(fixture, "fixture");
  const advisory = resolveAdvisoryStart(fixtureObject, options.advisoryStart);
  const prs = readPullRequests(fixtureObject);
  const minimum = options.minimum ?? readFixtureMinimum(fixtureObject) ?? 1;

  if (!Number.isInteger(minimum) || minimum < 0) {
    throw new Error(`--min must be a non-negative integer`);
  }

  const entries = prs.map(classifyPullRequest);
  const counting = entries.filter((entry) => entry.reason === "ts_kernel" || entry.reason === "kernel_fixture_refresh");
  const nonCounting = entries.filter((entry) => !counting.includes(entry));

  return {
    advisory_start: advisory.value,
    advisory_start_source: advisory.source,
    minimum,
    count: counting.length,
    meets_minimum: counting.length >= minimum,
    counting,
    non_counting: nonCounting,
  };
}

function classifyPullRequest(rawPr: unknown): CleanKernelPrReportEntry {
  const pr = asRecord(rawPr, "pull request");
  const files = readFiles(pr);
  const passingEvidence = hasPassingEvidence(pr);
  const base = {
    number: readNumber(pr),
    title: readTitle(pr),
    files,
    passing_evidence: passingEvidence,
  };

  if (isParserOnly(files)) {
    return { ...base, reason: "parser_only" };
  }
  if (isRustOnly(files)) {
    return { ...base, reason: "rust_only" };
  }
  if (!passingEvidence) {
    return { ...base, reason: "missing_passing_evidence" };
  }
  if (isTsKernelOnly(files)) {
    return { ...base, reason: "ts_kernel" };
  }
  if (isDeliberateKernelFixtureRefresh(pr, files)) {
    return { ...base, reason: "kernel_fixture_refresh" };
  }
  return { ...base, reason: "outside_kernel_promotion_scope" };
}

function parseCliArgs(args: readonly string[], cwd: string): CliOptions {
  let fixturePath = defaultFixturePath;
  let advisoryStart: string | undefined;
  let minimum = 1;

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--fixture") {
      fixturePath = requiredArgValue(args, index, arg);
      index += 1;
      continue;
    }
    if (arg === "--advisory-start") {
      advisoryStart = requiredArgValue(args, index, arg);
      index += 1;
      continue;
    }
    if (arg === "--min") {
      const value = requiredArgValue(args, index, arg);
      minimum = Number(value);
      if (!Number.isInteger(minimum) || minimum < 0) {
        throw new Error(`--min must be a non-negative integer`);
      }
      index += 1;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      throw new Error("usage: tsx scripts/count-clean-kernel-prs.ts [--fixture path] [--advisory-start evidence] [--min count]");
    }
    throw new Error(`unsupported argument: ${arg}`);
  }

  return {
    fixturePath: path.resolve(cwd, fixturePath),
    advisoryStart,
    minimum,
  };
}

function requiredArgValue(args: readonly string[], index: number, flag: string): string {
  const value = args[index + 1];
  if (!value || value.startsWith("--")) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function resolveAdvisoryStart(
  fixture: Record<string, unknown>,
  cliAdvisoryStart: unknown,
): { readonly source: "cli" | "fixture"; readonly value: unknown } {
  if (hasExplicitValue(cliAdvisoryStart)) {
    return { source: "cli", value: cliAdvisoryStart };
  }

  const snake = fixture.advisory_start;
  const camel = fixture.advisoryStart;
  if (hasExplicitValue(snake) && hasExplicitValue(camel) && JSON.stringify(snake) !== JSON.stringify(camel)) {
    throw new Error("fixture advisory_start and advisoryStart disagree");
  }
  if (hasExplicitValue(snake)) {
    return { source: "fixture", value: snake };
  }
  if (hasExplicitValue(camel)) {
    return { source: "fixture", value: camel };
  }
  throw new Error("missing advisory start evidence; pass --advisory-start or set fixture.advisory_start/advisoryStart");
}

function hasExplicitValue(value: unknown): boolean {
  if (typeof value === "string") {
    return value.trim().length > 0;
  }
  return value !== undefined && value !== null;
}

function readPullRequests(fixture: Record<string, unknown>): readonly unknown[] {
  const prs = fixture.prs ?? fixture.pull_requests ?? fixture.pullRequests;
  if (!Array.isArray(prs)) {
    throw new Error("fixture must include a prs, pull_requests, or pullRequests array");
  }
  return prs;
}

function readFixtureMinimum(fixture: Record<string, unknown>): number | undefined {
  const minimum = fixture.minimum ?? fixture.min;
  if (minimum === undefined) {
    return undefined;
  }
  if (typeof minimum !== "number") {
    throw new Error("fixture minimum/min must be a number");
  }
  return minimum;
}

function readFiles(pr: Record<string, unknown>): readonly string[] {
  const files = pr.files ?? pr.changed_files ?? pr.changedFiles;
  if (!Array.isArray(files) || !files.every((file) => typeof file === "string" && file.trim().length > 0)) {
    throw new Error(`pull request ${readTitle(pr)} must include a non-empty files array`);
  }
  return files.map((file) => normalizePath(file as string));
}

function readTitle(pr: Record<string, unknown>): string {
  const title = pr.title;
  return typeof title === "string" && title.trim().length > 0 ? title : "(untitled)";
}

function readNumber(pr: Record<string, unknown>): number | undefined {
  const value = pr.number ?? pr.pr_number ?? pr.prNumber;
  return typeof value === "number" && Number.isInteger(value) ? value : undefined;
}

function hasPassingEvidence(pr: Record<string, unknown>): boolean {
  if (pr.passing_evidence === true || pr.passingEvidence === true) {
    return true;
  }
  const evidence = pr.evidence;
  if (!evidence || typeof evidence !== "object" || Array.isArray(evidence)) {
    return false;
  }
  return evidenceContainsPass(evidence as Record<string, unknown>);
}

function evidenceContainsPass(evidence: Record<string, unknown>): boolean {
  const directValues = [evidence.status, evidence.verdict, evidence.conclusion, evidence.result];
  const directEvidence = directValues.filter((value) => value !== undefined && value !== null);
  const hasDirectPass = directValues.some(isPassingToken);

  const checks = evidence.checks ?? evidence.required_checks ?? evidence.requiredChecks;
  if (Array.isArray(checks) && checks.length > 0) {
    const allChecksPass = checks.every((check) => {
      if (!check || typeof check !== "object" || Array.isArray(check)) {
        return false;
      }
      const record = check as Record<string, unknown>;
      return [record.status, record.verdict, record.conclusion, record.result].some(isPassingToken);
    });
    return allChecksPass && (directEvidence.length === 0 || hasDirectPass);
  }
  return hasDirectPass;
}

function isPassingToken(value: unknown): boolean {
  return typeof value === "string" && ["clean", "pass", "passed", "success", "successful"].includes(value.toLowerCase());
}

function isTsKernelOnly(files: readonly string[]): boolean {
  return files.every(isTsKernelFile);
}

function isDeliberateKernelFixtureRefresh(pr: Record<string, unknown>, files: readonly string[]): boolean {
  const explicit = pr.deliberate_kernel_fixture_refresh === true
    || pr.deliberateKernelFixtureRefresh === true
    || pr.kind === "kernel_fixture_refresh"
    || pr.classification === "kernel_fixture_refresh";
  return explicit
    && files.some(isKernelFixtureFile)
    && files.every((file) => isKernelFixtureFile(file) || isTsKernelFile(file));
}

function isTsKernelFile(file: string): boolean {
  return /^packages\/core\/src\/(?:state-machine|policy)\/.+\.ts$/u.test(file);
}

function isKernelFixtureFile(file: string): boolean {
  return file.startsWith("fixtures/kernel/");
}

function isRustOnly(files: readonly string[]): boolean {
  return files.every((file) => file.startsWith("crates/") || file === "Cargo.toml" || file.endsWith(".rs"));
}

function isParserOnly(files: readonly string[]): boolean {
  return files.every((file) =>
    file.startsWith("packages/core/src/parser/")
    || file.startsWith("crates/runx-parser/")
    || file.startsWith("fixtures/parser/")
    || file === "scripts/generate-rust-parser-fixtures.ts"
    || file.endsWith("-parser.test.ts"),
  );
}

function normalizePath(value: string): string {
  return value.replace(/\\/gu, "/").replace(/^\.\//u, "");
}

function asRecord(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be a JSON object`);
  }
  return value as Record<string, unknown>;
}

async function readJsonFile(filePath: string): Promise<unknown> {
  return JSON.parse(await readFile(filePath, "utf8")) as unknown;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  void runCountCleanKernelPrsCli(process.argv.slice(2)).then((result) => {
    if (result.stdout) {
      process.stdout.write(result.stdout);
    }
    if (result.stderr) {
      process.stderr.write(result.stderr);
    }
    process.exitCode = result.exitCode;
  });
}
