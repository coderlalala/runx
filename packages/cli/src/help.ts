import type { Writable } from "node:stream";

import { theme } from "./ui.js";

const BANNER_LINES = [
  "_______ __ __  ____ ___  ___",
  "\\_  __ \\  |  \\/    \\\\  \\/  /",
  " |  | \\/  |  /   |  \\>    < ",
  " |__|  |____/|___|  /__/\\_ \\",
  "                  \\/      \\/",
];

export function isHelpRequest(argv: readonly string[]): boolean {
  return argv.length === 1 && (argv[0] === "--help" || argv[0] === "-h");
}

export function writeBanner(stream: Writable, env: NodeJS.ProcessEnv): void {
  const t = theme(stream, env);
  const gradient = t.on
    ? ["\u001b[38;5;201m", "\u001b[38;5;207m", "\u001b[38;5;177m", "\u001b[38;5;147m", "\u001b[38;5;117m"]
    : ["", "", "", "", ""];
  const lines: string[] = [""];
  for (let index = 0; index < BANNER_LINES.length; index += 1) {
    lines.push(`  ${gradient[index]}${t.bold}${BANNER_LINES[index]}${t.reset}`);
  }
  lines.push("");
  stream.write(`${lines.join("\n")}\n`);
}

export function writeUsage(stream: Writable, env: NodeJS.ProcessEnv = process.env): void {
  const t = theme(stream, env);
  const wantsBanner = t.on || env.RUNX_BANNER === "1";
  if (wantsBanner) {
    writeBanner(stream, env);
  }
  stream.write(
    [
      "Usage:",
      "  runx <command> [args]",
      "  runx --help",
      "  runx --version",
      "",
      "Native help is authoritative:",
      "  runx --help",
      "  runx <command> --help",
      "",
      "This TypeScript entrypoint is a package launcher and test harness only; command grammar lives in the Rust binary.",
      "",
    ].join("\n"),
  );
}
