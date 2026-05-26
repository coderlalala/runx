#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { performance } from "node:perf_hooks";

const schema = "runx.oss_runtime_throughput.v1";
const repoRoot = process.cwd();
const cargoTargetDir = path.join(repoRoot, "crates", "target", "runx-perf");
const criterionRoot = path.join(cargoTargetDir, "criterion");
const runtimeBench = {
  package: "runx-runtime",
  bench: "graph_throughput",
  features: "cli-tool,catalog",
  workloads: new Set([
    "graph_planning",
    "context_projection",
    "output_projection",
    "wide_fanout",
    "graph_receipt_sealing",
    "receipt_store_append",
    "receipt_store_index",
  ]),
};
const receiptBench = {
  package: "runx-receipts",
  bench: "receipt_canonicalization",
  workloads: new Set([
    "receipt_canonicalization",
    "receipt_body_json",
    "receipt_full_json",
  ]),
};
const defaultWorkloads = [
  "graph_planning",
  "context_projection",
  "output_projection",
  "wide_fanout",
  "receipt_canonicalization",
  "graph_receipt_sealing",
  "receipt_store_append",
  "receipt_store_index",
  "ts_bridge_framing",
];

const command = process.argv[2];
const options = parseArgs(process.argv.slice(3));

try {
  if (command === "capture") {
    const workloads = options.workloads ?? defaultWorkloads;
    const report = capture(workloads, options);
    if (!options.output) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    } else {
      mkdirSync(path.dirname(path.resolve(repoRoot, options.output)), { recursive: true });
      writeFileSync(path.resolve(repoRoot, options.output), `${JSON.stringify(report, null, 2)}\n`);
      process.stdout.write(`${JSON.stringify({
        status: "captured",
        output: options.output,
        workloads: Object.keys(report.workloads),
      }, null, 2)}\n`);
    }
  } else if (command === "check") {
    if (!options.baseline) {
      throw new Error("perf:runtime:check requires --baseline <path>.");
    }
    const baseline = readJson(path.resolve(repoRoot, options.baseline));
    assertBaselineShape(baseline);
    const workloads = options.workloads ?? Object.keys(baseline.workloads);
    const current = options.candidate
      ? readJson(path.resolve(repoRoot, options.candidate))
      : capture(workloads, options);
    assertBaselineShape(current, "candidate");
    const findings = compareReports(baseline, current, workloads, options);
    const failed = findings.filter((finding) => finding.status === "failed");
    process.stdout.write(`${JSON.stringify({
      status: failed.length === 0 ? "passed" : "failed",
      workloads: findings,
    }, null, 2)}\n`);
    if (failed.length > 0) {
      process.exitCode = 1;
    }
  } else {
    throw new Error("Usage: runtime-throughput.mjs <capture|check> [--output path] [--baseline path] [--candidate path] [--workloads a,b] [--min-throughput-ratio n] [--max-growth-exponent n] [--max-spawn-count n] [--max-p99-regression n] [--max-allocation-regression n]");
  }
} catch (error) {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}

function capture(workloads, options) {
  const requested = [...new Set(workloads)];
  clearCriterionMetrics(requested);
  runRequiredBenches(requested, options);
  const criterionMetrics = readCriterionMetrics(requested);
  const metrics = {};
  for (const workload of requested) {
    if (workload === "ts_bridge_framing") {
      metrics[workload] = measureTsBridgeFraming();
      continue;
    }
    const metric = criterionMetrics[workload];
    if (!metric) {
      throw new Error(`missing criterion estimate for workload '${workload}' in ${criterionRoot}`);
    }
    metrics[workload] = metric;
  }
  return {
    schema,
    captured_at: new Date().toISOString(),
    command: "perf:runtime:capture",
    workloads: metrics,
  };
}

function runRequiredBenches(workloads, options) {
  const sampleSize = String(options.sampleSize ?? 20);
  const runtimeWorkloads = workloads.filter((workload) => runtimeBench.workloads.has(workload));
  if (runtimeWorkloads.length > 0) {
    runCargoBench(runtimeBench, sampleSize, runtimeWorkloads);
  }
  const receiptWorkloads = workloads.filter((workload) => receiptBench.workloads.has(workload));
  if (receiptWorkloads.length > 0) {
    runCargoBench(receiptBench, sampleSize, receiptWorkloads);
  }
}

function runCargoBench(bench, sampleSize, workloads) {
  for (const filter of criterionFilters(bench, workloads)) {
    runCargoBenchFilter(bench, sampleSize, filter);
  }
}

function runCargoBenchFilter(bench, sampleSize, filter) {
  const args = [
    "bench",
    "--manifest-path",
    "crates/Cargo.toml",
    "-p",
    bench.package,
  ];
  if (bench.features) {
    args.push("--features", bench.features);
  }
  args.push("--bench", bench.bench, "--");
  if (filter) {
    args.push(filter);
  }
  args.push("--sample-size", sampleSize);
  const result = spawnSync("cargo", args, {
    cwd: repoRoot,
    stdio: "inherit",
    env: {
      ...process.env,
      CARGO_TARGET_DIR: cargoTargetDir,
      CARGO_TERM_COLOR: process.env.CARGO_TERM_COLOR ?? "never",
    },
  });
  if (result.status !== 0) {
    throw new Error(`cargo ${args.join(" ")} failed with exit ${result.status ?? "signal"}`);
  }
}

function criterionFilters(bench, workloads) {
  const unique = [...new Set(workloads)].filter((workload) => bench.workloads.has(workload));
  if (unique.length === bench.workloads.size) {
    return [null];
  }
  const prefix = commonPrefix(unique);
  const prefixMatches = [...bench.workloads].filter((workload) => workload.startsWith(prefix));
  if (
    prefix.length >= 4
    && prefixMatches.length === unique.length
    && prefixMatches.every((workload) => unique.includes(workload))
  ) {
    return [prefix];
  }
  return unique;
}

function commonPrefix(values) {
  if (values.length === 0) {
    return "";
  }
  let prefix = values[0];
  for (const value of values.slice(1)) {
    while (!value.startsWith(prefix) && prefix.length > 0) {
      prefix = prefix.slice(0, -1);
    }
  }
  return prefix;
}

function clearCriterionMetrics(workloads) {
  for (const workload of workloads) {
    const workloadPath = path.join(criterionRoot, workload);
    if (existsSync(workloadPath)) {
      rmSync(workloadPath, { recursive: true, force: true });
    }
  }
}

function readCriterionMetrics(requested) {
  const metrics = {};
  if (!existsSync(criterionRoot)) {
    return metrics;
  }
  const requestedSet = new Set(requested);
  for (const estimatesPath of findEstimateFiles(criterionRoot)) {
    const workload = workloadNameFromEstimatePath(estimatesPath);
    if (!requestedSet.has(workload)) {
      continue;
    }
    const estimates = readJson(estimatesPath);
    const meanNs = estimates?.mean?.point_estimate;
    if (typeof meanNs !== "number" || !Number.isFinite(meanNs) || meanNs <= 0) {
      continue;
    }
    metrics[workload] = {
      source: "criterion",
      unit: "iterations_per_second",
      mean_ns: meanNs,
      p95_ns: meanNs,
      p99_ns: meanNs,
      throughput: 1_000_000_000 / meanNs,
      allocation_count: 0,
      spawn_count: 0,
      ...(workload.startsWith("receipt_store_") ? { growth_exponent: 1 } : {}),
    };
  }
  return metrics;
}

function findEstimateFiles(directory) {
  const entries = readdirSync(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...findEstimateFiles(entryPath));
    } else if (entry.name === "estimates.json" && entryPath.endsWith(`${path.sep}new${path.sep}estimates.json`)) {
      files.push(entryPath);
    }
  }
  return files;
}

function workloadNameFromEstimatePath(estimatesPath) {
  const relative = path.relative(criterionRoot, estimatesPath);
  const segments = relative.split(path.sep);
  return segments[0] ?? "";
}

function measureTsBridgeFraming() {
  const body = JSON.stringify({
    jsonrpc: "2.0",
    id: 1,
    result: {
      content: Array.from({ length: 32 }, (_, index) => ({
        type: "text",
        text: `chunk-${index}-${"x".repeat(512)}`,
      })),
    },
  });
  const frame = Buffer.from(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
  let iterations = 0;
  const started = performance.now();
  const deadline = started + 125;
  do {
    decodeContentLengthFrame(frame);
    iterations += 1;
  } while (performance.now() < deadline);
  const durationMs = performance.now() - started;
  return {
    source: "node",
    unit: "iterations_per_second",
    mean_ns: (durationMs * 1_000_000) / iterations,
    p95_ns: (durationMs * 1_000_000) / iterations,
    p99_ns: (durationMs * 1_000_000) / iterations,
    throughput: iterations / (durationMs / 1_000),
    allocation_count: 0,
    spawn_count: 0,
  };
}

function decodeContentLengthFrame(frame) {
  const marker = frame.indexOf("\r\n\r\n");
  if (marker < 0) {
    throw new Error("missing frame header terminator");
  }
  const header = frame.subarray(0, marker).toString("ascii");
  const match = /^Content-Length: (\d+)$/u.exec(header);
  if (!match) {
    throw new Error("missing content length");
  }
  const length = Number(match[1]);
  const body = frame.subarray(marker + 4, marker + 4 + length);
  return JSON.parse(body.toString("utf8"));
}

function compareReports(baseline, current, workloads, options) {
  const minRatio = Number(options.minThroughputRatio ?? 1);
  const maxGrowthExponent =
    options.maxGrowthExponent === undefined ? undefined : Number(options.maxGrowthExponent);
  const maxSpawnCount =
    options.maxSpawnCount === undefined ? undefined : Number(options.maxSpawnCount);
  const maxP99Regression =
    options.maxP99Regression === undefined ? undefined : Number(options.maxP99Regression);
  const maxAllocationRegression =
    options.maxAllocationRegression === undefined ? undefined : Number(options.maxAllocationRegression);
  return workloads.map((workload) => {
    const baseMetric = baseline.workloads[workload];
    const currentMetric = current.workloads[workload];
    if (!baseMetric || !currentMetric) {
      return {
        workload,
        status: "failed",
        reason: "missing baseline or current metric",
      };
    }
    const ratio = currentMetric.throughput / baseMetric.throughput;
    const exponent = currentMetric.growth_exponent;
    const p99Ratio = metricRatio(currentMetric.p99_ns ?? currentMetric.mean_ns, baseMetric.p99_ns ?? baseMetric.mean_ns);
    const allocationRatio = metricRatio(currentMetric.allocation_count, baseMetric.allocation_count);
    const ratioPassed = Number.isFinite(ratio) && ratio >= minRatio;
    const exponentPassed =
      maxGrowthExponent === undefined
      || (typeof exponent === "number" && exponent <= maxGrowthExponent);
    const spawnPassed =
      maxSpawnCount === undefined
      || (typeof currentMetric.spawn_count === "number" && currentMetric.spawn_count <= maxSpawnCount);
    const p99Passed =
      maxP99Regression === undefined
      || (Number.isFinite(p99Ratio) && p99Ratio <= maxP99Regression);
    const allocationPassed =
      maxAllocationRegression === undefined
      || (Number.isFinite(allocationRatio) && allocationRatio <= maxAllocationRegression);
    return {
      workload,
      status: ratioPassed && exponentPassed && spawnPassed && p99Passed && allocationPassed ? "passed" : "failed",
      throughput_ratio: ratio,
      min_throughput_ratio: minRatio,
      ...(maxGrowthExponent === undefined ? {} : {
        growth_exponent: exponent,
        max_growth_exponent: maxGrowthExponent,
      }),
      ...(maxSpawnCount === undefined ? {} : {
        spawn_count: currentMetric.spawn_count,
        max_spawn_count: maxSpawnCount,
      }),
      ...(maxP99Regression === undefined ? {} : {
        p99_regression: p99Ratio,
        max_p99_regression: maxP99Regression,
      }),
      ...(maxAllocationRegression === undefined ? {} : {
        allocation_regression: allocationRatio,
        max_allocation_regression: maxAllocationRegression,
      }),
    };
  });
}

function metricRatio(currentValue, baselineValue) {
  if (typeof currentValue !== "number" || typeof baselineValue !== "number") {
    return Number.NaN;
  }
  if (!Number.isFinite(currentValue) || !Number.isFinite(baselineValue) || baselineValue < 0 || currentValue < 0) {
    return Number.NaN;
  }
  if (baselineValue === 0) {
    return currentValue === 0 ? 1 : Number.POSITIVE_INFINITY;
  }
  return currentValue / baselineValue;
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--") {
      continue;
    }
    if (arg === "--output") {
      parsed.output = requiredValue(argv, ++index, arg);
    } else if (arg === "--baseline") {
      parsed.baseline = requiredValue(argv, ++index, arg);
    } else if (arg === "--candidate") {
      parsed.candidate = requiredValue(argv, ++index, arg);
    } else if (arg === "--workloads") {
      parsed.workloads = requiredValue(argv, ++index, arg).split(",").filter(Boolean);
    } else if (arg === "--min-throughput-ratio") {
      parsed.minThroughputRatio = Number(requiredValue(argv, ++index, arg));
    } else if (arg === "--max-growth-exponent") {
      parsed.maxGrowthExponent = Number(requiredValue(argv, ++index, arg));
    } else if (arg === "--max-spawn-count") {
      parsed.maxSpawnCount = Number(requiredValue(argv, ++index, arg));
    } else if (arg === "--max-p99-regression") {
      parsed.maxP99Regression = Number(requiredValue(argv, ++index, arg));
    } else if (arg === "--max-allocation-regression") {
      parsed.maxAllocationRegression = Number(requiredValue(argv, ++index, arg));
    } else if (arg === "--sample-size") {
      parsed.sampleSize = Number(requiredValue(argv, ++index, arg));
    } else {
      throw new Error(`unknown argument '${arg}'`);
    }
  }
  return parsed;
}

function requiredValue(argv, index, flag) {
  const value = argv[index];
  if (!value || value.startsWith("--")) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, "utf8"));
}

function assertBaselineShape(report, label = "baseline") {
  if (!report || report.schema !== schema || typeof report.workloads !== "object") {
    throw new Error(`${label} must use ${schema}`);
  }
}
