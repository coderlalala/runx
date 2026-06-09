# Orchestrator Directory Listings

The orchestrator integration goal is distribution, not only connectivity:

- a runx listing on n8n's public integrations surface
- a runx app page in Zapier's public App Directory
- follow-on listings in adjacent automation, connector, CI, and MCP registries
- backlinks from those pages to runx-owned landing and support pages

Self-hosted n8n command nodes and webhook templates are useful dogfood, but they
do not earn those listings. A public listing needs an actual package/app that the
orchestrator can review and expose to users.

## Target Surfaces

### n8n

Target: n8n's public integration library, especially the partner-built/verified
community node surface at `https://n8n.io/integrations/partner-built/`.

The practical route is a verified community node package, not local command
wiring. Current n8n docs require community node packages to:

- use a package name beginning with `n8n-nodes-` or scoped as
  `@<scope>/n8n-nodes-`
- include the `n8n-community-node-package` npm keyword
- declare nodes and credentials in the package `n8n` attribute
- pass lint/local tests
- publish to npm

For verification, n8n currently requires GitHub Actions publishing with npm
provenance. n8n also says verified community nodes must follow technical and UX
guidelines, have proper README/docs, and must not use runtime dependencies.

Proposed package:

- `@runxhq/n8n-nodes-runx`
- Node name: `Runx`
- Credential: `Runx API`
- Initial operation: `Run Skill`
- Secondary operation after receipts API exists: `Get Receipt`
- Backlink target: a stable runx-owned n8n integration page, not a GitHub file

Real blocker: a verified n8n Cloud-usable node needs a production HTTPS runx API.
The local CLI/MCP path cannot be the verified listing path because n8n Cloud
cannot run a local shell or reach localhost.

## Zapier

Target: a public runx app in Zapier's App Directory.

Zapier distinguishes private integrations from public integrations. Public
integrations can be published in the App Directory, join the Partner Program, and
expose Zap templates. Public publishing currently requires:

- app/API ownership or permission proof
- production HTTPS endpoints
- secure credential handling through Zapier authentication configuration
- a publicly launched production app, not a beta or sandbox-only service
- documented APIs
- successful enabled test Zaps with Zap history available for review
- listing name/description/homepage/logo that follow Zapier conventions
- an admin team member using the app/API domain
- a non-expiring test account for `integration-testing@zapier.com`
- passing Zapier validation checks and publishing tasks

Zapier's publishing requirements prohibit integrations that facilitate financial
transactions, transfer assets, or process payments. Public runx v1 on Zapier
must therefore exclude payment, token-transfer, and settlement actions even if
runx can govern those skills elsewhere.

Proposed public Zapier v1:

- App name: `runx`
- Authentication: API key/OAuth against hosted runx
- Action: `Run Skill` for non-payment skills only
- Action: `Get Receipt`
- Search: `Find Run`
- No trigger in v1 unless a production webhook/resume surface exists and passes
  Zapier's public-trigger constraints
- Backlink target: runx marketing homepage plus a stable Zapier integration
  support page

Real blocker: Zapier public listing requires a production HTTPS runx API and
reviewable test account. Webhook templates alone do not qualify.

## Other Registries We Overlooked

n8n and Zapier are still the first backlink targets because they map cleanly to
the product story: no-code workflow triggers call a governed runx step. The
next surfaces should be prioritized by whether they create an indexed public
listing and whether they can reuse the same hosted run-skill and receipt APIs.

### Priority 0: Same Buyer, Same API

These should sit directly behind n8n and Zapier once the hosted API exists.

**Make**

Target: Make's public integrations surface and community/approved app path.

Make says community apps can be public, appear with other public apps on the
Integrations page, and can receive a Make landing page with a link to the
partner's site. A public app can be shared by invite link; an approved app is
available to all Make users after review.

Proposed Make v1:

- App name: `runx`
- Module: `Run Skill`
- Module: `Get Receipt`
- Optional later module: `Resume Run` only after external resume exists
- Backlink target: `https://runx.ai/integrations/make`

Real blocker: Make cloud cannot call localhost. The Make app is gated on the
same production HTTPS run-skill API as Zapier.

**Pipedream**

Target: Pipedream's Marketplace and source-available component registry.

Pipedream verified components are sources and actions curated through a GitHub
PR process; registered components appear in Pipedream's Marketplace and in the
workflow builder UI. Private or non-conforming components do not earn the same
registry value.

Proposed Pipedream v1:

- App integration: `runx`
- Action: `Run Skill`
- Action: `Get Receipt`
- Source later: `New Run Completed` after webhook/resume/event delivery exists
- Backlink target: `https://runx.ai/integrations/pipedream`

Real blocker: same as Zapier and Make: production HTTPS API, stable auth, and
reviewable component behavior.

**Microsoft Power Platform**

Target: certified connector pages across Power Automate, Power Apps, Logic
Apps, and Copilot Studio.

Microsoft connector certification makes a custom connector publicly available,
adds it to official connector documentation, and gives each connector its own
public page. This is higher effort than n8n/Zapier, but it is likely the
strongest enterprise directory surface.

Proposed Power Platform v1:

- Connector name: `runx`
- Action: `Run Skill`
- Action: `Get Receipt`
- No payment/asset-transfer action in public v1
- Backlink target: `https://runx.ai/integrations/power-automate`

Real blocker: certification needs a production API, stable OpenAPI/connector
definition, publisher identity, support process, and Microsoft review.

### Priority 1: Developer And OSS Distribution

These are useful backlinks and developer discovery surfaces, but they should not
pull product/API work ahead of the hosted run-skill seam.

**Node-RED Flow Library**

Target: `flows.nodered.org`.

Node-RED nodes are npm packages and must be manually submitted to the Flow
Library; publishing to npm with the old keyword path is not enough by itself.

Proposed package:

- `@runxhq/node-red-runx`
- Node: `runx skill`
- Node: `runx receipt`
- Backlink target: `https://runx.ai/integrations/node-red`

This can support self-hosted/local runx better than cloud-only registries, but a
public package still needs clear credential handling and docs.

**Activepieces**

Target: Activepieces pieces ecosystem.

Activepieces supports publishing pieces by contributing back to the main
repository, publishing a community npm package, or publishing privately. Pieces
are TypeScript packages and can expose actions and triggers.

Proposed piece:

- Package: `@activepieces/piece-runx`
- Action: `Run Skill`
- Action: `Get Receipt`
- Trigger later: `Run Completed`
- Backlink target: `https://runx.ai/integrations/activepieces`

Real blocker: a cloud-usable community piece still needs the hosted runx API.

**GitHub Actions Marketplace**

Target: GitHub Actions Marketplace.

This is CI distribution rather than workflow-orchestrator distribution, but it
is a low-friction backlink surface for developers. GitHub requires a public
repository, a single root `action.yml` or `action.yaml`, and a unique action
metadata `name`.

Proposed action:

- Repository: `runxhq/runx-action`
- Action name: `runx`
- Operation: run a governed skill in CI and upload/print receipt metadata
- Backlink target: `https://runx.ai/integrations/github-actions`

Status: public repository and `v0.1.0` release exist at
`https://github.com/runxhq/runx-action`. Final Marketplace publication still
requires the GitHub release UI checkbox and any required Marketplace Developer
Agreement acceptance.

This can start as a CLI wrapper before hosted APIs are complete, but the public
copy must be explicit that cloud orchestrator use is still hosted-API gated.

**Official MCP Registry**

Target: `registry.modelcontextprotocol.io`.

The official MCP Registry hosts metadata, not artifacts. A server package must
be published elsewhere first, then described with `server.json` and published
with `mcp-publisher`.

Proposed MCP listing:

- Package/server name: `io.github.runxhq/runx`
- Artifact: npm, PyPI, Docker, or a hosted MCP server depending on the packaging
  decision
- Backlink target: `https://runx.ai/integrations/mcp`

This is not an n8n/Zapier replacement. It is agent-tool discovery and should be
listed separately from workflow automation directories.

### Priority 2: Enterprise Or Fit-Dependent Surfaces

These are worth tracking, but only after the smaller public app/package surfaces
prove demand.

- **Workato**: community connectors and partner connectors can surface in
  Workato's connector directory/community library. Strong enterprise audience,
  but partner-led and higher support burden.
- **IFTTT**: services get dedicated IFTTT service pages and Applets, but the fit
  is weaker unless runx has consumer/IoT-style triggers and at least a dozen
  useful Applets ready for review.
- **Tray.ai**: custom connectors can be built and published/reviewed, but the
  public backlink route is less direct than Make, Pipedream, or Power Platform.
- **UiPath Marketplace**: useful if runx becomes an RPA/governed-automation
  control point. Connector Builder has a publish-to-marketplace flow, but this
  should follow enterprise demand.
- **MuleSoft Anypoint Exchange**: relevant for API/enterprise integration and
  now agent/MCP asset discovery, but mostly valuable when runx has enterprise
  customers asking for Anypoint assets.

## Backlink Pack

Before submitting listings, runx needs stable public pages:

- `https://runx.ai/integrations/n8n`
- `https://runx.ai/integrations/zapier`
- `https://runx.ai/integrations/make`
- `https://runx.ai/integrations/pipedream`
- `https://runx.ai/integrations/power-automate`
- `https://runx.ai/integrations/node-red`
- `https://runx.ai/integrations/activepieces`
- `https://runx.ai/integrations/github-actions`
- `https://runx.ai/integrations/mcp`
- `https://runx.ai/docs/orchestrators`
- `https://runx.ai/security`
- `https://runx.ai/support`

The pages should explain:

- governed skill execution
- signed receipts
- policy and secret ownership
- non-payment limitation for Zapier public v1
- support contact and status page
- API docs for hosted run-skill and receipt lookup once those APIs exist

## Listing Copy

n8n short description:

> Run governed runx skills from n8n workflows and return signed receipts for
> policy, audit, and replay.

Zapier app description:

> runx is a governed runtime for agent and automation work. It runs skills under
> policy and returns signed receipts for audit and replay.

Avoid claims that n8n or Zapier endorse runx before approval. Avoid saying runx
is listed, verified, public, or available in either directory until the listing
is live.

## What The Local Work Is For

The existing local n8n guidance remains useful as dogfood:

- self-hosted n8n can call `runx skill ... --json`
- self-hosted n8n can consume local MCP HTTP on loopback
- runx can call n8n/Zapier-style webhook URLs as outbound effects

That work proves workflow value and receipt shape. It is not the backlink path.

## Execution Order

1. Build stable runx integration landing/support pages.
2. Build hosted non-pausing run-skill and receipt lookup APIs.
3. Build `@runxhq/n8n-nodes-runx` using n8n's node tooling and publish with
   GitHub Actions provenance.
4. Submit the n8n package for verification through the Creator Portal.
5. Build a private Zapier integration against production HTTPS APIs.
6. Run validation, turn on test Zaps, prepare test account, and submit for
   public Zapier App Directory review.
7. Build Make and Pipedream clients on the same hosted API.
8. Build Node-RED, Activepieces, GitHub Actions, and MCP packages as developer
   distribution surfaces.
9. Evaluate Power Platform certification, Workato, IFTTT, Tray.ai, UiPath, and
   MuleSoft after the smaller public listings prove demand.
10. Add templates, embedded links, and co-marketing copy only after each listing
    is approved or live.

## Source Links

- n8n submit community nodes:
  `https://docs.n8n.io/integrations/creating-nodes/deploy/submit-community-nodes/`
- n8n verification guidelines:
  `https://docs.n8n.io/integrations/creating-nodes/build/reference/verification-guidelines/`
- n8n partner-built integrations:
  `https://n8n.io/integrations/partner-built/`
- Zapier publishing requirements:
  `https://docs.zapier.com/integrations/publish/integration-publishing-requirements`
- Zapier private vs public integrations:
  `https://docs.zapier.com/integrations/quickstart/private-vs-public-integrations`
- Zapier integration checks:
  `https://docs.zapier.com/integrations/publish/integration-checks-reference`
- Make community apps FAQ:
  `https://developers.make.com/custom-apps-documentation/community-apps/how-does-it-work`
- Make app visibility:
  `https://developers.make.com/custom-apps-documentation/create-your-first-app/app-visibility`
- Pipedream components:
  `https://pipedream.com/docs/components`
- Microsoft connector certification:
  `https://learn.microsoft.com/en-us/connectors/custom-connectors/submit-certification`
- Node-RED packaging and Flow Library submission:
  `https://nodered.org/docs/creating-nodes/packaging`
- Activepieces sharing pieces:
  `https://www.activepieces.com/docs/build-pieces/sharing-pieces/overview`
- Activepieces publish custom pieces:
  `https://www.activepieces.com/docs/build-pieces/misc/publish-piece`
- GitHub Actions Marketplace publishing:
  `https://docs.github.com/en/actions/how-tos/create-and-publish-actions/publish-in-github-marketplace`
- MCP Registry quickstart:
  `https://modelcontextprotocol.io/registry/quickstart`
- Workato community connectors:
  `https://docs.workato.com/developing-connectors/community/community`
- IFTTT build your integration:
  `https://ifttt.com/docs`
