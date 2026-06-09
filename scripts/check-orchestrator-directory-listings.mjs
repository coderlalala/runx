import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../..");

const doc = read("oss/docs/orchestrator-integrations.md");
const draft = read(".scafld/specs/drafts/runx-orchestrator-integration-v1.md");
const adoptionPlan = read("plans/adoption-strategy.md");
const activeAdoptionPlan = read(".plans/active/runx-adoption-strategy.md");

for (const phrase of [
  "The orchestrator integration goal is distribution, not only connectivity:",
  "a runx listing on n8n's public integrations surface",
  "a runx app page in Zapier's public App Directory",
  "follow-on listings in adjacent automation, connector, CI, and MCP registries",
  "backlinks from those pages to runx-owned landing and support pages",
  "@runxhq/n8n-nodes-runx",
  "GitHub Actions publishing with npm",
  "provenance. n8n also says verified community nodes",
  "must not use runtime dependencies",
  "Zapier's publishing requirements prohibit integrations that facilitate financial",
  "Public runx v1 on Zapier",
  "must therefore exclude payment, token-transfer, and settlement actions",
  "Webhook templates alone do not qualify.",
  "Other Registries We Overlooked",
  "Make's public integrations surface and community/approved app path",
  "Pipedream's Marketplace and source-available component registry",
  "Microsoft Power Platform",
  "Node-RED Flow Library",
  "@runxhq/node-red-runx",
  "@activepieces/piece-runx",
  "GitHub Actions Marketplace",
  "https://github.com/runxhq/runx-action",
  "Official MCP Registry",
  "Workato",
  "IFTTT",
  "Tray.ai",
  "UiPath Marketplace",
  "MuleSoft Anypoint Exchange",
  "It is not the backlink path.",
]) {
  assertIncludes(doc, phrase, "orchestrator doc");
}

for (const url of [
  "https://n8n.io/integrations/partner-built/",
  "https://docs.n8n.io/integrations/creating-nodes/deploy/submit-community-nodes/",
  "https://docs.zapier.com/integrations/publish/integration-publishing-requirements",
  "https://docs.zapier.com/integrations/quickstart/private-vs-public-integrations",
  "https://developers.make.com/custom-apps-documentation/community-apps/how-does-it-work",
  "https://developers.make.com/custom-apps-documentation/create-your-first-app/app-visibility",
  "https://pipedream.com/docs/components",
  "https://learn.microsoft.com/en-us/connectors/custom-connectors/submit-certification",
  "https://nodered.org/docs/creating-nodes/packaging",
  "https://www.activepieces.com/docs/build-pieces/sharing-pieces/overview",
  "https://www.activepieces.com/docs/build-pieces/misc/publish-piece",
  "https://docs.github.com/en/actions/how-tos/create-and-publish-actions/publish-in-github-marketplace",
  "https://modelcontextprotocol.io/registry/quickstart",
  "https://docs.workato.com/developing-connectors/community/community",
  "https://ifttt.com/docs",
]) {
  assertIncludes(doc, url, "source link");
}

for (const phrase of [
  "Distribution correction (2026-06-10)",
  "The commercial target is directory presence and backlinks",
  "@runxhq/n8n-nodes-runx",
  "a real public Zapier integration backed by production HTTPS runx APIs",
  "Phase 0/1 local command/webhook work is demoted to dogfood/supporting material.",
  "Make, Pipedream, and Microsoft Power Platform are the next serious directory targets",
  "Node-RED, Activepieces, GitHub Actions Marketplace, and the official MCP Registry",
  "Workato, IFTTT, Tray.ai, UiPath Marketplace, and MuleSoft Anypoint Exchange",
]) {
  assertIncludes(draft, phrase, "orchestrator exploration draft");
}

for (const phrase of [
  "runx is the governed action layer inside the tools people already use.",
  "proof -> local use -> public listing -> hosted run -> receipt link -> proof",
  "n8n public integrations or verified community node page",
  "Zapier public App Directory page",
  "Make public/community or approved app page",
  "Pipedream Marketplace/verified component page",
  "Microsoft Power Platform certified connector page",
  "GitHub Actions Marketplace page",
  "runxhq/runx-action",
  "official MCP Registry entry",
  "Do not measure adoption by raw provider count.",
  "The strongest signal is not a page view. It is a second receipted run.",
]) {
  assertIncludes(adoptionPlan, phrase, "adoption strategy plan");
}

for (const phrase of [
  "`plans/adoption-strategy.md`.",
  "Submit n8n and Zapier first.",
  "They are dogfood. The backlink path is public directory/app/registry presence",
]) {
  assertIncludes(activeAdoptionPlan, phrase, "active adoption handoff");
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
