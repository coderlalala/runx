import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
const rustKernelBin = path.join(
  workspaceRoot,
  "crates",
  "target",
  "debug",
  process.platform === "win32" ? "runx.exe" : "runx",
);
const rustHarnessFixtureOracleBin = path.join(
  workspaceRoot,
  "crates",
  "target",
  "debug",
  process.platform === "win32" ? "runx-harness-fixture-oracles.exe" : "runx-harness-fixture-oracles",
);

const commands = [
  ["pnpm", ["boundary:check"]],
  ["pnpm", ["typecheck"]],
  ["node", ["scripts/build-workspace.mjs"]],
  ["node", ["scripts/check-publishable-package-manifests.mjs"]],
  ["pnpm", ["rust:crate-graph"]],
  ["pnpm", ["rust:style"]],
  ["node", ["scripts/check-authoring-package-contract.mjs"]],
  ["node", ["scripts/check-create-skill-package-contract.mjs"]],
  ["pnpm", ["fixtures:kernel:validate"]],
  ["pnpm", ["fixtures:kernel:check"]],
  ["pnpm", ["fixtures:kernel:keys"]],
  ["pnpm", ["fixtures:contracts:check"]],
  ["pnpm", ["fixtures:contracts:keys"]],
  ["pnpm", ["fixtures:harness:check"]],
  ["pnpm", ["fixtures:adapters:a2a:check"]],
  ["pnpm", ["fixtures:adapters:agent:check"]],
  ["pnpm", ["fixtures:cli-parity:check"]],
  ["pnpm", ["fixtures:cli-help:check"]],
  ["pnpm", ["docs:exit-codes"]],
  ["pnpm", ["exec", "tsx", "packages/cli/src/index.ts", "doctor", "--json"]],
  ["pnpm", ["test:fast"]],
];

buildRustBin(["build", "--quiet", "--manifest-path", "crates/Cargo.toml", "-p", "runx-cli", "--bin", "runx"]);
buildRustBin([
  "build",
  "--quiet",
  "--manifest-path",
  "crates/Cargo.toml",
  "-p",
  "runx-runtime",
  "--features",
  "cli-tool",
  "--bin",
  "runx-harness-fixture-oracles",
]);

// The binary is built once above; point the kernel / parser / CLI eval paths at
// that single prebuilt binary so subprocess-backed suites never cold-start a
// debug binary under parallel load.
const evalBinEnv = {
  RUNX_KERNEL_EVAL_BIN: rustKernelBin,
  RUNX_PARSER_EVAL_BIN: rustKernelBin,
  RUNX_RUST_CLI_BIN: rustKernelBin,
  RUNX_DEV_RUST_CLI_BIN: rustKernelBin,
  RUNX_HARNESS_FIXTURE_ORACLE_BIN: rustHarnessFixtureOracleBin,
  RUNX_RECEIPT_SIGN_KID: process.env.RUNX_RECEIPT_SIGN_KID ?? "verify-fast-test-key",
  RUNX_RECEIPT_SIGN_ED25519_SEED_BASE64:
    process.env.RUNX_RECEIPT_SIGN_ED25519_SEED_BASE64 ?? "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=",
  RUNX_RECEIPT_SIGN_ISSUER_TYPE: process.env.RUNX_RECEIPT_SIGN_ISSUER_TYPE ?? "hosted",
};

for (const [command, args] of commands) {
  const result = spawnSync(command, args, {
    cwd: workspaceRoot,
    env: { ...process.env, ...evalBinEnv },
    stdio: "inherit",
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function buildRustBin(args) {
  const result = spawnSync(cargo, args, {
    cwd: workspaceRoot,
    env: process.env,
    stdio: "inherit",
  });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
