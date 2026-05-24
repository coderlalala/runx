# Releasing the runx CLI

Maintainer doc. Most contributors do not need it.

## Identity

The CLI ships from `github.com/runxhq/runx`. Release tags are `cli-vX.Y.Z`
(prefixed so they do not collide with the repo's other release trains). The
git tag is the single source of truth for the version.

The same product version is used on every channel:

- GitHub Release: `cli-vX.Y.Z` (the hub; serves the raw per-target archives)
- npm: `@runxhq/cli@X.Y.Z` (+ `@runxhq/cli-<platform>@X.Y.Z`)
- crates.io: `runx-cli X.Y.Z` (`cargo install runx-cli`)
- Homebrew, Scoop, winget, AUR, Docker (GHCR): `X.Y.Z`

`runx --version` reports `CARGO_PKG_VERSION`, so the crate and npm versions are
stamped from the tag at build time and the number is truthful regardless of how
the binary was installed.

## Versioning model

The source tree keeps its development version; release jobs **stamp** the tag
version, they never commit it. One command stamps every version-bearing
manifest (npm `package.json` + its `optionalDependencies`, `runx-cli/Cargo.toml`,
and `Cargo.lock`):

```bash
pnpm exec tsx scripts/set-release-version.ts X.Y.Z          # write
pnpm exec tsx scripts/set-release-version.ts --check X.Y.Z  # CI drift guard
```

It accepts a raw `cli-vX.Y.Z` / `vX.Y.Z` tag and strips the prefix.

The dependency crates (`runx-runtime`, `runx-contracts`, ...) carry their own
versions and are **not** tied to the release version; they only publish to
crates.io when their own version is bumped.

## Pipeline

`.github/workflows/release.yml` fires on `cli-v*` tags. `workflow_dispatch`
(with a `version` input) runs a build + render dry-run with no publishing.

Stages (the order is intentional — the GitHub Release must exist before any
channel that downloads its archives):

1. **prepare** — resolve the version, stamp + `--check` manifests, `verify:fast`.
2. **build** (5-platform matrix) — pinned toolchain (`rust-toolchain.toml`), stamp,
   `cargo build --release`, then per platform: npm artifacts (`package-rust-cli.ts`),
   the raw archive (`build-release-archives.ts`), and the `.deb` (linux). Uploads
   npm + archive artifacts.
3. **smoke** (5-platform matrix) — downloads each built archive and runs
   `runx --version` on the real OS. Gates the release: a broken or wrong-arch
   binary fails here before anything is published. Runs in dry-runs too.
4. **github-release** — assemble `checksums.txt`, generate a CycloneDX SBOM, emit
   build-provenance attestations for the binaries, stage the install scripts, and
   publish the Release with all archives. This is the hub.
5. **publish-npm** — verify + publish the selector and native packages with npm
   provenance (`skip-existing`).
6. **publish-crates** — publish the crates in dependency order, then `runx-cli`.
7. **package-managers** — build the channel input from the published checksums
   (`build-channel-input.mjs`), render Homebrew / Scoop / winget / AUR manifests
   (`gen-channel-manifests.ts`), attach them to the Release.
8. **publish-{homebrew,scoop,winget,aur}** — push to the owned registries when
   their credentials are configured; otherwise skipped with a warning.
9. **publish-docker** — multi-arch GHCR image (pulls the musl archive from the
   Release; no Rust toolchain in the image build).

## Installing (end users)

These work the moment a `cli-v*` tag ships, with no package-manager setup:

```sh
# macOS / Linux
curl -fsSL runx.ai/install | sh
```
```powershell
# Windows
irm runx.ai/install.ps1 | iex
```

`runx.ai/install` and `runx.ai/install.ps1` are clean public paths that **proxy**
to the scripts in this repo ([scripts/install](../scripts/install) and
[scripts/install.ps1](../scripts/install.ps1) on `main`); the script bodies are
not duplicated on the site. Both detect OS/arch, download the matching archive
from the GitHub Release, verify its sha256, and install to a user bin dir.
Overrides: `RUNX_VERSION`, `RUNX_INSTALL_DIR`, `RUNX_BASE_URL` (private mirror).

> Site proxy: point `runx.ai/install` → the raw `scripts/install` and
> `runx.ai/install.ps1` → raw `scripts/install.ps1` (302 or pass-through). Keep
> the path extensionless for the shell installer.

## Required secrets

Publishing degrades gracefully: each registry job is gated on its secret and
skipped (with a `::warning::`) when unset, so a release can go out npm-only and
gain channels as credentials land.

| Secret | Channel | Required for |
| --- | --- | --- |
| `NPM_TOKEN` | npm | selector + native packages |
| `CARGO_REGISTRY_TOKEN` | crates.io | `cargo install runx-cli` |
| `HOMEBREW_TAP_TOKEN` | Homebrew | push to `runxhq/homebrew-tap` |
| `SCOOP_BUCKET_TOKEN` | Scoop | push to `runxhq/scoop-bucket` |
| `WINGET_TOKEN` | winget | PR to `microsoft/winget-pkgs` |
| `AUR_SSH_PRIVATE_KEY` | AUR | push `runx-bin` |
| `GITHUB_TOKEN` | GitHub Release, GHCR | provided automatically |

External repos to create before enabling those channels: `runxhq/homebrew-tap`,
`runxhq/scoop-bucket`, and the `runxhq.runx` winget package / `runx-bin` AUR
package.

## Cutting a release

```bash
# 1. dry-run from the Actions tab (workflow_dispatch, version = X.Y.Z) — optional
# 2. tag and push:
git tag cli-vX.Y.Z
git push origin cli-vX.Y.Z
```

Never move a published semver tag; cut a new patch instead.

## Layout

```
crates/rust-toolchain.toml    # pinned Rust version for reproducible builds
scripts/
  set-release-version.ts      # stamp / --check the version across manifests
  build-release-archives.ts   # raw tar.gz/zip + .sha256 per target (release hub)
  build-channel-input.mjs     # checksums -> channel manifest input
  gen-channel-manifests.ts    # render Homebrew / Scoop / winget / AUR
  make-signature-manifest.ts  # npm native-package signature manifest
  package-rust-cli.ts         # npm selector + native package staging
  check-rust-cli-release-artifacts.ts  # npm release contract validator
  install / install.ps1       # end-user one-liner installers (proxied via runx.ai/install)
packaging/
  docker/Dockerfile           # GHCR image (fetches the musl archive)
```
