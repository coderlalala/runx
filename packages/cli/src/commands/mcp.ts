import { spawn } from "node:child_process";
import process from "node:process";

import { firstNonEmptyOrUndefined } from "../cli-util.js";

import type { CliIo } from "../index.js";

export interface McpCommandArgs {
  readonly mcpRefs?: readonly string[];
  readonly mcpNativeArgs?: readonly string[];
  readonly runner?: string;
  readonly receiptDir?: string;
}

export interface McpCommandDependencies {
  readonly resolveRegistryStoreForGraphs?: (env: NodeJS.ProcessEnv) => Promise<unknown>;
  readonly resolveDefaultReceiptDir?: (env: NodeJS.ProcessEnv) => string;
}

interface NativeMcpProcessOptions {
  readonly command: string;
  readonly args: readonly string[];
  readonly cwd: string;
  readonly env: NodeJS.ProcessEnv;
  readonly io: CliIo;
}

export async function handleMcpServeCommand(
  parsed: McpCommandArgs,
  io: CliIo,
  env: NodeJS.ProcessEnv,
  _deps?: McpCommandDependencies,
): Promise<void> {
  const skillRefs = parsed.mcpRefs ?? [];
  if (skillRefs.length === 0) {
    throw new Error("runx mcp serve requires at least one skill reference.");
  }

  await runNativeMcpProcess({
    command: resolveNativeRunxCommand(env),
    args: parsed.mcpNativeArgs ?? nativeMcpServeArgs(parsed, skillRefs),
    cwd: env.RUNX_CWD || process.cwd(),
    env: {
      ...process.env,
      ...env,
      RUNX_RUST_CLI: "1",
    },
    io,
  });
}

function nativeMcpServeArgs(parsed: McpCommandArgs, skillRefs: readonly string[]): readonly string[] {
  const args = ["mcp", "serve", ...skillRefs];
  if (parsed.receiptDir) {
    args.push("--receipt-dir", parsed.receiptDir);
  }
  if (parsed.runner) {
    args.push("--runner", parsed.runner);
  }
  return args;
}

function resolveNativeRunxCommand(env: NodeJS.ProcessEnv): string {
  const command = firstNonEmptyOrUndefined(
    env.RUNX_RUST_CLI_BIN,
    env.RUNX_MCP_NATIVE_BIN,
    env.RUNX_KERNEL_EVAL_BIN,
  );
  if (!command) {
    throw new Error("runx mcp serve requires RUNX_RUST_CLI_BIN, RUNX_MCP_NATIVE_BIN, or RUNX_KERNEL_EVAL_BIN.");
  }
  return command;
}

function runNativeMcpProcess(options: NativeMcpProcessOptions): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(options.command, options.args, {
      cwd: options.cwd,
      env: options.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    let settled = false;
    let stderr = "";

    const cleanup = (): void => {
      options.io.stdin.unpipe(child.stdin);
      child.stdout.unpipe(options.io.stdout);
      child.stderr.unpipe(options.io.stderr);
    };

    const rejectOnce = (error: Error): void => {
      if (settled) return;
      settled = true;
      cleanup();
      reject(error);
    };

    child.stdin.on("error", (error: NodeJS.ErrnoException) => {
      if (error.code === "EPIPE" || error.code === "ERR_STREAM_DESTROYED") {
        return;
      }
      rejectOnce(new Error(`Native MCP serve stdin failed: ${error.message}`));
    });
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      stderr += chunk;
    });
    child.stdout.pipe(options.io.stdout, { end: false });
    child.stderr.pipe(options.io.stderr, { end: false });
    options.io.stdin.pipe(child.stdin);

    child.on("error", (error) => {
      rejectOnce(new Error(`Failed to spawn native MCP command '${options.command}': ${error.message}`));
    });
    child.on("close", (status, signal) => {
      if (settled) return;
      settled = true;
      cleanup();
      if (signal) {
        reject(new Error(`Native MCP serve exited from signal ${signal}.`));
        return;
      }
      if (status !== 0) {
        reject(new Error(nativeMcpExitMessage(status, stderr)));
        return;
      }
      resolve();
    });
  });
}

function nativeMcpExitMessage(status: number | null, stderr: string): string {
  const details = stderr.trim();
  return `Native MCP serve failed with exit ${status ?? "unknown"}${details ? `: ${details}` : "."}`;
}
