# Releases

Read this file before you change a version, a tag, or the release workflow.

## Contract

- The repository is public. The code license is MIT.
- v1 ships **Windows x64 and Linux x64** only.
- Windows artifact: NSIS installer. Linux artifacts: `.deb` and `.AppImage`.
- macOS is deferred until Apple Developer signing and notarization are funded.
- Windows builds are **unsigned** in v1. SmartScreen shows an unknown publisher.
  The release notes and README must say this.
- v1 has **no auto-update**. Do not add updater keys or `latest.json`.
- GitHub Releases is the only distribution channel.
- A release starts from a tag on `main`. A release never starts from a branch.
- The tag version and `package.json` version must match. `src-tauri/tauri.conf.json`
  reads the version from `../package.json`.
- The workflow creates a draft. A human approves the publish job after the
  checksums and artifacts look correct. After publish, the release is immutable.
- Do not move a published tag. Ship a patch release.

## Version bump

Run these commands from the main checkout:

```bash
git switch main
git pull --ff-only
node scripts/version.mjs 0.2.0
cargo check --workspace
pnpm install --lockfile-only
git add package.json ui/package.json Cargo.toml Cargo.lock
git commit -m "chore(release): v0.2.0"
git push origin main
```

`scripts/version.mjs` updates `package.json`, `ui/package.json`, and the
`[workspace.package]` version in `Cargo.toml`. `cargo check` refreshes
`Cargo.lock`.

## Release

```bash
git switch main
git pull --ff-only
test -z "$(git status --porcelain)"
git tag -s v0.2.0 -m "v0.2.0"
git push origin v0.2.0
gh run list --workflow=release.yml --limit 1
```

Open the run. The `prepare` job creates a draft. The Windows and Linux jobs
upload assets. `finalize` adds `SHA256SUMS`. The `publish` job waits for the
`release` environment approval.

Before you approve, download the draft and install it on a clean machine:

```bash
gh release download v0.2.0 --dir /tmp/msw-v0.2.0
(cd /tmp/msw-v0.2.0 && sha256sum -c SHA256SUMS)
```

Windows: run the NSIS installer. SmartScreen warns. Continue only for a
trusted build. Linux: install the `.deb` and run the AppImage. Check that the
window opens and the health check works.

Then approve the `release` environment.

## Verify

```bash
gh release view v0.2.0 --json isDraft,isPrerelease,assets
gh release download v0.2.0 --pattern SHA256SUMS --dir /tmp/msw-release
(cd /tmp/msw-release && sha256sum -c SHA256SUMS)
gh attestation verify ./Monsquaz\ Swap_0.2.0_x64-setup.exe --repo RAMPKORV/monsquaz-swap
```

## Failure

- If the draft fails, fix the cause and re-run the workflow.
- Delete a draft and its tag only when the release was never published:

```bash
gh release delete v0.2.0 --yes --cleanup-tag
git tag -d v0.2.0
```

- If the release is published, do not delete it. Create `v0.2.1`.
- Do not use `--clobber` on a published release.
- A re-run may replace assets in a draft.

## Signing status

| Platform | v1 status | User effect |
|---|---|---|
| Windows | Unsigned | SmartScreen warning; Smart App Control can block the app. |
| Linux | SHA-256 checksums + GitHub attestations | No OS prompt. The user verifies the sum or attestation. |
| macOS | Not shipped | Not applicable. |

The owner chose "never sign" for v1. If this changes, use SignPath Foundation
(free for eligible OSS) or a Certum Open Source certificate. A signed Windows
build needs a two-pass flow: sign the app and sidecar, bundle, then sign the
installer. Do not add signing secrets to pull request jobs.

## Updater

The updater is off in v1. Do not set `createUpdaterArtifacts`, do not add
`latest.json`, and do not create a Tauri signing key yet.

When it is enabled later:

1. Generate one Tauri updater key pair. Back it up offline. Losing it locks
   installed users out of future updates.
2. Put the public key in `tauri.conf.json`.
3. Store the private key and password in the `release` environment.
4. Set `createUpdaterArtifacts: true` and `requireSignedVersion: true`.
5. Publish one manual updater-capable release first.
6. Generate `latest.json` once in `finalize`, not in each matrix job.
7. Test update and restart on Windows and the Linux AppImage.

The Tauri updater signature is not a Windows Authenticode or Apple Developer
signature. It proves update payload integrity, not publisher identity.

## Sidecar

`openmpt123` is not bundled in v0.1. The preview issue adds it with
`bundle.externalBin` and a per-platform static build. When that lands, the
release workflow must build the sidecar for each target before `tauri build`.
Tauri appends the target triple to the file name.

## Required checks before publish

- [ ] `ci-gate` is green on `main`.
- [ ] Tag is on `main`; version matches `package.json`.
- [ ] Windows NSIS and Linux `.deb`/`.AppImage` are in the draft.
- [ ] `SHA256SUMS` is present and verifies.
- [ ] The app opens on clean Windows 11 and Ubuntu 22.04/24.04.
- [ ] The health check returns `ok`.
- [ ] The README still says Windows is unsigned.
- [ ] No secrets or updater keys are in the repository.
