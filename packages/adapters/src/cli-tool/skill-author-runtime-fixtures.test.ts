import { describe, expect, it } from "vitest";
import { mkdtemp, rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";

import { invokeCliTool, type CliToolSandbox } from "./index.js";

interface FixtureCase {
  readonly id: string;
  readonly mode: string;
  readonly cwd?: string;
  readonly input_mode?: "stdin" | "args" | "none";
  readonly large_input_bytes?: number;
  readonly timeout_seconds: number;
  readonly sandbox: {
    readonly profile: CliToolSandbox["profile"];
    readonly cwd_policy?: CliToolSandbox["cwdPolicy"];
  };
  readonly inputs: Record<string, unknown>;
  readonly expected: {
    readonly status: "sealed" | "failure";
    readonly stdout_bytes?: number;
    readonly stdout_json?: unknown;
    readonly stderr_contains?: string;
    readonly max_duration_ms?: number;
    readonly sentinel_absent_after_ms?: number;
  };
}

interface FixtureSuite {
  readonly probe: string;
  readonly skill_directory: string;
  readonly cases: readonly FixtureCase[];
}

const fixtureRoot = path.resolve("fixtures/skill-author-runtime");
const suite = JSON.parse(readFileSync(path.join(fixtureRoot, "cases.json"), "utf8")) as FixtureSuite;
const probePath = path.join(fixtureRoot, suite.probe);
const skillDirectory = path.join(fixtureRoot, suite.skill_directory);

describe("skill author runtime fixtures", () => {
  for (const fixture of suite.cases) {
    it(`matches the v1 author contract for ${fixture.id}`, async () => {
      const tempDir = await mkdtemp(path.join(os.tmpdir(), `runx-${fixture.id}-`));
      const sentinelPath = path.join(tempDir, "sentinel");
      try {
        const started = performance.now();
        const result = await invokeCliTool({
          source: {
            command: "node",
            args: [probePath, fixture.mode],
            cwd: fixture.cwd,
            inputMode: fixture.input_mode,
            timeoutSeconds: fixture.timeout_seconds,
            sandbox: {
              profile: fixture.sandbox.profile,
              cwdPolicy: fixture.sandbox.cwd_policy,
            },
          },
          inputs: fixtureInputs(fixture, sentinelPath),
          skillDirectory,
          env: {
            PATH: process.env.PATH,
            RUNX_CWD: fixtureRoot,
            RUNX_SENTINEL_PATH: sentinelPath,
            TMPDIR: tempDir,
          },
        });
        const durationMs = performance.now() - started;

        expect(result.status).toBe(fixture.expected.status);
        if (fixture.expected.stderr_contains !== undefined) {
          expect(result.stderr).toContain(fixture.expected.stderr_contains);
        } else {
          expect(result.stderr).toBe("");
        }
        if (fixture.expected.stdout_json !== undefined) {
          expect(JSON.parse(result.stdout)).toEqual(fixture.expected.stdout_json);
        }
        if (fixture.expected.stdout_bytes !== undefined) {
          expect(Buffer.byteLength(result.stdout, "utf8")).toBe(fixture.expected.stdout_bytes);
        }
        if (fixture.expected.max_duration_ms !== undefined) {
          expect(durationMs).toBeLessThan(fixture.expected.max_duration_ms);
        }
        if (fixture.expected.sentinel_absent_after_ms !== undefined) {
          await sleep(fixture.expected.sentinel_absent_after_ms);
          await expectFileMissing(sentinelPath);
        }
      } finally {
        await rm(tempDir, { recursive: true, force: true });
      }
    });
  }
});

function fixtureInputs(fixture: FixtureCase, sentinelPath: string): Record<string, unknown> {
  const inputs = {
    ...fixture.inputs,
  };
  if (fixture.large_input_bytes !== undefined) {
    inputs.large = "x".repeat(fixture.large_input_bytes);
  }
  if (fixture.mode === "timeout-descendant") {
    inputs.sentinel_path = sentinelPath;
  }
  return inputs;
}

async function expectFileMissing(filePath: string): Promise<void> {
  try {
    readFileSync(filePath, "utf8");
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && error.code === "ENOENT") {
      return;
    }
    throw error;
  }
  throw new Error(`expected ${filePath} to be absent`);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
