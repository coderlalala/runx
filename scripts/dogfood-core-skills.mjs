#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
const rustKernelBin = path.join(
  workspaceRoot,
  "crates",
  "target",
  "debug",
  process.platform === "win32" ? "runx.exe" : "runx",
);
const dogfoodEnv = { ...process.env, RUNX_KERNEL_EVAL_BIN: rustKernelBin };
const steps = [
  {
    label: "build rust kernel eval binary",
    command: cargo,
    args: ["build", "--quiet", "--manifest-path", "crates/Cargo.toml", "-p", "runx-cli", "--bin", "runx"],
  },
  {
    label: "prove rust payment runtime",
    command: cargo,
    args: ["test", "--quiet", "--manifest-path", "crates/Cargo.toml", "-p", "runx-runtime", "--test", "payment_execution"],
  },
  {
    label: "prove rust Stripe SPT payment runtime",
    command: cargo,
    args: ["test", "--quiet", "--manifest-path", "crates/Cargo.toml", "-p", "runx-runtime", "--test", "stripe_spt_payment"],
  },
  {
    label: "build workspace packages",
    command: pnpm,
    args: ["build"],
  },
  {
    label: "run workspace doctor",
    command: pnpm,
    args: ["exec", "tsx", "packages/cli/src/index.ts", "doctor", "--json"],
  },
  {
    label: "prove x402 mock payment fixtures",
    command: pnpm,
    args: ["exec", "vitest", "run", "tests/x402-pay-dogfood-mock.test.ts"],
  },
  {
    label: "prove payment skill profiles",
    command: pnpm,
    args: ["exec", "vitest", "run", "tests/payment-skill-profile-validation.test.ts"],
  },
  {
    label: "prove canonical payment graph harnesses",
    command: pnpm,
    args: ["exec", "vitest", "run", "tests/payment-graph-harness.test.ts"],
  },
  {
    label: "prove official skills with a fresh caller",
    command: pnpm,
    args: ["exec", "vitest", "run", "tests/external-skill-proving-ground.test.ts"],
  },
];

for (const step of steps) {
  process.stdout.write(`\n[dogfood] ${step.label}\n`);
  const result = spawnSync(step.command, step.args, {
    stdio: "inherit",
    shell: false,
    cwd: workspaceRoot,
    env: dogfoodEnv,
  });
  if (result.status === 0) {
    continue;
  }
  process.exit(result.status ?? 1);
}
