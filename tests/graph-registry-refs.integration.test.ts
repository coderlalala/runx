import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { materializeRegistrySkill } from "@runxhq/runtime-local";
import { parseSkillMarkdown, validateSkill } from "@runxhq/core/parser";
import { createFileRegistryStore } from "./registry-fixtures.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REAL_REGISTRY_ROOT = path.resolve(HERE, "..", "..", "cloud", ".data", "runx-registry");
const REGISTRY_AVAILABLE = existsSync(path.join(REAL_REGISTRY_ROOT, "runx", "scafld"));
const currentPaymentRegistrySkillIds = [
  "runx/charge-challenge",
  "runx/charge-price",
  "runx/charge-verify",
  "runx/dispute-respond",
  "runx/mock-charge",
  "runx/mock-pay",
  "runx/mock-refund",
  "runx/mpp-charge",
  "runx/mpp-pay",
  "runx/mpp-refund",
  "runx/pay-fulfill-rail",
  "runx/pay-quote",
  "runx/pay-recover",
  "runx/pay-reserve",
  "runx/refund-quote",
  "runx/refund-recover",
  "runx/refund-reserve",
  "runx/stripe-charge",
  "runx/stripe-pay",
  "runx/stripe-refund",
  "runx/x402-pay",
] as const;
const retiredPaymentRegistrySkillIds = [
  "runx/payment-authorize-reserve",
  "runx/payment-charge",
  "runx/payment-charge-challenge",
  "runx/payment-charge-price",
  "runx/payment-charge-verify",
  "runx/payment-execute",
  "runx/payment-execution",
  "runx/payment-fulfill",
  "runx/payment-fulfill-rail",
  "runx/payment-quote",
  "runx/payment-quote-preflight",
  "runx/payment-rail-mock",
  "runx/payment-recover",
  "runx/payment-recover-inspect",
  "runx/payment-refund",
  "runx/payment-refund-quote",
  "runx/payment-refund-recover",
  "runx/payment-refund-reserve",
  "runx/payment-reserve",
  "runx/x402-charge",
  "runx/x402-refund",
] as const;

function isPaymentRegistrySkillId(skillId: string): boolean {
  return (
    skillId.startsWith("runx/payment-") ||
    skillId.startsWith("runx/pay-") ||
    skillId.startsWith("runx/charge-") ||
    skillId.startsWith("runx/refund-") ||
    skillId.startsWith("runx/x402-") ||
    skillId === "runx/dispute-respond" ||
    /^runx\/(?:mock|mpp|stripe)-(?:charge|pay|refund)$/.test(skillId)
  );
}

// Skipped automatically when the local seeded registry isn't on disk
// (CI nodes, fresh clones without the cloud .data layout, etc.).
const runIfSeeded = REGISTRY_AVAILABLE ? describe : describe.skip;

runIfSeeded("graph registry refs — real seeded registry", () => {
  it("lists seeded runx skills that the homepage catalog expects", async () => {
    const store = createFileRegistryStore(REAL_REGISTRY_ROOT);
    const skills = await store.listSkills();
    const skillIds = skills.map((skill) => skill.skill_id);

    // Homepage buildFeaturedGroups expects at least these
    expect(skillIds).toEqual(expect.arrayContaining([
      "runx/evolve",
      "runx/issue-to-pr",
      "runx/release",
      "runx/skill-lab",
      "runx/work-plan",
      "runx/design-skill",
      "runx/scafld",
      "runx/prior-art",
      "runx/skill-testing",
    ]));
  });

  it("keeps the seeded local payment registry on the current skill shape", async () => {
    const store = createFileRegistryStore(REAL_REGISTRY_ROOT);
    const skills = await store.listSkills();
    const listedSkillIds = skills.map((skill) => skill.skill_id);
    const skillIds = new Set(listedSkillIds);

    expect(currentPaymentRegistrySkillIds.filter((skillId) => !skillIds.has(skillId))).toEqual([]);
    expect(retiredPaymentRegistrySkillIds.filter((skillId) => skillIds.has(skillId))).toEqual([]);
    expect(listedSkillIds.filter(isPaymentRegistrySkillId).sort()).toEqual(
      [...currentPaymentRegistrySkillIds].sort(),
    );
  });

  it("materializes a real seeded skill to disk, parseable as a valid skill", async () => {
    const cacheDir = await mkdtemp(path.join(os.tmpdir(), "runx-real-registry-cache-"));
    try {
      const store = createFileRegistryStore(REAL_REGISTRY_ROOT);
      const materialized = await materializeRegistrySkill({
        ref: "runx/scafld",
        store,
        cacheDir,
      });

      expect(materialized.skillDirectory).toMatch(/runx\/scafld/);
      expect(existsSync(materialized.skillPath)).toBe(true);

      const markdown = await readFile(materialized.skillPath, "utf8");
      expect(markdown).toBe(materialized.resolution.markdown);

      // The materialized SKILL.md has to round-trip through the real parser/validator
      // or the whole pipeline (graph → loadValidatedSkill → executeSkill) would fail.
      const raw = parseSkillMarkdown(markdown);
      const validated = validateSkill(raw, { mode: "strict" });
      expect(validated.name).toBe("scafld");
    } finally {
      await rm(cacheDir, { recursive: true, force: true });
    }
  });

  it("is idempotent: second materialization of the same digest is a cache hit", async () => {
    const cacheDir = await mkdtemp(path.join(os.tmpdir(), "runx-real-registry-cache-idem-"));
    try {
      const store = createFileRegistryStore(REAL_REGISTRY_ROOT);

      const first = await materializeRegistrySkill({ ref: "runx/scafld", store, cacheDir });
      const firstMtime = await fileMtime(first.skillPath);

      const second = await materializeRegistrySkill({ ref: "runx/scafld", store, cacheDir });
      const secondMtime = await fileMtime(second.skillPath);

      expect(second.skillDirectory).toBe(first.skillDirectory);
      expect(secondMtime).toBe(firstMtime);
    } finally {
      await rm(cacheDir, { recursive: true, force: true });
    }
  });

  it("surfaces a clear error for a ref that is not in the real registry", async () => {
    const cacheDir = await mkdtemp(path.join(os.tmpdir(), "runx-real-registry-missing-"));
    try {
      const store = createFileRegistryStore(REAL_REGISTRY_ROOT);
      await expect(
        materializeRegistrySkill({
          ref: "runx/definitely-not-a-real-skill",
          store,
          cacheDir,
        }),
      ).rejects.toThrow(/not found in registry/);
    } finally {
      await rm(cacheDir, { recursive: true, force: true });
    }
  });
});

async function fileMtime(filePath: string): Promise<number> {
  return (await stat(filePath)).mtimeMs;
}
