import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { chmod } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import process from "node:process";

import { describe, expect, it } from "vitest";

import { resolveNativeRunxBinary, spawnNativeRunx, streamNativeRunx } from "./native-runx.js";

describe("resolveNativeRunxBinary", () => {
  it("does not discover binaries from the caller cwd", async () => {
    const tempDir = mkdtempSync(path.join(os.tmpdir(), "runx-native-cwd-"));
    const fakeRunx = path.join(tempDir, "crates", "target", "debug", "runx");
    mkdirSync(path.dirname(fakeRunx), { recursive: true });
    writeFileSync(fakeRunx, "#!/bin/sh\nexit 99\n");
    await chmod(fakeRunx, 0o755);

    const previousCwd = process.cwd();
    try {
      process.chdir(tempDir);
      withUnsupportedNativePlatform(() => {
        expect(() => resolveNativeRunxBinary({})).toThrow("runx native package could not be verified");
      });
    } finally {
      process.chdir(previousCwd);
    }
  });

  it("fails closed instead of using RUNX_RUST_CLI_BIN as a packaged binary override", () => {
    withUnsupportedNativePlatform(() => {
      expect(() => resolveNativeRunxBinary({ RUNX_RUST_CLI_BIN: "/missing/runx" })).toThrow(
        "runx native package could not be verified",
      );
    });
  });

  it("fails when RUNX_DEV_RUST_CLI_BIN points at a missing binary", () => {
    expect(() => resolveNativeRunxBinary({ RUNX_DEV_RUST_CLI_BIN: "/missing/runx" })).toThrow(
      "RUNX_DEV_RUST_CLI_BIN does not exist",
    );
  });

  it("requires RUNX_DEV_RUST_CLI_BIN to be absolute", () => {
    expect(() => resolveNativeRunxBinary({ RUNX_DEV_RUST_CLI_BIN: "runx" })).toThrow(
      "RUNX_DEV_RUST_CLI_BIN must be an absolute path",
    );
  });
});

function withUnsupportedNativePlatform(callback: () => void): void {
  const platformDescriptor = Object.getOwnPropertyDescriptor(process, "platform");
  const archDescriptor = Object.getOwnPropertyDescriptor(process, "arch");
  try {
    Object.defineProperty(process, "platform", { configurable: true, value: "unsupported" });
    Object.defineProperty(process, "arch", { configurable: true, value: "unsupported" });
    callback();
  } finally {
    if (platformDescriptor) {
      Object.defineProperty(process, "platform", platformDescriptor);
    }
    if (archDescriptor) {
      Object.defineProperty(process, "arch", archDescriptor);
    }
  }
}

describe("spawnNativeRunx", () => {
  it("rejects when stdout exceeds the configured byte limit", async () => {
    await expect(
      spawnNativeRunx({
        command: process.execPath,
        args: ["-e", "process.stdout.write('abcdef')"],
        cwd: process.cwd(),
        env: {},
        timeoutMs: 5_000,
        maxOutputBytes: 3,
      }),
    ).rejects.toThrow("native runx stdout exceeded 3 bytes");
  });
});

describe("streamNativeRunx", () => {
  it("writes stdout and stderr before the native process exits", async () => {
    let stdout = "";
    let stderr = "";
    let sawEarlyOutput = () => {};
    const earlyOutput = new Promise<void>((resolve) => {
      sawEarlyOutput = resolve;
    });
    const resultPromise = streamNativeRunx(
      [
        "-e",
        "process.stdout.write('early-out'); process.stderr.write('early-err'); setTimeout(() => process.exit(7), 500);",
      ],
      {
        env: { RUNX_DEV_RUST_CLI_BIN: process.execPath },
        stdout: {
          write: (chunk: string | Uint8Array) => {
            stdout += chunk.toString();
            sawEarlyOutput();
            return true;
          },
        } as NodeJS.WritableStream,
        stderr: {
          write: (chunk: string | Uint8Array) => {
            stderr += chunk.toString();
            return true;
          },
        } as NodeJS.WritableStream,
        timeoutMs: 5_000,
      },
    );
    let settled = false;
    resultPromise.then(
      () => {
        settled = true;
      },
      () => {
        settled = true;
      },
    );

    await earlyOutput;

    expect(settled).toBe(false);
    await expect(resultPromise).resolves.toEqual({ status: 7, signal: null });
    expect(stdout).toBe("early-out");
    expect(stderr).toBe("early-err");
  });
});
