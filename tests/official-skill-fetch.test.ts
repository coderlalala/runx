import { generateKeyPairSync, sign } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { cp, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "vitest";

import { resolveRunnableSkillReference } from "../packages/cli/src/index.js";

describe("official skill fetch", () => {
  it("acquires, caches, and reruns an official skill offline from cache", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-official-fetch-"));
    const projectDir = path.join(tempDir, "project");
    const globalHomeDir = path.join(tempDir, "home");
    const env = {
      ...process.env,
      RUNX_CWD: projectDir,
      RUNX_HOME: globalHomeDir,
      RUNX_REGISTRY_URL: "https://runx.example.test",
      RUNX_DEV_RUST_CLI_BIN: nativeRunxBinaryForTest(),
    };
    const officialLock = JSON.parse(
      await readFile(path.resolve("packages/cli/src/official-skills.lock.json"), "utf8"),
    ) as ReadonlyArray<{
      readonly skill_id: string;
      readonly version: string;
      readonly digest: string;
    }>;
    const sourceyLock = officialLock.find((entry) => entry.skill_id === "runx/sourcey");
    if (!sourceyLock) {
      throw new Error("Missing runx/sourcey entry in official-skills.lock.json.");
    }

    try {
      const registryDir = path.join(tempDir, "registry");
      publishLocalRegistrySkill({
        registryDir,
        subject: path.resolve("skills/sourcey/SKILL.md"),
        profile: path.resolve("skills/sourcey/X.yaml"),
        owner: "runx",
        version: sourceyLock.version,
        env,
      });

      env.RUNX_REGISTRY_URL = registryDir;
      const firstPath = await resolveRunnableSkillReference("sourcey", env);
      expect(firstPath).toBe(path.join(globalHomeDir, "official-skills", "runx", "sourcey"));
      expect((await stat(path.join(globalHomeDir, "install.json"))).isFile()).toBe(true);
      expect((await stat(path.join(firstPath, "SKILL.md"))).isFile()).toBe(true);

      const secondPath = await resolveRunnableSkillReference("sourcey", env);
      expect(secondPath).toBe(firstPath);
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });

  it("rejects an official acquisition with a digest mismatch", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-official-fetch-digest-"));
    const env = {
      ...process.env,
      RUNX_CWD: path.join(tempDir, "project"),
      RUNX_HOME: path.join(tempDir, "home"),
      RUNX_REGISTRY_URL: "https://runx.example.test",
      RUNX_DEV_RUST_CLI_BIN: nativeRunxBinaryForTest(),
    };
    const officialLock = JSON.parse(
      await readFile(path.resolve("packages/cli/src/official-skills.lock.json"), "utf8"),
    ) as ReadonlyArray<{
      readonly skill_id: string;
      readonly version: string;
      readonly digest: string;
    }>;
    const sourceyLock = officialLock.find((entry) => entry.skill_id === "runx/sourcey");
    if (!sourceyLock) {
      throw new Error("Missing runx/sourcey entry in official-skills.lock.json.");
    }

    try {
      const registryDir = path.join(tempDir, "registry");
      const wrongSkillDir = path.join(tempDir, "wrong-sourcey");
      const wrongSkillPath = path.join(wrongSkillDir, "SKILL.md");
      const originalMarkdown = await readFile(path.resolve("skills/sourcey/SKILL.md"), "utf8");
      await writeTestFile(
        wrongSkillPath,
        originalMarkdown.replace(
          "description: Generate documentation for a project using Sourcey.",
          "description: Generate different documentation for a project using Sourcey.",
        ),
      );
      publishLocalRegistrySkill({
        registryDir,
        subject: wrongSkillPath,
        profile: path.resolve("skills/sourcey/X.yaml"),
        owner: "runx",
        version: sourceyLock.version,
        env,
      });
      env.RUNX_REGISTRY_URL = registryDir;

      await expect(resolveRunnableSkillReference("sourcey", env)).rejects.toThrow("digest_mismatch");
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });

  it("copies packaged stage helpers beside cached official graph skills", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-official-fetch-runtime-"));
    const env = {
      ...process.env,
      RUNX_CWD: path.join(tempDir, "project"),
      RUNX_HOME: path.join(tempDir, "home"),
      RUNX_REGISTRY_URL: "https://runx.example.test",
      RUNX_DEV_RUST_CLI_BIN: nativeRunxBinaryForTest(),
    };
    const officialLock = JSON.parse(
      await readFile(path.resolve("packages/cli/src/official-skills.lock.json"), "utf8"),
    ) as ReadonlyArray<{
      readonly skill_id: string;
      readonly version: string;
      readonly digest: string;
    }>;
    const lockEntry = officialLock.find((entry) => entry.skill_id === "runx/issue-to-pr");
    if (!lockEntry) {
      throw new Error("Missing runx/issue-to-pr entry in official-skills.lock.json.");
    }

    try {
      await seedOfficialCacheEntry(env, lockEntry.skill_id);
      await resolveRunnableSkillReference("issue-to-pr", env);
      expect(
        (await stat(
          path.join(env.RUNX_HOME, "official-skills", "runx", "issue-to-pr", "graph", "scafld", "run.mjs"),
        )).isFile(),
      ).toBe(true);
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });

  it("copies graph stages beside cached official graph skills", async () => {
    const tempDir = await mkdtemp(path.join(os.tmpdir(), "runx-official-fetch-stages-"));
    const env = {
      ...process.env,
      RUNX_CWD: path.join(tempDir, "project"),
      RUNX_HOME: path.join(tempDir, "home"),
      RUNX_REGISTRY_URL: "https://runx.example.test",
      RUNX_DEV_RUST_CLI_BIN: nativeRunxBinaryForTest(),
    };
    const officialLock = JSON.parse(
      await readFile(path.resolve("packages/cli/src/official-skills.lock.json"), "utf8"),
    ) as ReadonlyArray<{
      readonly skill_id: string;
      readonly version: string;
      readonly digest: string;
    }>;
    const lockEntry = officialLock.find((entry) => entry.skill_id === "runx/spend");
    if (!lockEntry) {
      throw new Error("Missing runx/spend entry in official-skills.lock.json.");
    }

    try {
      await seedOfficialCacheEntry(env, lockEntry.skill_id);
      const skillPath = await resolveRunnableSkillReference("spend", env);
      expect(skillPath).toBe(path.join(env.RUNX_HOME, "official-skills", "runx", "spend"));
      for (const stage of ["pay-quote", "pay-reserve", "pay-fulfill-rail"]) {
        expect(
          (await stat(
            path.join(env.RUNX_HOME, "official-skills", "runx", "spend", "graph", stage, "X.yaml"),
          )).isFile(),
          stage,
        ).toBe(true);
      }
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });
});

function publishLocalRegistrySkill(input: {
  readonly registryDir: string;
  readonly subject: string;
  readonly owner: string;
  readonly version: string;
  readonly env: NodeJS.ProcessEnv;
  readonly profile?: string;
}): void {
  const signingKey = testManifestSigningKey();
  input.env.RUNX_REGISTRY_MANIFEST_TRUST_KEY_ID = signingKey.keyId;
  input.env.RUNX_REGISTRY_MANIFEST_TRUST_KEY_BASE64 = signingKey.publicKeyBase64;
  const args = [
    "registry",
    "publish",
    input.subject,
    "--registry-dir",
    input.registryDir,
    "--owner",
    input.owner,
    "--version",
    input.version,
    "--upsert",
    "--json",
  ];
  if (input.profile) {
    args.push("--profile", input.profile);
  }
  const result = spawnSync(input.env.RUNX_DEV_RUST_CLI_BIN ?? "runx", args, {
    cwd: process.cwd(),
    env: input.env,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(`failed to publish local registry fixture: ${result.stderr || result.stdout}`);
  }
  signPublishedRegistryEntry(input.registryDir, signingKey);
}

async function seedOfficialCacheEntry(env: NodeJS.ProcessEnv, skillId: string): Promise<void> {
  const [owner, skillName] = skillId.split("/");
  if (!owner || !skillName) {
    throw new Error(`invalid official skill id ${skillId}`);
  }
  const target = path.join(env.RUNX_HOME ?? "", "official-skills", owner, skillName);
  const source = path.resolve("skills", skillName);
  await mkdir(target, { recursive: true });
  await writeFile(
    path.join(target, "SKILL.md"),
    await readFile(path.join(source, "SKILL.md"), "utf8"),
    "utf8",
  );
  for (const entry of await readdir(source, { withFileTypes: true })) {
    if (entry.name === "SKILL.md") {
      continue;
    }
    const sourcePath = path.join(source, entry.name);
    const targetPath = path.join(target, entry.name);
    if (entry.isDirectory()) {
      await cp(sourcePath, targetPath, { recursive: true, force: true });
    } else if (entry.isFile()) {
      await writeFile(targetPath, await readFile(sourcePath));
    }
  }
}

async function writeTestFile(filePath: string, contents: string): Promise<void> {
  await rm(path.dirname(filePath), { recursive: true, force: true });
  await mkdir(path.dirname(filePath), { recursive: true });
  await writeFile(filePath, contents, "utf8");
}

function nativeRunxBinaryForTest(): string {
  const existing = process.env.RUNX_DEV_RUST_CLI_BIN;
  if (existing) {
    return existing;
  }
  const candidate = path.resolve("crates/target/debug/runx");
  return existsSync(candidate) ? candidate : "runx";
}

interface TestManifestSigningKey {
  readonly keyId: string;
  readonly signerId: string;
  readonly publicKeyBase64: string;
  readonly privateKey: ReturnType<typeof generateKeyPairSync>["privateKey"];
}

let cachedManifestSigningKey: TestManifestSigningKey | undefined;

function testManifestSigningKey(): TestManifestSigningKey {
  if (cachedManifestSigningKey) {
    return cachedManifestSigningKey;
  }
  const keyPair = generateKeyPairSync("ed25519");
  const publicKeyDer = keyPair.publicKey.export({ format: "der", type: "spki" });
  const publicKeyRaw = Buffer.from(publicKeyDer).subarray(-32);
  cachedManifestSigningKey = {
    keyId: "runx-test-registry-ed25519",
    signerId: "runx-test-registry",
    publicKeyBase64: publicKeyRaw.toString("base64"),
    privateKey: keyPair.privateKey,
  };
  return cachedManifestSigningKey;
}

function signPublishedRegistryEntry(registryDir: string, signingKey: TestManifestSigningKey): void {
  const entryPath = findSingleRegistryEntry(registryDir);
  const entry = JSON.parse(readFileSync(entryPath, "utf8")) as {
    skill_id: string;
    version: string;
    digest: string;
    profile_digest?: string;
    signed_manifest?: unknown;
  };
  const payload =
    "runx.registry.signed_manifest.v1\n" +
    `skill_id=${entry.skill_id}\n` +
    `version=${entry.version}\n` +
    `digest=${entry.digest}\n` +
    `profile_digest=${entry.profile_digest ?? ""}\n` +
    `signer_id=${signingKey.signerId}\n` +
    `key_id=${signingKey.keyId}\n`;
  entry.signed_manifest = {
    schema: "runx.registry.signed_manifest.v1",
    skill_id: entry.skill_id,
    version: entry.version,
    digest: entry.digest,
    ...(entry.profile_digest ? { profile_digest: entry.profile_digest } : {}),
    signer: {
      id: signingKey.signerId,
      key_id: signingKey.keyId,
    },
    signature: {
      alg: "ed25519",
      value: `base64:${sign(null, Buffer.from(payload), signingKey.privateKey).toString("base64")}`,
    },
  };
  writeFileSync(entryPath, `${JSON.stringify(entry, null, 2)}\n`, "utf8");
}

function findSingleRegistryEntry(root: string): string {
  const matches: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of readdirSync(dir)) {
      const entryPath = path.join(dir, entry);
      const stats = statSync(entryPath);
      if (stats.isDirectory()) {
        walk(entryPath);
      } else if (entryPath.endsWith(".json")) {
        matches.push(entryPath);
      }
    }
  };
  walk(root);
  if (matches.length !== 1) {
    throw new Error(`expected one registry fixture entry, found ${matches.length}`);
  }
  return matches[0];
}
