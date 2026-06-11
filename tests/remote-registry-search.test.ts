import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { runCli } from "../packages/cli/src/index.js";

describe("remote registry search", () => {
  it("searches the hosted public registry through the native registry boundary", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-remote-registry-search-"));
    const nativeBin = path.join(tempDir, "runx-native");
    const stdout = createMemoryStream();
    const stderr = createMemoryStream();

    try {
      await writeNodeCommand(
        nativeBin,
        `
const args = process.argv.slice(2);
if (args.join(" ") !== "registry search sourcey --json") {
  process.stderr.write("unexpected args: " + args.join(" ") + "\\n");
  process.exit(2);
}
if (process.env.RUNX_REGISTRY_URL !== "https://runx.example.test") {
  process.stderr.write("missing registry env\\n");
  process.exit(2);
}
process.stdout.write(JSON.stringify({
  status: "success",
  registry: {
    action: "search",
    source: "remote",
    query: "sourcey",
    results: [
      {
        skill_id: "acme/sourcey",
        owner: "acme",
        name: "sourcey",
        description: "Generate docs from repo evidence.",
        version: "1.0.0",
        source: "runx-registry",
        source_label: "runx registry",
        source_type: "agent",
        profile_mode: "profiled",
        runner_names: ["agent", "sourcey"],
        required_scopes: [],
        tags: ["docs"],
        trust_tier: "community",
        trust_signals: [],
        install_command: "runx add acme/sourcey@1.0.0 --registry https://runx.example.test",
        run_command: "runx skill sourcey"
      }
    ]
  }
}, null, 2) + "\\n");
`,
      );

      const exitCode = await runCli(
        ["skill", "search", "sourcey", "--json"],
        { stdin: process.stdin, stdout, stderr },
        {
          ...process.env,
          RUNX_CWD: process.cwd(),
          RUNX_DEV_RUST_CLI_BIN: nativeBin,
          RUNX_REGISTRY_URL: "https://runx.example.test",
        },
      );

      expect(exitCode).toBe(0);
      expect(stderr.contents()).toBe("");
      expect(JSON.parse(stdout.contents())).toMatchObject({
        status: "success",
        query: "sourcey",
        results: [
          {
            skill_id: "acme/sourcey",
            source: "runx-registry",
            source_label: "runx registry",
            trust_tier: "community",
            profile_mode: "profiled",
            runner_names: ["agent", "sourcey"],
            add_command: "runx add acme/sourcey@1.0.0 --registry https://runx.example.test",
          },
        ],
      });
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
