# CLI

Run commands from the repository root. Use pnpm 12.23.0.

## Setup

```bash
pnpm install
cargo fetch
gh auth status
```

If `gh` is not on `PATH`, use the shared binary at
`~/.local/bin/gh` or `../kuk/tools/bin/gh`.

## Develop

```bash
pnpm tauri dev
pnpm dev
pnpm --dir ui test:watch
```

## Verify

```bash
scripts/check.sh
scripts/design-check.sh
node --test scripts/tests/
node scripts/ste-lint.mjs
node scripts/trace-scenarios.mjs
scripts/requirements-check.sh
scripts/formal/check.sh
node scripts/formal/changed.mjs origin/main
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm typecheck
pnpm test
pnpm build
```

Run `pnpm build` before `cargo test` on a clean checkout. `src-tauri` embeds
`ui/dist` at compile time through `tauri::generate_context!`.

## Tracker and worktrees

```bash
./bin/project status
./bin/project next
./bin/project issue show MSW-###
./bin/project issue claim MSW-### --as-agent <name>
./bin/project worktree new MSW-### --as-agent <name> \
  --force --reason "D-021: no PRD sign-off gate"
./bin/project worktree sync MSW-###
./bin/project worktree finish MSW-### --yes --as-agent <name> \
  --force --reason "D-021: no PRD sign-off gate"
./bin/project export
```

Worktrees live in `../MonsquazSwap-worktrees/`. Use one worktree per batch or
issue. Claim 3–6 related issues, create the worktree from the first issue, and
work the rest in the same worktree. Do not share a mutable `.project` database
across worktrees.

## Commit and pull request

```bash
git add -A
git commit -m "MSW-###: imperative summary"
git push -u origin HEAD
gh pr create --title "batch: ..." --body "... issue keys ..." --base main
gh pr checks --watch
gh pr merge --squash --delete-branch
```

## Release

```bash
node scripts/version.mjs 0.2.0
cargo check --workspace
git add package.json ui/package.json Cargo.toml Cargo.lock
git commit -m "chore(release): v0.2.0"
git push origin main
git tag -s v0.2.0 -m "v0.2.0"
git push origin v0.2.0
gh run list --workflow=release.yml --limit 1
gh release view v0.2.0
```

Read `ai-rules/RELEASES.md` before you release.

## Repository bootstrap

```bash
gh repo create RAMPKORV/monsquaz-swap \
  --public \
  --source=. \
  --remote=origin \
  --push \
  --description "Anonymous Discord music-swap host (Tauri 2 + Rust + React)"

gh repo edit RAMPKORV/monsquaz-swap \
  --enable-issues=false \
  --enable-squash-merge \
  --enable-merge-commit=false \
  --enable-rebase-merge=false \
  --delete-branch-on-merge \
  --enable-secret-scanning \
  --enable-secret-scanning-push-protection
```

The local tracker is the issue source of truth. Do not enable GitHub Issues.

## Troubleshooting

| Symptom | Action |
|---|---|
| `frontendDist` not found | Run `pnpm build` before `cargo test` or `tauri build`. |
| `binary not found: openmpt123-...` | Build the sidecar for the host triple. Check the file suffix. |
| AppImage build fails in linuxdeploy | Set `APPIMAGE_EXTRACT_AND_RUN=1`. Install `libfuse2`. |
| `GLIBC_2.39 not found` | Build on Ubuntu 22.04 or in an `ubuntu:22.04` container. |
| `Resource not accessible by integration` | Add `contents: write` to the job that creates the release. |
| `release already exists` | Reuse the draft. Do not let matrix jobs create releases. |
| pnpm lockfile error | Run `pnpm install --lockfile-only`. Commit the lockfile. |
| SmartScreen warning | Expected on unsigned Windows builds. |
| Tracker database missing | Run `python3 tools/project/cli.py rebuild --yes`. |
| `PRD is not signed off` | Expected. Pass `--force --reason "D-021: no PRD sign-off gate"`. See `ai-rules/WORKFLOW.md`. |
| `ste: fail` | Rewrite the sentence in STE. See `ai-rules/skills/ste-writing/SKILL.md`. |
| `trace: ... has no test mapping` | Add `// scenario:` above the proving test, or tag the scenario `@pending`. |
| `requirements: FAILED (the z3-solver Python package is not installed)` | Run `python3 -m pip install -r specs/requirements/requirements.txt`. |
| `requirements: FAILED (... is stale)` | Run `scripts/requirements-check.sh` and commit the generated report. |
| `requirements: ... contradiction` | Read the two source links and the unsat core. Fix the natural-language source or the model. Do not weaken the rule. |
