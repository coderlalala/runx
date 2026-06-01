import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { parseRunnerManifestYaml, validateRunnerManifest } from "@runxhq/core/parser";

import { officialSkillVisibleForCatalog } from "./skill-refs.js";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const privatePaymentStepNodes = [
  "pay-quote",
  "pay-reserve",
  "pay-fulfill-rail",
  "pay-recover",
  "charge-challenge",
  "charge-price",
  "charge-verify",
  "refund-quote",
  "refund-reserve",
  "refund-recover",
];
const privateMockFaces = ["mock-charge", "mock-pay", "mock-refund"];
const publicPaymentFaces = ["x402-pay", "stripe-pay", "stripe-charge", "stripe-refund", "mpp-pay", "mpp-charge", "mpp-refund"];

describe("official skill catalog exposure", () => {
  it("hides mock payment faces unless the dev catalog is explicitly enabled", () => {
    expect(officialSkillVisibleForCatalog("runx/mock-pay", {})).toBe(false);
    expect(officialSkillVisibleForCatalog("runx/mock-charge", {})).toBe(false);
    expect(officialSkillVisibleForCatalog("runx/mock-refund", {})).toBe(false);
    expect(
      officialSkillVisibleForCatalog("runx/mock-pay", {
        RUNX_DEV_CATALOG: "1",
      }),
    ).toBe(true);
  });

  it("keeps real payment faces visible", () => {
    expect(officialSkillVisibleForCatalog("runx/x402-pay", {})).toBe(true);
    expect(officialSkillVisibleForCatalog("runx/stripe-pay", {})).toBe(true);
    expect(officialSkillVisibleForCatalog("runx/mpp-pay", {})).toBe(true);
  });

  it("keeps payment catalog visibility explicit in first-party runner manifests", () => {
    for (const skill of privatePaymentStepNodes) {
      expect(catalogVisibility(skill), skill).toBe("private");
    }
    for (const skill of privateMockFaces) {
      expect(catalogVisibility(skill), skill).toBe("private");
    }
    for (const skill of publicPaymentFaces) {
      expect(catalogVisibility(skill), skill).toBe("public");
    }
  });
});

function catalogVisibility(skill: string): string | undefined {
  const manifestPath = path.join(repoRoot, "skills", skill, "X.yaml");
  const manifest = validateRunnerManifest(parseRunnerManifestYaml(readFileSync(manifestPath, "utf8")));
  return manifest.catalog?.visibility;
}
