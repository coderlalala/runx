import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";

interface CommandMatrixEntry {
  readonly id: string;
  readonly usage: string;
  readonly aliases?: readonly string[];
  readonly requiredPositionals: readonly string[];
  readonly conditionalPositionals?: readonly string[];
  readonly flags: readonly string[];
  readonly exitCodes: readonly number[];
  readonly parity: {
    readonly humanOutput: "semantic" | "none";
    readonly jsonOutput: "schema-exact" | "none";
    readonly receipt: "schema-exact" | "none";
    readonly sideEffect: "none" | "filesystem" | "local-runtime" | "adapter" | "external-stub";
    readonly surfaces: readonly string[];
  };
  readonly cases: readonly string[];
}

interface RuntimeSurface {
  readonly id: string;
  readonly owner: string;
  readonly parityClass: "schema-exact" | "semantic" | "fixture-backed" | "stubbed";
  readonly coveredBy: readonly string[];
  readonly notes: string;
}

interface OracleCase {
  readonly id: string;
  readonly commandId: string;
  readonly mode: "execute" | "validate";
  readonly argv?: readonly string[];
  readonly expectedExitCode?: number;
  readonly expectJson?: boolean;
  readonly expect?: {
    readonly pendingRuns: number;
    readonly firstPendingRunId: string;
    readonly firstPendingRunStatus: string;
  };
  readonly stdoutIncludes?: readonly string[];
  readonly stderrIncludes?: readonly string[];
  readonly proves: readonly string[];
}

const check = process.argv.includes("--check");
const checkHelpCoverage = process.argv.includes("--check-help-coverage");
const canonicalOnly = process.argv.includes("--canonical-only");
const root = resolve(".");
const fixturesDir = join(root, "fixtures/cli-parity");
const casesDir = join(fixturesDir, "cases");

const exitCodes = [0, 1, 2, 64] as const;

const commands: readonly CommandMatrixEntry[] = [
  command("cli.help", "runx --help", [], ["--help", "-h"], "none", ["cli-presentation"], ["help.top-level"]),
  command("skill.run", "runx skill <skill-ref|skill-dir|SKILL.md>", [], ["--runner", "--input", "--non-interactive", "--json", "--answers"], "local-runtime", ["skill-resolution", "graph-runtime", "receipts", "sandbox", "caller-mediated-resolution", "adapter-cli-tool", "adapter-a2a", "adapter-agent"], ["skill.run.validate"]),
  command("skill.search", "runx skill search <query>", [], ["--source", "--registry", "--json"], "external-stub", ["registry", "cli-presentation"], ["skill.search.validate"]),
  command("skill.add", "runx skill add <ref>", [], ["--version", "--to", "--registry", "--digest", "--json"], "filesystem", ["registry", "skill-resolution"], ["skill.add.validate"]),
  command("skill.publish", "runx skill publish <skill-dir|SKILL.md>", [], ["--owner", "--version", "--registry", "--json"], "external-stub", ["registry", "receipts"], ["skill.publish.validate"]),
  command("skill.inspect", "runx skill inspect <receipt-id>", [], ["--receipt-dir", "--json"], "none", ["receipts", "cli-presentation"], ["skill.inspect.validate"]),
  command("evolve", "runx evolve [objective]", [], ["--receipt", "--non-interactive", "--json", "--answers"], "local-runtime", ["graph-runtime", "receipts", "artifacts"], ["evolve.validate"]),
  command("replay", "runx replay <run-id|receipt-id>", [], ["--receipt-dir", "--non-interactive", "--json", "--answers"], "local-runtime", ["continuation-replay", "ledger", "receipts"], ["replay.validate"]),
  command("diff", "runx diff <left-run-or-receipt> <right-run-or-receipt>", [], ["--receipt-dir", "--json"], "none", ["receipts", "cli-presentation"], ["diff.validate"]),
  command("history", "runx history [query]", [], ["--skill", "--status", "--source", "--actor", "--artifact-type", "--since", "--until", "--receipt-dir", "--json"], "none", ["history", "receipts"], ["history.validate"]),
  command("export-receipts.trainable", "runx export-receipts --trainable", [], ["--receipt-dir", "--since", "--until", "--status", "--source"], "none", ["trainable-export", "receipts"], ["export-receipts.validate"]),
  command("knowledge.show", "runx knowledge show --project .", [], ["--project", "--json"], "none", ["knowledge", "cli-presentation"], ["knowledge.show.validate"]),
  command("connect.list", "runx connect list", [], ["--json"], "external-stub", ["connect", "cli-presentation"], ["connect.list.validate"]),
  command("connect.revoke", "runx connect revoke <grant-id>", [], ["--json"], "external-stub", ["connect", "cli-presentation"], ["connect.revoke.validate"]),
  command("connect.preprovision", "runx connect <provider>", [], ["--scope", "--scope-family", "--authority-kind", "--target-repo", "--target-locator", "--json"], "external-stub", ["connect", "authority"], ["connect.preprovision.validate"]),
  command("config.set", "runx config set <key> <value>", [], ["--json"], "filesystem", ["config", "cli-presentation"], ["config.set.validate"]),
  command("config.get", "runx config get <key>", [], ["--json"], "filesystem", ["config", "cli-presentation"], ["config.get.validate"]),
  command("config.list", "runx config list", [], ["--json"], "filesystem", ["config", "cli-presentation"], ["config.list.execute"]),
  command("policy.inspect", "runx policy inspect <policy.json>", [], ["--json"], "none", ["policy", "cli-presentation"], ["policy.inspect.validate"]),
  command("policy.lint", "runx policy lint <policy.json>", [], ["--json"], "none", ["policy", "cli-presentation"], ["policy.lint.validate"]),
  command("new", "runx new <name>", [], ["--directory", "--json"], "filesystem", ["scaffold", "cli-presentation"], ["new.validate"]),
  command("init", "runx init", [], ["-g", "--global", "--prefetch", "--json"], "filesystem", ["scaffold", "official-skills"], ["init.validate"]),
  command("harness", "runx harness <fixture.yaml>", [], ["--json"], "local-runtime", ["harness", "receipts", "sandbox"], ["harness.execute"]),
  command("list", "runx list [tools|skills|graphs|packets|overlays]", [], ["--ok-only", "--invalid-only", "--json"], "none", ["list", "tool-catalog"], ["list.tools.execute"]),
  command("doctor", "runx doctor [path]", [], ["--fix", "--explain", "--list-diagnostics", "--json"], "filesystem", ["doctor", "cli-presentation"], ["doctor.validate"]),
  command("dev", "runx dev [path]", [], ["--lane", "--record", "--real-agents", "--watch", "--json"], "local-runtime", ["dev", "harness", "receipts"], ["dev.validate"]),
  command("mcp.serve", "runx mcp serve <skill-ref>", [], [], "adapter", ["mcp", "adapter-mcp"], ["mcp.serve.validate"]),
  command("tool.search", "runx tool search <query>", [], ["--source", "--json"], "external-stub", ["tool-catalog", "adapter-catalog"], ["tool.search.validate"]),
  command("tool.inspect", "runx tool inspect <ref>", [], ["--source", "--json"], "external-stub", ["tool-catalog", "adapter-catalog"], ["tool.inspect.validate"]),
  command("tool.build", "runx tool build <tool-dir>|--all", [], ["--all", "--json"], "filesystem", ["tool-catalog", "authoring"], ["tool.build.validate"], { conditionalPositionals: ["<tool-dir>"] }),
];

const surfaces: readonly RuntimeSurface[] = [
  surface("cli-presentation", "packages/cli", "semantic", ["cli.help", "config.list"], "Human output is normalized semantically; JSON output stays schema-exact."),
  surface("skill-resolution", "packages/cli + packages/core", "fixture-backed", ["skill.run", "skill.add"], "Covers local paths, registry refs, and official skill resolution."),
  surface("graph-runtime", "packages/runtime-local", "fixture-backed", ["skill.run", "evolve"], "Covers graph execution, branching, caller handoffs, and receipts."),
  surface("receipts", "packages/core + packages/runtime-local", "schema-exact", ["skill.run", "harness", "history", "export-receipts.trainable"], "Receipt JSON and signature metadata are schema-exact parity surfaces."),
  surface("ledger", "packages/runtime-local", "schema-exact", ["history"], "Append-only run state and continuation history must survive cutover."),
  surface("artifacts", "packages/core", "schema-exact", ["evolve", "dev"], "Large outputs are referenced by artifact metadata instead of copied into receipts."),
  surface("sandbox", "packages/core/policy + packages/runtime-local", "schema-exact", ["skill.run", "harness"], "Declared and enforced sandbox metadata must remain distinct."),
  surface("harness", "packages/runtime-local/harness", "fixture-backed", ["harness", "dev"], "Harness replay mode proves deterministic fixture execution and sealed receipt checks."),
  surface("history", "packages/cli", "semantic", ["history"], "Search/filter behavior is command-level parity with normalized output."),
  surface("continuation-replay", "packages/runtime-local", "fixture-backed", ["replay", "diff"], "Continuation runs and replay/diff inputs must resolve to the same receipt graph semantics."),
  surface("registry", "packages/core/registry", "stubbed", ["skill.search", "skill.add", "skill.publish"], "Live registries are replaced by deterministic registry fixtures."),
  surface("tool-catalog", "packages/runtime-local/tool-catalogs", "stubbed", ["tool.search", "tool.inspect", "tool.build", "list"], "Catalog discovery and local tool builds use fixtures or local files."),
  surface("mcp", "packages/runtime-local/mcp", "stubbed", ["mcp.serve"], "Protocol behavior uses local servers and deterministic clients."),
  surface("adapter-cli-tool", "packages/adapters/cli-tool", "fixture-backed", ["skill.run"], "Process invocation, env, cwd, and sandbox metadata are parity-critical."),
  surface("adapter-mcp", "packages/adapters/mcp", "stubbed", ["mcp.serve"], "MCP transport and tool results use local protocol fixtures."),
  surface("adapter-a2a", "packages/adapters/a2a", "stubbed", ["skill.run"], "A2A remains a deterministic adapter path until live provider cutover."),
  surface("adapter-catalog", "packages/adapters/catalog", "stubbed", ["tool.search", "tool.inspect"], "Catalog adapter inputs and normalized outputs are preserved."),
  surface("adapter-agent", "packages/adapters/agent", "stubbed", ["skill.run", "dev"], "Managed agent calls are represented by local stubs, not live providers."),
  surface("config", "packages/cli", "schema-exact", ["config.set", "config.get", "config.list"], "RUNX_HOME and local config file behavior are part of CLI parity."),
  surface("doctor", "packages/cli", "semantic", ["doctor"], "Diagnostics can add ids, but the documented command surface must not disappear."),
  surface("dev", "packages/cli", "fixture-backed", ["dev"], "Development lanes run deterministic or recorded harness fixtures."),
  surface("knowledge", "packages/core/knowledge", "schema-exact", ["knowledge.show"], "Public projection output stays schema-exact."),
  surface("connect", "packages/cli", "stubbed", ["connect.list", "connect.revoke", "connect.preprovision"], "OAuth/provider calls are represented by stub services."),
  surface("authority", "packages/core/policy", "schema-exact", ["connect.preprovision", "skill.run"], "Grant, scope, and authority-kind parsing must remain machine-checkable."),
  surface("policy", "packages/core/policy", "schema-exact", ["policy.inspect", "policy.lint"], "Policy inspection and linting stay machine-checkable before mutation gates run."),
  surface("caller-mediated-resolution", "packages/runtime-local", "fixture-backed", ["skill.run"], "Required input, approvals, and agent work keep the same continuation contract."),
  surface("scaffold", "packages/cli", "semantic", ["new", "init"], "Project and standalone package scaffolds preserve command shape and generated-file intent."),
  surface("official-skills", "packages/cli", "schema-exact", ["init"], "Prefetch and lockfile behavior stays fixture-backed before Rust cutover."),
  surface("list", "packages/cli", "semantic", ["list"], "Inventory output for tools, skills, graphs, packets, and overlays stays represented."),
  surface("authoring", "packages/authoring", "schema-exact", ["tool.build"], "Tool build output and manifest validation remain schema-exact."),
  surface("trainable-export", "packages/cli", "schema-exact", ["export-receipts.trainable"], "Redacted trainable receipt export remains a contract surface."),
];

const casesExecutedById = new Set([
  "help.top-level",
  "config.list.execute",
  "harness.execute",
  "history.execute",
  "list.tools.execute",
]);

const cases: readonly OracleCase[] = [
  execute("help.top-level", "cli.help", ["--help"], 0, false, ["Usage:", "runx skill", "runx harness"], []),
  execute("usage.unsupported", "cli.help", ["not-a-command"], 64, false, [], ["Usage:"]),
  execute("config.list.execute", "config.list", ["config", "list", "--json"], 0, true, [], []),
  execute("harness.execute", "harness", ["harness", "fixtures/harness/echo-skill.yaml", "--json"], 0, true, [], []),
  {
    id: "history.execute",
    commandId: "history",
    mode: "execute",
    argv: ["history", "--receipt-dir", "$FIXTURE_RECEIPTS", "--json"],
    expectedExitCode: 0,
    expectJson: true,
    expect: {
      pendingRuns: 1,
      firstPendingRunId: "gx_needs_agent_oracle",
      firstPendingRunStatus: "paused",
    },
    stdoutIncludes: ["\"pendingRuns\"", "\"gx_needs_agent_oracle\"", "\"selectedRunner\": \"agent-step\""],
    stderrIncludes: [],
    proves: ["history", "ledger", "receipts", "cli-presentation"],
  },
  execute("list.tools.execute", "list", ["list", "tools", "--json"], 0, true, [], []),
  ...commands
    .filter((entry) => !entry.cases.some((caseId) => casesExecutedById.has(caseId)))
    .map((entry) => validate(`${entry.id}.validate`, entry.id, entry.parity.surfaces)),
];

const files = new Map<string, string>([
  [join(fixturesDir, "README.md"), readme()],
  [join(fixturesDir, "commands.json"), stableJson({ schema: "runx.cli_feature_parity_matrix.v1", sourceOfTruth: "@runxhq/cli TypeScript implementation", exitCodes, commands })],
  [join(fixturesDir, "runtime-surfaces.json"), stableJson({ schema: "runx.cli_runtime_surfaces.v1", surfaces })],
  [join(casesDir, "oracle.json"), stableJson({ schema: "runx.cli_parity_oracle_cases.v1", cases })],
]);

if (canonicalOnly) {
  checkCanonicalOnly();
}

if (checkHelpCoverage) {
  checkUsageCoverage();
}

if (check) {
  checkFiles();
} else {
  writeFiles();
}

function command(
  id: string,
  usage: string,
  aliases: readonly string[],
  flags: readonly string[],
  sideEffect: CommandMatrixEntry["parity"]["sideEffect"],
  surfaces: readonly string[],
  casesForCommand: readonly string[],
  options: { readonly conditionalPositionals?: readonly string[] } = {},
): CommandMatrixEntry {
  const conditionalPositionals = new Set(options.conditionalPositionals ?? []);
  const requiredPositionals = (usage.match(/<[^>]+>/g) ?? [])
    .filter((positional) => !conditionalPositionals.has(positional));
  return {
    id,
    usage,
    aliases,
    requiredPositionals,
    ...(conditionalPositionals.size > 0 ? { conditionalPositionals: [...conditionalPositionals] } : {}),
    flags,
    exitCodes,
    parity: {
      humanOutput: "semantic",
      jsonOutput: flags.includes("--json") ? "schema-exact" : "none",
      receipt: surfaces.includes("receipts") ? "schema-exact" : "none",
      sideEffect,
      surfaces,
    },
    cases: casesForCommand,
  };
}

function surface(
  id: string,
  owner: string,
  parityClass: RuntimeSurface["parityClass"],
  coveredBy: readonly string[],
  notes: string,
): RuntimeSurface {
  return { id, owner, parityClass, coveredBy, notes };
}

function execute(
  id: string,
  commandId: string,
  argv: readonly string[],
  expectedExitCode: number,
  expectJson: boolean,
  stdoutIncludes: readonly string[],
  stderrIncludes: readonly string[],
): OracleCase {
  return {
    id,
    commandId,
    mode: "execute",
    argv,
    expectedExitCode,
    expectJson,
    stdoutIncludes,
    stderrIncludes,
    proves: commands.find((entry) => entry.id === commandId)?.parity.surfaces ?? [],
  };
}

function validate(id: string, commandId: string, proves: readonly string[]): OracleCase {
  return { id, commandId, mode: "validate", proves };
}

function readme(): string {
  return `# CLI Feature Parity Matrix

This directory is the TypeScript oracle for future native Rust CLI/runtime
cutovers. The matrix is generated from \`scripts/generate-cli-feature-parity.ts\`
and checked against the current help surface.

Required exit-code coverage: \`"exitCodes": [0, 1, 2, 64]\`.

## Files

- \`commands.json\`: command, alias, flag, exit-code, output, receipt, and
  side-effect coverage.
- \`runtime-surfaces.json\`: non-help runtime surfaces that must not disappear
  during a Rust rebuild.
- \`cases/oracle.json\`: executable or validation-only oracle cases.

## Parity Rules

- JSON output and receipt behavior are schema-exact.
- Human output is semantic and may be normalized for timestamps, paths,
  receipt ids, and platform-specific wording.
- Live providers are replaced by deterministic mocks, fixtures, or local
  protocol servers.
- Rust candidates must pass this matrix before any npm-to-Rust CLI cutover.
`;
}

function checkUsageCoverage(): void {
  const usageCommands = extractUsageCommands(readFileSync(join(root, "packages/cli/src/help.ts"), "utf8"));
  const commandIds = new Set(commands.map((entry) => entry.id));
  const missing = usageCommands.flatMap((usage) =>
    helpUsageCommandIds(usage)
      .filter((id) => !commandIds.has(id))
      .map((id) => `${usage} -> ${id}`));
  if (missing.length > 0) {
    throw new Error(`CLI parity matrix is missing help usage entries:\n${missing.join("\n")}`);
  }
}

function extractUsageCommands(helpSource: string): readonly string[] {
  const quoted = [...helpSource.matchAll(/"([^"]*)"/g)].map((match) => match[1] ?? "");
  return [
    ...extractQuotedHelpBlock(quoted, "Usage:"),
    ...extractQuotedHelpBlock(quoted, "Manage Skills:"),
  ];
}

function extractQuotedHelpBlock(quoted: readonly string[], label: string): readonly string[] {
  const start = quoted.indexOf(label);
  if (start === -1) {
    throw new Error(`Could not find ${label} block in packages/cli/src/help.ts`);
  }
  const entries: string[] = [];
  for (const line of quoted.slice(start + 1)) {
    if (line === "") {
      break;
    }
    const trimmed = line.trim();
    if (trimmed.startsWith("runx ")) {
      entries.push(trimmed);
    }
  }
  return entries;
}

function helpUsageCommandIds(usage: string): readonly string[] {
  if (usage.startsWith("runx skill <")) {
    return ["skill.run"];
  }
  if (usage.startsWith("runx skill search")) {
    return ["skill.search"];
  }
  if (usage.startsWith("runx skill add")) {
    return ["skill.add"];
  }
  if (usage.startsWith("runx skill publish")) {
    return ["skill.publish"];
  }
  if (usage.startsWith("runx skill inspect")) {
    return ["skill.inspect"];
  }
  if (usage.startsWith("runx export-receipts")) {
    return ["export-receipts.trainable"];
  }
  if (usage.startsWith("runx knowledge show")) {
    return ["knowledge.show"];
  }
  if (usage.startsWith("runx connect ")) {
    return ["connect.list", "connect.revoke", "connect.preprovision"];
  }
  if (usage.startsWith("runx config ")) {
    return ["config.set", "config.get", "config.list"];
  }
  if (usage.startsWith("runx policy inspect|lint")) {
    return ["policy.inspect", "policy.lint"];
  }
  if (usage.startsWith("runx policy inspect")) {
    return ["policy.inspect"];
  }
  if (usage.startsWith("runx policy lint")) {
    return ["policy.lint"];
  }
  if (usage.startsWith("runx mcp serve")) {
    return ["mcp.serve"];
  }
  if (usage.startsWith("runx tool search")) {
    return ["tool.search"];
  }
  if (usage.startsWith("runx tool inspect")) {
    return ["tool.inspect"];
  }
  if (usage.startsWith("runx tool build")) {
    return ["tool.build"];
  }
  const commandName = usage.split(/\s+/)[1];
  if (!commandName) {
    return [];
  }
  return [commandName];
}

function checkCanonicalOnly(): void {
  const aliases = commands.flatMap((entry) =>
    (entry.aliases ?? []).map((alias) => `${entry.id}: ${alias}`));
  if (aliases.length > 0) {
    throw new Error(`canonical CLI matrix must not include aliases:\n${aliases.join("\n")}`);
  }
}

function checkFiles(): void {
  const stale = [...files.entries()]
    .filter(([path, contents]) => !existsSync(path) || readFileSync(path, "utf8") !== contents)
    .map(([path]) => path);
  if (stale.length > 0) {
    throw new Error(`CLI parity fixtures are stale; run this script without --check:\n${stale.join("\n")}`);
  }
  const caseFiles = readdirSync(casesDir).filter((name) => name.endsWith(".json"));
  if (!caseFiles.includes("oracle.json")) {
    throw new Error("fixtures/cli-parity/cases/oracle.json is missing");
  }
}

function writeFiles(): void {
  for (const [path, contents] of files) {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, contents);
  }
}

function stableJson(value: unknown): string {
  return `${JSON.stringify(value, null, 2)}\n`;
}
