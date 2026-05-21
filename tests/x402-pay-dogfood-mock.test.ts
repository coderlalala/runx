import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { runCli } from "../packages/cli/src/index.js";

const mockScenarioPunchlistCandidates = [
  ".scafld/specs/active/x402-pay-phase1-mock-scenario-punchlist.md",
  ".scafld/specs/approved/x402-pay-phase1-mock-scenario-punchlist.md",
  ".scafld/specs/drafts/x402-pay-phase1-mock-scenario-punchlist.md",
  ".scafld/specs/archive/2026-05/x402-pay-phase1-mock-scenario-punchlist.md",
] as const;
const paymentGraphFixture = "fixtures/harness/x402-pay-approval.yaml";
const deniedPaymentGraphFixture = "fixtures/harness/x402-pay-approval-denied.yaml";
const mockRailSessionMaterialRef = "rail-session-material:mock:payment-execution-001";
const rustKernelBin = path.resolve(
  "crates",
  "target",
  "debug",
  process.platform === "win32" ? "runx.exe" : "runx",
);

const coveredScenarios = new Set(["P1.1", "P1.5", "P1.6", "P1.15", "P1.16"]);
const punchlistedScenarios = [
  "P1.2",
  "P1.3",
  "P1.4",
  "P1.7",
  "P1.8",
  "P1.9",
  "P1.10",
  "P1.11",
  "P1.12",
  "P1.13",
  "P1.14",
  "P1.17",
] as const;

describe("x402-pay Phase 1 mock dogfood fixtures", () => {
  it("runs the mock approval graph through the CLI harness surface", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-x402-pay-cli-"));
    const stdout = createMemoryStream();
    const stderr = createMemoryStream();

    try {
      const exitCode = await runCli(
        ["harness", paymentGraphFixture, "--json"],
        { stdin: process.stdin, stdout, stderr },
        {
          ...paymentDogfoodEnv(),
          RUNX_CWD: process.cwd(),
          RUNX_HOME: path.join(tempDir, "home"),
        },
      );

      expect(exitCode).toBe(0);
      expect(stderr.contents()).toBe("");
      const receipt = requireRecord(JSON.parse(stdout.contents()), "receipt");
      expect(requireRecord(receipt.harness, "receipt.harness").state).toBe("sealed");
      expect(requireRecord(receipt.seal, "receipt.seal").disposition).toBe("closed");
      expect(childReceiptUris(receipt)).toEqual([
        "runx:harness_receipt:hrn_rcpt_x402-pay-approval_approve-spend",
        "runx:harness_receipt:hrn_rcpt_x402-pay-approval_fulfill",
      ]);
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  }, 30_000);

  it("seals the happy path only after the mock rail proof is present and history can observe it", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-x402-pay-history-"));

    try {
      const { receipt, stdout } = await runHarnessJson(paymentGraphFixture, {
        RUNX_HOME: path.join(tempDir, "home"),
      });
      expect(requireRecord(receipt.harness, "receipt.harness").state).toBe("sealed");
      expect(requireRecord(receipt.seal, "receipt.seal").disposition).toBe("closed");
      expect(childReceiptUris(receipt)).toEqual([
        "runx:harness_receipt:hrn_rcpt_x402-pay-approval_approve-spend",
        "runx:harness_receipt:hrn_rcpt_x402-pay-approval_fulfill",
      ]);

      expect(stdout).not.toContain("rail_session_material_ref");
      expect(stdout).not.toContain(mockRailSessionMaterialRef);
      expect(stdout).not.toContain("credential_envelope");

      const receiptDir = path.join(tempDir, "receipts");
      await writeReceiptForHistory(receiptDir, receipt);
      const history = await runHistory(receiptDir, path.join(tempDir, "home"));
      expect(history.receipts).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            id: receipt.id,
            status: "closed",
          }),
        ]),
      );
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  }, 30_000);

  it("halts cleanly when the payment approval gate is denied", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-x402-pay-denied-"));

    try {
      const { receipt } = await runHarnessJson(deniedPaymentGraphFixture, {
        RUNX_HOME: path.join(tempDir, "home"),
      });

      expect(requireRecord(receipt.harness, "receipt.harness").state).toBe("sealed");
      expect(requireRecord(receipt.seal, "receipt.seal")).toMatchObject({
        disposition: "blocked",
        reason_code: "graph_blocked",
      });
      expect(childReceiptUris(receipt)).toEqual([
        "runx:harness_receipt:hrn_rcpt_x402-pay-approval_approve-spend",
      ]);
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  }, 30_000);

  it("keeps every Phase 1 mock eventuality either asserted or explicitly punch-listed", async () => {
    const punchlist = await readFile(resolvePunchlistPath(), "utf8");
    const allScenarioIds = Array.from({ length: 17 }, (_, index) => `P1.${index + 1}`);
    const missing = allScenarioIds.filter(
      (scenarioId) => !coveredScenarios.has(scenarioId) && !punchlist.includes(`| ${scenarioId} |`),
    );

    expect(missing).toEqual([]);
    for (const scenarioId of punchlistedScenarios) {
      const row = punchlist.split("\n").find((line) => line.startsWith(`| ${scenarioId} |`));
      expect(row, scenarioId).toBeDefined();
      expect(row, scenarioId).toContain("Open");
      expect(row, scenarioId).toContain("Missing");
    }
  });
});

function resolvePunchlistPath(): string {
  const path = mockScenarioPunchlistCandidates.find((candidate) => existsSync(candidate));
  if (!path) {
    throw new Error("missing x402-pay Phase 1 mock scenario punch-list spec");
  }
  return path;
}

function paymentDogfoodEnv(): NodeJS.ProcessEnv {
  const configured = process.env.RUNX_KERNEL_EVAL_BIN;
  const kernelBin = configured && configured.length > 0
    ? configured
    : existsSync(rustKernelBin)
      ? rustKernelBin
      : undefined;
  if (!kernelBin) {
    throw new Error(
      "x402 mock dogfood fixtures require RUNX_KERNEL_EVAL_BIN or a built crates/target/debug/runx binary.",
    );
  }
  return {
    ...process.env,
    RUNX_KERNEL_EVAL_BIN: kernelBin,
  };
}

async function runHistory(receiptDir: string, runxHome: string): Promise<{ readonly receipts: readonly Record<string, unknown>[] }> {
  const stdout = createMemoryStream();
  const stderr = createMemoryStream();
  const exitCode = await runCli(
    ["history", "--receipt-dir", receiptDir, "--json"],
    { stdin: process.stdin, stdout, stderr },
    {
      ...paymentDogfoodEnv(),
      RUNX_CWD: process.cwd(),
      RUNX_HOME: runxHome,
    },
  );
  expect(exitCode).toBe(0);
  expect(stderr.contents()).toBe("");
  return JSON.parse(stdout.contents()) as { readonly receipts: readonly Record<string, unknown>[] };
}

async function runHarnessJson(
  fixture: string,
  env: NodeJS.ProcessEnv = {},
): Promise<{ readonly receipt: Record<string, unknown>; readonly stdout: string }> {
  const stdout = createMemoryStream();
  const stderr = createMemoryStream();
  const exitCode = await runCli(
    ["harness", fixture, "--json"],
    { stdin: process.stdin, stdout, stderr },
    {
      ...paymentDogfoodEnv(),
      ...env,
      RUNX_CWD: process.cwd(),
    },
  );
  expect(exitCode).toBe(0);
  expect(stderr.contents()).toBe("");
  const raw = stdout.contents();
  return {
    receipt: requireRecord(JSON.parse(raw), "receipt"),
    stdout: raw,
  };
}

async function writeReceiptForHistory(receiptDir: string, receipt: Record<string, unknown>): Promise<void> {
  if (typeof receipt.id !== "string") {
    throw new Error("receipt.id must be a string.");
  }
  await mkdir(receiptDir, { recursive: true });
  await writeFile(path.join(receiptDir, `${receipt.id}.json`), `${JSON.stringify(receipt, null, 2)}\n`, "utf8");
}

function childReceiptUris(receipt: Record<string, unknown>): readonly string[] {
  const harness = requireRecord(receipt.harness, "receipt.harness");
  const refs = Array.isArray(harness.child_harness_receipt_refs) ? harness.child_harness_receipt_refs : [];
  return refs.map((ref) => requireRecord(ref, "child_harness_receipt_ref").uri).filter(isString);
}

function requireRecord(value: unknown, label: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error(`${label} must be an object.`);
  }
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isString(value: unknown): value is string {
  return typeof value === "string";
}

function createMemoryStream(): NodeJS.WriteStream & { contents: () => string } {
  let buffer = "";
  return {
    write: (chunk: string | Uint8Array) => {
      buffer += chunk.toString();
      return true;
    },
    contents: () => buffer,
  } as NodeJS.WriteStream & { contents: () => string };
}
