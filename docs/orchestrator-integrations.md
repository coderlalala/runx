# Orchestrator Directory Listings

The orchestrator integration goal is distribution, not only connectivity:

- a runx listing on n8n's public integrations surface
- a runx app page in Zapier's public App Directory
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

## Backlink Pack

Before submitting either listing, runx needs stable public pages:

- `https://runx.ai/integrations/n8n`
- `https://runx.ai/integrations/zapier`
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
7. Add Zap templates and embedded links after public approval.

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
