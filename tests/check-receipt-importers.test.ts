import { describe, expect, it } from "vitest";

import {
  hasDeletionBlockers,
  scanFile,
  summarizeReceiptAudit,
  type ReceiptAuditReport,
} from "../scripts/check-receipt-importers.js";

describe("receipt importer audit classifier", () => {
  it("classifies live @runxhq/core/receipts imports as deletion blockers", () => {
    const findings = scanFile(
      "packages/runtime-local/src/runner-local/approval.ts",
      `import { writeLocalReceipt, type LocalSkillReceipt } from "@runxhq/core/receipts";\n`,
    );

    expect(findings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "retired_receipt_import",
          classification: "active_blocker",
          token: "@runxhq/core/receipts",
        }),
        expect.objectContaining({
          kind: "retired_receipt_type",
          classification: "active_blocker",
          token: "LocalSkillReceipt",
        }),
      ]),
    );
  });

  it("separates generated stale artifacts and archived fixtures from live blockers", () => {
    expect(scanFile("scripts/generate-rust-skill-fixtures.ts", `kind: "skill_execution",\n`)).toEqual([
      expect.objectContaining({
        kind: "retired_receipt_shape",
        classification: "generated_stale_artifact",
      }),
    ]);

    expect(scanFile("fixtures/runtime/skills/issue-intake/metadata.json", `"skill_name": "issue-intake"\n`)).toEqual([
      expect.objectContaining({
        kind: "retired_receipt_shape",
        classification: "fixture_archive",
      }),
    ]);
  });

  it("marks canonical harness receipt references as migrated evidence", () => {
    expect(scanFile("packages/contracts/src/schemas/spine.ts", `schema: "runx.harness_receipt.v1",\n`)).toEqual([
      expect.objectContaining({
        kind: "harness_receipt_shape",
        classification: "migrated",
      }),
    ]);
  });

  it("summarizes blockers for deletion-gate mode", () => {
    const report: ReceiptAuditReport = {
      workspaceRoot: "/workspace",
      scannedFiles: 2,
      cloudSibling: "not_found",
      findings: [
        ...scanFile("packages/core/package.json", `"./receipts": {\n`),
        ...scanFile("scripts/generate-rust-contract-fixtures.ts", `kind: "skill_execution",\n`),
      ],
    };

    expect(summarizeReceiptAudit(report)).toMatchObject({
      active_blocker: 1,
      generated_stale_artifact: 1,
    });
    expect(hasDeletionBlockers(report)).toBe(true);
  });
});
