import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const inputs = readInputs();
const rawDiff = unwrapInput(inputs.diff);
if (typeof rawDiff !== "string") {
  process.stderr.write("secret-catcher: diff input must be a string\n");
  process.exit(2);
}
const diff = rawDiff;
const scanContext = parseContext(unwrapInput(inputs.scan_context));

const detectors = [
  {
    type: "aws_access_key_id",
    severity: "high",
    pattern: /\bAKIA[0-9A-Z]{16}\b/g,
    replacement: "[REDACTED:aws_access_key_id]",
  },
  {
    type: "github_token",
    severity: "high",
    pattern: /\bgh[pousr]_[A-Za-z0-9_]{20,}\b/g,
    replacement: "[REDACTED:github_token]",
  },
  {
    type: "openai_api_key",
    severity: "high",
    pattern: /\bsk-(?:proj-|live-)?[A-Za-z0-9_-]{32,}\b/g,
    replacement: "[REDACTED:openai_api_key]",
  },
  {
    type: "slack_token",
    severity: "high",
    pattern: /\bxox[baprs]-[A-Za-z0-9-]{20,}\b/g,
    replacement: "[REDACTED:slack_token]",
  },
  {
    type: "private_key_block",
    severity: "high",
    pattern: /-----BEGIN [A-Z ]*PRIVATE KEY-----/g,
    replacement: "[REDACTED:private_key_block]",
  },
  {
    type: "generic_assignment_secret",
    severity: "medium",
    pattern: /\b(?:api[_-]?key|secret|token|password)\b\s*[:=]\s*["']?([A-Za-z0-9_./+=-]{20,})["']?/gi,
    replacement: "[REDACTED:generic_secret]",
  },
];

const findings = [];
const replacements = [];
let currentFile = null;
let oldLine = 0;
let newLine = 0;

const lines = diff.split(/\r?\n/);

for (let index = 0; index < lines.length; index += 1) {
  const rawLine = lines[index];

  if (rawLine.startsWith("+++ ")) {
    currentFile = normalizeFile(rawLine.slice(4));
    continue;
  }

  const hunk = /^@@\s+-(\d+)(?:,\d+)?\s+\+(\d+)(?:,\d+)?\s+@@/.exec(rawLine);
  if (hunk) {
    oldLine = Number(hunk[1]);
    newLine = Number(hunk[2]);
    continue;
  }

  const diffSide = classifyDiffLine(rawLine);
  const effectiveLine = diffSide === "removed" ? oldLine : newLine;
  const content = diffSide === "context" ? rawLine : rawLine.slice(1);

  if (diffSide === "added" || diffSide === "context" || !rawLine.startsWith("-")) {
    scanLine(content, {
      file: currentFile,
      line: effectiveLine || index + 1,
      diff_side: diffSide,
    });
  }

  if (rawLine.startsWith("+") && !rawLine.startsWith("+++ ")) {
    newLine += 1;
  } else if (rawLine.startsWith("-") && !rawLine.startsWith("--- ")) {
    oldLine += 1;
  } else if (!rawLine.startsWith("\\") && !rawLine.startsWith("diff ") && !rawLine.startsWith("index ")) {
    oldLine += oldLine ? 1 : 0;
    newLine += newLine ? 1 : 0;
  }
}

const block = findings.some((finding) => finding.severity === "high" || finding.severity === "medium");

const result = {
  schema: "runx.secret_catcher.result.v1",
  package: "secret-catcher",
  summary: block
    ? `Detected ${findings.length} credential-like span(s); block is true.`
    : "No credential-like spans detected; block is false.",
  scan_context: scanContext,
  block,
  findings,
  redaction_proposal: {
    strategy: "replace_matched_span_only",
    replacements,
  },
  safety: {
    raw_secret_echoed: false,
    repository_mutated: false,
    scanned_removed_lines: false,
  },
};

process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);

function scanLine(content, lineInfo) {
  for (const detector of detectors) {
    detector.pattern.lastIndex = 0;
    let match;
    while ((match = detector.pattern.exec(content)) !== null) {
      const secretValue = match[1] ?? match[0];
      const start = match.index + (match[0].indexOf(secretValue));
      const end = start + secretValue.length;
      const evidenceHash = sha256(secretValue);

      findings.push({
        type: detector.type,
        severity: detector.severity,
        location: {
          file: lineInfo.file ?? "unknown",
          line: lineInfo.line,
          column_start: start + 1,
          column_end: end + 1,
          diff_side: lineInfo.diff_side,
        },
        evidence_hash: `sha256:${evidenceHash}`,
      });

      replacements.push({
        location: {
          file: lineInfo.file ?? "unknown",
          line: lineInfo.line,
          column_start: start + 1,
          column_end: end + 1,
        },
        replacement: detector.replacement,
      });
    }
  }
}

function readInputs() {
  if (process.env.RUNX_INPUTS_JSON) {
    return JSON.parse(process.env.RUNX_INPUTS_JSON);
  }
  if (process.env.RUNX_INPUTS_PATH) {
    return JSON.parse(readFileSync(process.env.RUNX_INPUTS_PATH, "utf8"));
  }
  return {
    diff: process.env.RUNX_INPUT_DIFF ?? "",
    scan_context: process.env.RUNX_INPUT_SCAN_CONTEXT ?? "",
  };
}

function unwrapInput(value) {
  if (value && typeof value === "object" && !Array.isArray(value) && Object.hasOwn(value, "value")) {
    return value.value;
  }
  return value;
}

function parseContext(value) {
  if (value == null || value === "") {
    return null;
  }
  if (typeof value !== "string") {
    return value;
  }
  try {
    return JSON.parse(value);
  } catch {
    return { note: value };
  }
}

function classifyDiffLine(line) {
  if (line.startsWith("+") && !line.startsWith("+++ ")) {
    return "added";
  }
  if (line.startsWith("-") && !line.startsWith("--- ")) {
    return "removed";
  }
  return "context";
}

function normalizeFile(value) {
  return value.replace(/^b\//, "").trim() || null;
}

function sha256(value) {
  return createHash("sha256").update(String(value), "utf8").digest("hex");
}
