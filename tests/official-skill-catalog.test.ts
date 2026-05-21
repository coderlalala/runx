import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { beforeAll, describe, expect, it } from "vitest";

import { parseSkillMarkdown, parseRunnerManifestYaml, validateRunnerManifest, validateSkill } from "@runxhq/core/parser";

const officialSkillPackages = [
  "content-pipeline",
  "deep-research-brief",
  "draft-content",
  "ecosystem-vuln-scan",
  "evolve",
  "issue-intake",
  "issue-triage",
  "issue-to-pr",
  "ecosystem-brief",
  "moltbook",
  "work-plan",
  "design-skill",
  "prior-art",
  "write-harness",
  "review-receipt",
  "review-skill",
  "improve-skill",
  "reflect-digest",
  "release",
  "skill-lab",
  "research",
  "scafld",
  "skill-testing",
  "sourcey",
  "vuln-scan",
] as const;

const harnessedShowcasePackages = [
  "content-pipeline",
  "deep-research-brief",
  "draft-content",
  "ecosystem-vuln-scan",
  "evolve",
  "issue-intake",
  "issue-triage",
  "ecosystem-brief",
  "moltbook",
  "work-plan",
  "design-skill",
  "prior-art",
  "write-harness",
  "review-receipt",
  "review-skill",
  "improve-skill",
  "reflect-digest",
  "release",
  "skill-lab",
  "research",
  "scafld",
  "skill-testing",
  "sourcey",
  "vuln-scan",
] as const;

const workspaceRoot = process.cwd();
const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
const nativeRunx = path.resolve("crates", "target", "debug", process.platform === "win32" ? "runx.exe" : "runx");

describe("official skill catalog", () => {
  beforeAll(() => {
    const result = spawnSync(
      cargo,
      ["build", "--quiet", "--manifest-path", "crates/Cargo.toml", "-p", "runx-cli", "--bin", "runx"],
      {
        cwd: workspaceRoot,
        encoding: "utf8",
        env: process.env,
        maxBuffer: 8 * 1024 * 1024,
      },
    );

    expect(result.status, result.stderr || result.stdout).toBe(0);
  }, 60_000);

  it("ships official skills as portable packages plus checked-in execution profiles", async () => {
    for (const skillName of officialSkillPackages) {
      const skillDir = path.resolve("skills", skillName);
      const skillMarkdownPath = path.join(skillDir, "SKILL.md");
      const manifestPath = path.join(skillDir, "X.yaml");

      expect(existsSync(skillDir)).toBe(true);
      expect(existsSync(skillMarkdownPath)).toBe(true);
      expect(existsSync(manifestPath)).toBe(true);

      const skill = validateSkill(parseSkillMarkdown(await readFile(skillMarkdownPath, "utf8")));
      const manifest = validateRunnerManifest(parseRunnerManifestYaml(await readFile(manifestPath, "utf8")));

      expect(skill.name).toBe(skillName);
      expect(manifest.catalog).toBeDefined();
      expect(Object.keys(manifest.runners).length).toBeGreaterThan(0);
    }
  });

  it("keeps evaluator-facing packages runnable through native inline harness fixtures", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-official-native-harness-"));
    let executedCases = 0;
    try {
      for (const skillName of harnessedShowcasePackages) {
        const manifestPath = path.resolve("skills", skillName, "X.yaml");
        const manifest = validateRunnerManifest(parseRunnerManifestYaml(await readFile(manifestPath, "utf8")));
        if (Object.values(manifest.runners).some((runner) => runner.source.graph)) {
          continue;
        }
        if (!manifest.harness || manifest.harness.cases.length === 0) {
          throw new Error(`expected inline harness suite for ${skillName}`);
        }
        for (const entry of manifest.harness.cases) {
          const fixturePath = path.join(tempDir, `${skillName}-${entry.name}.yaml`);
          await writeFile(fixturePath, JSON.stringify({
            name: entry.name,
            kind: "skill",
            target: path.resolve("skills", skillName),
            runner: entry.runner,
            inputs: entry.inputs,
            env: entry.env,
            caller: entry.caller,
            expect: entry.expect,
          }, null, 2));
          const result = spawnSync(nativeRunx, ["harness", fixturePath, "--json"], {
            cwd: workspaceRoot,
            encoding: "utf8",
            env: process.env,
            maxBuffer: 8 * 1024 * 1024,
          });

          expect(result.status, `${skillName}/${entry.name}\n${result.stderr || result.stdout}`).toBe(0);
          expect(JSON.parse(result.stdout)).toMatchObject({ schema: "runx.harness_receipt.v1" });
          executedCases += 1;
        }
      }
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
    expect(executedCases).toBeGreaterThan(0);
  }, 60_000);
});
