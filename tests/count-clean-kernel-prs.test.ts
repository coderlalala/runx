import path from "node:path";

import { describe, expect, it } from "vitest";

import {
  analyzeCleanKernelPrs,
  runCountCleanKernelPrsCli,
} from "../scripts/count-clean-kernel-prs.js";

const fixturePath = path.join(process.cwd(), "tests", "fixtures", "clean-kernel-prs.json");

describe("clean kernel PR counter", () => {
  it("counts only TS kernel PRs and deliberate kernel fixture refreshes with passing evidence", async () => {
    const result = await runCountCleanKernelPrsCli(["--fixture", fixturePath, "--min", "3"]);
    const report = JSON.parse(result.stdout) as ReturnType<typeof analyzeCleanKernelPrs>;

    expect(result).toMatchObject({ exitCode: 0, stderr: "" });
    expect(report.count).toBe(4);
    expect(report.counting.map((entry) => entry.number)).toEqual([101, 102, 103, 108]);
    expect(report.counting.map((entry) => entry.reason)).toEqual([
      "ts_kernel",
      "ts_kernel",
      "kernel_fixture_refresh",
      "kernel_fixture_refresh",
    ]);
  });

  it("requires explicit classification for mixed TS kernel and kernel fixture refresh PRs", async () => {
    const result = await runCountCleanKernelPrsCli(["--fixture", fixturePath, "--min", "3"]);
    const report = JSON.parse(result.stdout) as ReturnType<typeof analyzeCleanKernelPrs>;

    expect(report.counting).toEqual(expect.arrayContaining([
      expect.objectContaining({
        number: 108,
        reason: "kernel_fixture_refresh",
        passing_evidence: true,
      }),
    ]));
    expect(report.non_counting).toEqual(expect.arrayContaining([
      expect.objectContaining({
        number: 109,
        reason: "outside_kernel_promotion_scope",
        passing_evidence: true,
      }),
    ]));
  });

  it("reports Rust-only and parser-only PRs as non-counting", async () => {
    const result = await runCountCleanKernelPrsCli(["--fixture", fixturePath, "--min", "3"]);
    const report = JSON.parse(result.stdout) as ReturnType<typeof analyzeCleanKernelPrs>;

    expect(report.non_counting).toEqual(expect.arrayContaining([
      expect.objectContaining({
        number: 104,
        reason: "rust_only",
        passing_evidence: true,
      }),
      expect.objectContaining({
        number: 105,
        reason: "parser_only",
        passing_evidence: true,
      }),
      expect.objectContaining({
        number: 106,
        reason: "missing_passing_evidence",
        passing_evidence: false,
      }),
      expect.objectContaining({
        number: 107,
        reason: "outside_kernel_promotion_scope",
        passing_evidence: true,
      }),
      expect.objectContaining({
        number: 110,
        reason: "missing_passing_evidence",
        passing_evidence: false,
      }),
    ]));
  });

  it("does not let evidence-object pass tokens override skipped or ambiguous required checks", () => {
    const report = analyzeCleanKernelPrs({
      advisory_start: "local advisory baseline",
      prs: [
        {
          number: 201,
          title: "policy: ambiguous check state",
          files: ["packages/core/src/policy/public-work.ts"],
          evidence: {
            status: "passed",
            checks: [
              { name: "test:fast", conclusion: "success" },
              { name: "rust:kernel-parity", conclusion: "skipped" },
            ],
          },
        },
        {
          number: 202,
          title: "policy: explicit passing checks",
          files: ["packages/core/src/policy/public-work.ts"],
          evidence: {
            status: "passed",
            checks: [
              { name: "test:fast", conclusion: "success" },
              { name: "rust:kernel-parity", conclusion: "success" },
            ],
          },
        },
        {
          number: 203,
          title: "policy: audited fixture operator evidence",
          files: ["packages/core/src/policy/public-work.ts"],
          passing_evidence: true,
          evidence: {
            status: "renamed",
          },
        },
      ],
    });

    expect(report.counting.map((entry) => entry.number)).toEqual([202, 203]);
    expect(report.non_counting).toEqual(expect.arrayContaining([
      expect.objectContaining({
        number: 201,
        reason: "missing_passing_evidence",
        passing_evidence: false,
      }),
    ]));
  });

  it("requires explicit advisory start evidence from the fixture or CLI", () => {
    expect(() => analyzeCleanKernelPrs({ prs: [] })).toThrow(
      /missing advisory start evidence/,
    );

    expect(analyzeCleanKernelPrs({
      advisoryStart: "local advisory baseline",
      pullRequests: [],
    })).toMatchObject({
      advisory_start: "local advisory baseline",
      advisory_start_source: "fixture",
    });

    expect(analyzeCleanKernelPrs(
      { prs: [] },
      { advisoryStart: "cli advisory baseline" },
    )).toMatchObject({
      advisory_start: "cli advisory baseline",
      advisory_start_source: "cli",
    });
  });

  it("fails closed when the requested minimum is not met", async () => {
    const result = await runCountCleanKernelPrsCli(["--fixture", fixturePath, "--min", "5"]);
    const report = JSON.parse(result.stdout) as ReturnType<typeof analyzeCleanKernelPrs>;

    expect(result.exitCode).toBe(1);
    expect(result.stderr).toBe("clean kernel PR count 4 is below required minimum 5\n");
    expect(report).toMatchObject({
      count: 4,
      minimum: 5,
      meets_minimum: false,
    });
  });
});
