import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../..");

const doc = read("oss/docs/orchestrator-integrations.md");
const draft = read(".scafld/specs/drafts/runx-orchestrator-integration-v1.md");

for (const phrase of [
  "The orchestrator integration goal is distribution, not only connectivity:",
  "a runx listing on n8n's public integrations surface",
  "a runx app page in Zapier's public App Directory",
  "backlinks from those pages to runx-owned landing and support pages",
  "@runxhq/n8n-nodes-runx",
  "GitHub Actions publishing with npm",
  "provenance. n8n also says verified community nodes",
  "must not use runtime dependencies",
  "Zapier's publishing requirements prohibit integrations that facilitate financial",
  "Public runx v1 on Zapier",
  "must therefore exclude payment, token-transfer, and settlement actions",
  "Webhook templates alone do not qualify.",
  "It is not the backlink path.",
]) {
  assertIncludes(doc, phrase, "orchestrator doc");
}

for (const url of [
  "https://n8n.io/integrations/partner-built/",
  "https://docs.n8n.io/integrations/creating-nodes/deploy/submit-community-nodes/",
  "https://docs.zapier.com/integrations/publish/integration-publishing-requirements",
  "https://docs.zapier.com/integrations/quickstart/private-vs-public-integrations",
]) {
  assertIncludes(doc, url, "source link");
}

for (const phrase of [
  "Distribution correction (2026-06-10)",
  "The commercial target is directory presence and backlinks",
  "@runxhq/n8n-nodes-runx",
  "a real public Zapier integration backed by production HTTPS runx APIs",
  "Phase 0/1 local command/webhook work is demoted to dogfood/supporting material.",
]) {
  assertIncludes(draft, phrase, "orchestrator exploration draft");
}

console.log("orchestrator directory listing docs ok");

function read(relativePath) {
  return readFileSync(path.resolve(repoRoot, relativePath), "utf8");
}

function assertIncludes(text, phrase, label) {
  const normalizedText = text.replace(/\s+/gu, " ");
  const normalizedPhrase = phrase.replace(/\s+/gu, " ");
  if (!normalizedText.includes(normalizedPhrase)) {
    throw new Error(`${label} missing required phrase: ${phrase}`);
  }
}
