import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { runCli } from "../packages/cli/src/index.js";
import { resolveRunxBinary } from "./runx-binary.js";

describe("harness CLI", () => {
  it("runs a skill harness fixture non-interactively", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-harness-cli-"));
    const stdout = createMemoryStream();
    const stderr = createMemoryStream();

    try {
      const exitCode = await runCli(
        ["harness", "fixtures/harness/echo-skill.yaml", "--json"],
        { stdin: process.stdin, stdout, stderr },
        harnessCliEnv(tempDir),
      );

      expect(exitCode).toBe(0);
      const receipt = JSON.parse(stdout.contents()) as {
        schema?: string;
        subject?: { kind?: string; ref?: { type?: string; uri?: string } };
        seal?: { disposition?: string; reason_code?: string };
      };
      expect(receipt).toMatchObject({
        schema: "runx.receipt.v1",
        subject: { kind: "skill", ref: { type: "harness", uri: "hrn_echo-skill_echo" } },
        seal: { disposition: "closed", reason_code: "process_closed" },
      });
      expect(stderr.contents()).toBe("");
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });

  it("rejects inline skill directories on the native harness CLI", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-harness-inline-cli-"));
    const stdout = createMemoryStream();
    const stderr = createMemoryStream();

    try {
      const exitCode = await runCli(
        ["harness", "skills/evolve", "--json"],
        { stdin: process.stdin, stdout, stderr },
        harnessCliEnv(tempDir),
      );

      expect(exitCode).toBe(1);
      expect(stdout.contents()).toBe("");
      expect(stderr.contents()).toContain("native harness replay failed");
      expect(stderr.contents()).toContain("failed to read harness fixture");
      expect(stderr.contents()).toContain("Is a directory");
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });
});

function harnessCliEnv(tempDir: string): NodeJS.ProcessEnv {
  return {
    ...process.env,
    RUNX_CWD: process.cwd(),
    RUNX_HOME: path.join(tempDir, "home"),
    RUNX_RUST_CLI_BIN: resolveRunxBinary(),
  };
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
