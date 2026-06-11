import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { runCli } from "../packages/cli/src/index.js";

describe("skill-search CLI", () => {
  it("returns normalized runx registry results as JSON", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-skill-search-"));
    const registryDir = path.join(tempDir, "registry");
    const stdout = createMemoryStream();
    const stderr = createMemoryStream();

    try {
      const portableSkillDir = path.join(tempDir, "sourcey-portable");
      await mkdir(portableSkillDir, { recursive: true });
      await writeFile(path.join(portableSkillDir, "SKILL.md"), await readFile(path.resolve("skills/sourcey/SKILL.md"), "utf8"));

      const publishStdout = createMemoryStream();
      const publishStderr = createMemoryStream();
      await expect(
        runCli(
          ["skill", "publish", portableSkillDir, "--owner", "acme", "--version", "1.0.0", "--registry", registryDir, "--json"],
          { stdin: process.stdin, stdout: publishStdout, stderr: publishStderr },
          { ...process.env, RUNX_CWD: process.cwd() },
        ),
      ).resolves.toBe(0);
      expect(publishStderr.contents()).toBe("");

      const exitCode = await runCli(
        ["skill", "search", "sourcey", "--json"],
        { stdin: process.stdin, stdout, stderr },
        {
          ...process.env,
          RUNX_CWD: process.cwd(),
          RUNX_REGISTRY_DIR: registryDir,
          RUNX_REGISTRY_URL: "https://runx.example.test",
        },
      );

      expect(exitCode).toBe(0);
      expect(stderr.contents()).toBe("");
      const report = JSON.parse(stdout.contents()) as {
        status: string;
        query: string;
        results: {
          skill_id: string;
          source: string;
          source_label: string;
          trust_tier: string;
          add_command: string;
          profile_mode: string;
          runner_names: string[];
          profile_digest?: string;
        }[];
      };
      expect(report).toMatchObject({
        status: "success",
        query: "sourcey",
      });
      expect(report.results).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            skill_id: "acme/sourcey",
            source: "runx-registry",
            source_label: "runx registry",
            trust_tier: "community",
            profile_mode: "portable",
            runner_names: [],
            add_command: "runx add acme/sourcey@1.0.0 --registry https://runx.example.test",
          }),
        ]),
      );
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });

  it("returns no results for retired fixture marketplace source filters", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-skill-search-marketplace-"));
    const stdout = createMemoryStream();
    const stderr = createMemoryStream();

    try {
      const exitCode = await runCli(
        ["skill", "search", "sourcey", "--source", "fixture-marketplace", "--json"],
        { stdin: process.stdin, stdout, stderr },
        {
          ...process.env,
          RUNX_CWD: process.cwd(),
          RUNX_REGISTRY_DIR: path.join(tempDir, "registry"),
          RUNX_ENABLE_FIXTURE_MARKETPLACE: "1",
        },
      );

      expect(exitCode).toBe(0);
      expect(stderr.contents()).toBe("");
      const report = JSON.parse(stdout.contents()) as {
        results: {
          skill_id: string;
          source: string;
          source_label: string;
          trust_tier: string;
          profile_mode: string;
          runner_names: string[];
        }[];
      };
      expect(report.results).toEqual([]);
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });

  it("uses native registry search for registry source results", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-skill-search-rust-"));
    const registryBin = path.join(tempDir, "registry-search");
    const stdout = createMemoryStream();
    const stderr = createMemoryStream();

    try {
      await writeNodeCommand(
        registryBin,
        `
if (process.argv.slice(2).join(" ") !== "registry search sourcey --json") {
  process.stderr.write("unexpected args: " + process.argv.slice(2).join(" ") + "\\n");
  process.exit(2);
}
process.stdout.write(JSON.stringify({
  status: "success",
  registry: {
    action: "search",
    source: "local",
    query: "sourcey",
    results: [{
      skill_id: "rust/sourcey",
      name: "sourcey",
      owner: "rust",
      source: "runx-registry",
      source_label: "runx registry",
      source_type: "cli-tool",
      trust_tier: "community",
      required_scopes: [],
      tags: [],
      profile_mode: "portable",
      runner_names: [],
      install_command: "runx add rust/sourcey@1.0.0",
      run_command: "runx skill sourcey",
      version: "1.0.0"
    }]
  }
}, null, 2) + "\\n");
`,
      );

      const exitCode = await runCli(
        ["skill", "search", "sourcey", "--source", "registry", "--json"],
        { stdin: process.stdin, stdout, stderr },
        {
          ...process.env,
          RUNX_CWD: process.cwd(),
          RUNX_DEV_RUST_CLI_BIN: registryBin,
          RUNX_REGISTRY_DIR: path.join(tempDir, "unused-registry"),
        },
      );

      expect(exitCode).toBe(0);
      expect(stderr.contents()).toBe("");
      const report = JSON.parse(stdout.contents()) as {
        results: {
          skill_id: string;
          source: string;
        }[];
      };
      expect(report.results).toEqual([
        expect.objectContaining({
          skill_id: "rust/sourcey",
          source: "runx-registry",
        }),
      ]);
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });

  it("does not route retired fixture marketplace search through native registry search", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-skill-search-marketplace-rust-"));
    const registryBin = path.join(tempDir, "registry-search");
    const stdout = createMemoryStream();
    const stderr = createMemoryStream();

    try {
      await writeNodeCommand(
        registryBin,
        `
process.stderr.write("native registry search should not run for fixture marketplace\\n");
process.exit(2);
`,
      );

      const exitCode = await runCli(
        ["skill", "search", "sourcey", "--source", "fixture-marketplace", "--json"],
        { stdin: process.stdin, stdout, stderr },
        {
          ...process.env,
          RUNX_CWD: process.cwd(),
          RUNX_REGISTRY_DIR: path.join(tempDir, "registry"),
          RUNX_ENABLE_FIXTURE_MARKETPLACE: "1",
          RUNX_DEV_RUST_CLI_BIN: registryBin,
        },
      );

      expect(exitCode).toBe(0);
      expect(stderr.contents()).toBe("");
      const report = JSON.parse(stdout.contents()) as {
        results: {
          skill_id: string;
          source: string;
        }[];
      };
      expect(report.results).toEqual([]);
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });
});

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

async function writeNodeCommand(commandPath: string, source: string): Promise<void> {
  const scriptPath = `${commandPath}.mjs`;
  await writeFile(scriptPath, source, "utf8");
  await writeFile(commandPath, `#!/bin/sh\nexec ${JSON.stringify(process.execPath)} ${JSON.stringify(scriptPath)} "$@"\n`, "utf8");
  await chmod(commandPath, 0o755);
}
