import { createHash } from "node:crypto";
import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

import { validateRunnerManifestYaml, validateSkillMarkdown } from "./parser-eval.js";

export type RegistryTrustTier = "first_party" | "verified" | "community";

export interface RegistryPublisher {
  readonly kind: "organization" | "user" | "team" | "service" | "publisher";
  readonly id: string;
  readonly handle?: string;
  readonly display_name?: string;
}

export interface RegistryAttestation {
  readonly kind: "source" | "publisher" | "verification";
  readonly id: string;
  readonly status: "verified" | "declared";
  readonly summary: string;
  readonly source?: string;
  readonly issued_at?: string;
  readonly metadata?: Readonly<Record<string, unknown>>;
}

export interface RegistrySkillVersion {
  readonly skill_id: string;
  readonly owner: string;
  readonly name: string;
  readonly description?: string;
  readonly version: string;
  readonly digest: string;
  readonly markdown: string;
  readonly profile_document?: string;
  readonly profile_digest?: string;
  readonly runner_names: readonly string[];
  readonly source_type: string;
  readonly trust_tier: RegistryTrustTier;
  readonly maturity?: "alpha" | "beta" | "stable";
  readonly catalog_kind?: "skill" | "graph";
  readonly catalog_audience?: "public" | "builder" | "operator";
  readonly catalog_visibility?: "public" | "internal";
  readonly attestations?: readonly RegistryAttestation[];
  readonly required_scopes: readonly string[];
  readonly runtime?: unknown;
  readonly auth?: unknown;
  readonly risk?: unknown;
  readonly runx?: Readonly<Record<string, unknown>>;
  readonly tags: readonly string[];
  readonly publisher: RegistryPublisher;
  readonly created_at: string;
  readonly updated_at: string;
}

export interface RegistrySkill {
  readonly skill_id: string;
  readonly owner: string;
  readonly name: string;
  readonly description?: string;
  readonly latest_version: string;
  readonly latest_digest: string;
  readonly versions: readonly RegistrySkillVersion[];
}

export interface RegistryStore {
  readonly putVersion: (
    version: RegistrySkillVersion,
    options?: { readonly upsert?: boolean },
  ) => Promise<RegistrySkillVersion>;
  readonly getVersion: (skillId: string, version?: string) => Promise<RegistrySkillVersion | undefined>;
  readonly listVersions: (skillId: string) => Promise<readonly RegistrySkillVersion[]>;
  readonly listSkills: () => Promise<readonly RegistrySkill[]>;
}

export interface PublishSkillMarkdownOptions {
  readonly owner?: string;
  readonly version?: string;
  readonly createdAt?: string;
  readonly profileDocument?: string;
  readonly registryUrl?: string;
  readonly trustTier?: RegistryTrustTier;
  readonly upsert?: boolean;
}

export interface PublishSkillMarkdownResult {
  readonly status: "published" | "unchanged";
  readonly skill_id: string;
  readonly name: string;
  readonly version: string;
  readonly digest: string;
  readonly profile_digest?: string;
  readonly runner_names: readonly string[];
  readonly source_type: string;
  readonly registry_url?: string;
  readonly link: {
    readonly link: string;
    readonly skill_id: string;
    readonly version: string;
    readonly digest: string;
    readonly registry_url?: string;
    readonly install_command: string;
    readonly run_command: string;
  };
  readonly record: RegistrySkillVersion;
}

export async function seedRegistrySkill(
  store: RegistryStore,
  markdown: string,
  options: PublishSkillMarkdownOptions = {},
): Promise<RegistrySkillVersion> {
  return (await publishRegistryFixtureSkill(store, markdown, options)).record;
}

export async function publishRegistryFixtureSkill(
  store: RegistryStore,
  markdown: string,
  options: PublishSkillMarkdownOptions = {},
): Promise<PublishSkillMarkdownResult> {
  const record = buildRegistryFixtureRecord(markdown, options);
  const existing = await store.getVersion(record.skill_id, record.version);
  const stored = await store.putVersion(record, { upsert: options.upsert });
  return {
    status: existing ? "unchanged" : "published",
    skill_id: stored.skill_id,
    name: stored.name,
    version: stored.version,
    digest: stored.digest,
    profile_digest: stored.profile_digest,
    runner_names: stored.runner_names,
    source_type: stored.source_type,
    registry_url: options.registryUrl,
    link: runxLinkForVersion(stored, options.registryUrl),
    record: stored,
  };
}

export async function buildRegistryFixtureVersion(
  markdown: string,
  options: PublishSkillMarkdownOptions = {},
): Promise<RegistrySkillVersion> {
  return await seedRegistrySkill(createMemoryRegistryStore(), markdown, options);
}

export function createMemoryRegistryStore(): RegistryStore {
  const versions = new Map<string, RegistrySkillVersion>();

  return {
    putVersion: async (
      version: RegistrySkillVersion,
      options?: { readonly upsert?: boolean },
    ): Promise<RegistrySkillVersion> => {
      const key = versionKey(version.skill_id, version.version);
      const existing = versions.get(key);
      if (existing && (existing.digest !== version.digest || existing.profile_digest !== version.profile_digest) && !options?.upsert) {
        throw new Error(`Registry version ${version.skill_id}@${version.version} already exists with a different digest.`);
      }
      const stored = existing ? { ...version, created_at: existing.created_at } : version;
      versions.set(key, stored);
      return stored;
    },
    getVersion: async (skillId: string, version?: string): Promise<RegistrySkillVersion | undefined> => {
      const candidates = sortedVersions(Array.from(versions.values()).filter((candidate) => candidate.skill_id === skillId));
      return version ? candidates.find((candidate) => candidate.version === version) : candidates.at(-1);
    },
    listVersions: async (skillId: string): Promise<readonly RegistrySkillVersion[]> =>
      sortedVersions(Array.from(versions.values()).filter((candidate) => candidate.skill_id === skillId)),
    listSkills: async (): Promise<readonly RegistrySkill[]> => {
      const bySkill = new Map<string, RegistrySkillVersion[]>();
      for (const version of versions.values()) {
        bySkill.set(version.skill_id, [...(bySkill.get(version.skill_id) ?? []), version]);
      }
      const skills: RegistrySkill[] = [];
      for (const [skillId, skillVersions] of bySkill.entries()) {
        const sorted = sortedVersions(skillVersions);
        const latest = sorted.at(-1);
        if (latest) {
          skills.push({
            skill_id: skillId,
            owner: latest.owner,
            name: latest.name,
            description: latest.description,
            latest_version: latest.version,
            latest_digest: latest.digest,
            versions: sorted,
          });
        }
      }
      return skills.sort((left, right) => left.skill_id.localeCompare(right.skill_id));
    },
  };
}

export function createFileRegistryStore(root: string): RegistryStore {
  return {
    putVersion: async (
      version: RegistrySkillVersion,
      options?: { readonly upsert?: boolean },
    ): Promise<RegistrySkillVersion> => {
      const versionPath = registryVersionPath(root, version.skill_id, version.version);
      await mkdir(path.dirname(versionPath), { recursive: true });
      const existing = await readRegistryVersion(versionPath);
      if (existing) {
        if (existing.digest !== version.digest || existing.profile_digest !== version.profile_digest) {
          if (!options?.upsert) {
            throw new Error(`Registry version ${version.skill_id}@${version.version} already exists with a different digest.`);
          }
          const upserted = { ...version, updated_at: new Date().toISOString() };
          await writeFile(versionPath, `${JSON.stringify(upserted, null, 2)}\n`, { flag: "w", mode: 0o600 });
          return upserted;
        }
        const refreshed = { ...version, created_at: existing.created_at, updated_at: new Date().toISOString() };
        await writeFile(versionPath, `${JSON.stringify(refreshed, null, 2)}\n`, { flag: "w", mode: 0o600 });
        return refreshed;
      }
      await writeFile(versionPath, `${JSON.stringify(version, null, 2)}\n`, { flag: "wx", mode: 0o600 });
      return version;
    },
    getVersion: async (skillId: string, version?: string): Promise<RegistrySkillVersion | undefined> => {
      const versions = await listFileVersions(root, skillId);
      return version ? versions.find((candidate) => candidate.version === version) : versions.at(-1);
    },
    listVersions: async (skillId: string): Promise<readonly RegistrySkillVersion[]> => listFileVersions(root, skillId),
    listSkills: async (): Promise<readonly RegistrySkill[]> => {
      const versions = await collectFileRegistryVersions(root);
      return skillsFromVersions(versions);
    },
  };
}

export async function searchRegistryFixture(
  store: RegistryStore,
  query: string,
  options: { readonly limit?: number; readonly registryUrl?: string } = {},
) {
  const normalizedQuery = query.trim().toLowerCase();
  const skills = await store.listSkills();
  const latestVersions = skills
    .map((skill) => skill.versions.at(-1))
    .filter((version): version is RegistrySkillVersion => version !== undefined);
  return latestVersions
    .filter((version) => normalizedQuery.length === 0 || searchableText(version).includes(normalizedQuery))
    .sort((left, right) => left.skill_id.localeCompare(right.skill_id))
    .slice(0, options.limit ?? 20)
    .map((version) => {
      const link = runxLinkForVersion(version, options.registryUrl);
      return {
        skill_id: version.skill_id,
        name: version.name,
        summary: version.description,
        owner: version.owner,
        version: version.version,
        digest: version.digest,
        source_type: version.source_type,
        trust_tier: version.trust_tier,
        required_scopes: version.required_scopes,
        tags: version.tags,
        profile_mode: version.profile_document ? "profiled" : "portable",
        runner_names: version.runner_names,
        profile_digest: version.profile_digest,
        profile_trust_tier: version.profile_document ? version.trust_tier : undefined,
        trust_signals: deriveTrustSignals(version),
        add_command: link.install_command,
        run_command: link.run_command,
        source: "runx-registry",
        source_label: "runx registry",
      };
    });
}

function buildRegistryFixtureRecord(markdown: string, options: PublishSkillMarkdownOptions): RegistrySkillVersion {
  const skill = validateSkillMarkdown(markdown, { mode: "strict" });
  const manifest = options.profileDocument ? validateRunnerManifestYaml(options.profileDocument) : undefined;
  const digest = hashString(markdown);
  const profileDigest = options.profileDocument ? hashString(options.profileDocument) : undefined;
  const owner = options.owner ?? "local";
  const createdAt = options.createdAt ?? new Date().toISOString();
  const publisher = defaultPublisher(owner);
  const trustTier = options.trustTier ?? (owner === "runx" ? "first_party" : "community");
  const version = options.version ?? `sha-${hashString(JSON.stringify({ markdown_digest: digest, profile_digest: profileDigest })).slice(0, 12)}`;
  const runnerNames = manifest ? Object.keys(manifest.runners) : [];
  return {
    skill_id: `${slugify(owner)}/${slugify(skill.name)}`,
    owner,
    name: skill.name,
    description: typeof skill.description === "string" ? skill.description : undefined,
    version,
    digest,
    markdown,
    profile_document: options.profileDocument,
    profile_digest: profileDigest,
    runner_names: runnerNames,
    source_type: skill.source.type,
    trust_tier: trustTier,
    maturity: "alpha",
    catalog_kind: "skill",
    catalog_audience: "public",
    catalog_visibility: "public",
    attestations: [{
      kind: "publisher",
      id: `publisher:${publisher.id}`,
      status: trustTier === "community" ? "declared" : "verified",
      summary: publisher.handle ?? publisher.id,
      issued_at: createdAt,
      metadata: {
        publisher_id: publisher.id,
        publisher_kind: publisher.kind,
        publisher_handle: publisher.handle,
        trust_tier: trustTier,
      },
    }],
    required_scopes: [],
    runtime: runnerNames.length > 0 ? { runners: runnerNames } : undefined,
    auth: skill.auth,
    risk: skill.risk,
    runx: isRecord(skill.runx) ? skill.runx : undefined,
    tags: [],
    publisher,
    created_at: createdAt,
    updated_at: new Date().toISOString(),
  };
}

function versionKey(skillId: string, version: string): string {
  return `${skillId}@${version}`;
}

function sortedVersions(versions: readonly RegistrySkillVersion[]): readonly RegistrySkillVersion[] {
  return versions
    .slice()
    .sort((left, right) => left.created_at.localeCompare(right.created_at) || left.version.localeCompare(right.version));
}

function skillsFromVersions(versions: readonly RegistrySkillVersion[]): readonly RegistrySkill[] {
  const bySkill = new Map<string, RegistrySkillVersion[]>();
  for (const version of versions) {
    bySkill.set(version.skill_id, [...(bySkill.get(version.skill_id) ?? []), version]);
  }
  const skills: RegistrySkill[] = [];
  for (const [skillId, skillVersions] of bySkill.entries()) {
    const sorted = sortedVersions(skillVersions);
    const latest = sorted.at(-1);
    if (latest) {
      skills.push({
        skill_id: skillId,
        owner: latest.owner,
        name: latest.name,
        description: latest.description,
        latest_version: latest.version,
        latest_digest: latest.digest,
        versions: sorted,
      });
    }
  }
  return skills.sort((left, right) => left.skill_id.localeCompare(right.skill_id));
}

function hashString(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

function runxLinkForVersion(record: RegistrySkillVersion, registryUrl?: string): PublishSkillMarkdownResult["link"] {
  const ref = `${record.skill_id}@${record.version}`;
  const registryFlag = registryUrl ? ` --registry ${registryUrl}` : "";
  return {
    link: `runx://skill/${encodeURIComponent(record.skill_id)}@${encodeURIComponent(record.version)}`,
    skill_id: record.skill_id,
    version: record.version,
    digest: record.digest,
    registry_url: registryUrl,
    install_command: `runx skill add ${ref}${registryFlag}`,
    run_command: `runx skill ${record.name}`,
  };
}

function deriveTrustSignals(version: RegistrySkillVersion) {
  const publisherLabel = version.publisher.display_name ?? version.publisher.handle ?? version.publisher.id;
  const publisherAttestation = version.attestations?.find((attestation) => attestation.kind === "publisher");
  return [
    { id: "digest", label: "Immutable digest", status: "verified", value: `sha256:${version.digest}` },
    {
      id: "trust_tier",
      label: "Trust tier",
      status: version.trust_tier === "community" ? "declared" : "verified",
      value: version.trust_tier,
    },
    {
      id: "publisher",
      label: "Publisher identity",
      status: publisherAttestation?.status ?? "not_declared",
      value: publisherLabel,
    },
    { id: "provenance", label: "Source provenance", status: "not_declared", value: "no source attestation" },
    { id: "source_type", label: "Execution source", status: "declared", value: version.source_type },
    {
      id: "scopes",
      label: "Required scopes",
      status: version.required_scopes.length > 0 ? "declared" : "not_declared",
      value: version.required_scopes.length > 0 ? version.required_scopes.join(", ") : "none declared",
    },
    {
      id: "runtime",
      label: "Runtime requirements",
      status: version.runtime ? "declared" : "not_declared",
      value: version.runtime ? "declared in skill metadata" : "none declared",
    },
    {
      id: "runner_metadata",
      label: "Materialized binding",
      status: version.profile_digest ? "verified" : "not_declared",
      value: version.profile_digest
        ? `${version.runner_names.length} runner(s), binding sha256:${version.profile_digest}`
        : "portable agent runner",
    },
  ];
}

async function readRegistryVersion(versionPath: string): Promise<RegistrySkillVersion | undefined> {
  try {
    return JSON.parse(await readFile(versionPath, "utf8")) as RegistrySkillVersion;
  } catch (error) {
    if (isNotFound(error)) {
      return undefined;
    }
    throw error;
  }
}

async function listFileVersions(root: string, skillId: string): Promise<readonly RegistrySkillVersion[]> {
  const [owner, name] = splitSkillId(skillId);
  const skillDir = path.join(root, encodeURIComponent(owner), encodeURIComponent(name));
  const entries = await safeReadDirNames(skillDir);
  const versions = await Promise.all(
    entries
      .filter((entry) => entry.endsWith(".json"))
      .map(async (entry) => JSON.parse(await readFile(path.join(skillDir, entry), "utf8")) as RegistrySkillVersion),
  );
  return sortedVersions(versions);
}

async function collectFileRegistryVersions(root: string): Promise<readonly RegistrySkillVersion[]> {
  const versions: RegistrySkillVersion[] = [];
  for (const owner of await safeReadDirNames(root)) {
    for (const name of await safeReadDirNames(path.join(root, owner))) {
      for (const file of await safeReadDirNames(path.join(root, owner, name))) {
        if (file.endsWith(".json")) {
          versions.push(JSON.parse(await readFile(path.join(root, owner, name, file), "utf8")) as RegistrySkillVersion);
        }
      }
    }
  }
  return versions;
}

async function safeReadDirNames(directory: string): Promise<readonly string[]> {
  try {
    return await readdir(directory);
  } catch (error) {
    if (isNotFound(error) || String(error).includes("ENOENT")) {
      return [];
    }
    throw error;
  }
}

function registryVersionPath(root: string, skillId: string, version: string): string {
  const [owner, name] = splitSkillId(skillId);
  return path.join(root, encodeURIComponent(owner), encodeURIComponent(name), `${encodeURIComponent(version)}.json`);
}

export function splitSkillId(skillId: string): readonly [string, string] {
  const parts = skillId.split("/");
  if (parts.length !== 2 || !parts[0] || !parts[1]) {
    throw new Error(`Invalid registry skill id '${skillId}'. Expected '<owner>/<name>'.`);
  }
  return [parts[0], parts[1]];
}

function slugify(value: string): string {
  const slug = value.trim().toLowerCase().replace(/[^a-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "");
  if (!slug) {
    throw new Error("Registry slugs cannot be empty.");
  }
  return slug;
}

function searchableText(version: RegistrySkillVersion): string {
  return [
    version.skill_id,
    version.name,
    version.description,
    version.owner,
    version.source_type,
    ...version.runner_names,
    ...version.tags,
  ]
    .filter((entry): entry is string => typeof entry === "string")
    .join(" ")
    .trim()
    .toLowerCase();
}

function defaultPublisher(owner: string): RegistryPublisher {
  return owner === "runx"
    ? { kind: "organization", id: owner, handle: owner }
    : { kind: "publisher", id: owner, handle: owner };
}

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNotFound(error: unknown): boolean {
  return isRecord(error) && error.code === "ENOENT";
}
