# GitHub Actions security operations

This document separates controls enforced by repository files from controls
that must be configured in GitHub. The settings snapshot below was observed on
2026-08-13 and must be rechecked before an administrative change.

## Workflow trust boundaries

- `test.yml` and `reference-environments.yml` run pull-request code only on
  GitHub-hosted runners. They receive a read-only `contents` token, no repository
  secrets, non-persistent checkout credentials, and explicit time limits.
- Reference-environment pull-request jobs force a non-publishing recipe rebuild.
  A separate job runs only on trusted `main`/manual events, receives scoped
  `packages: write`, publishes or restores a checksum-addressed GHCR image, and
  verifies it with the same offline smoke/reproduction path.
- `benchmark.yml`, `docs.yml`, and `freeze.yml` accept manual or scheduled work
  only from `refs/heads/main`. User inputs are passed through action inputs or
  environment variables rather than interpolated into shell programs.
- `native-toolchain-probe.yml` is manual-only, checks out nothing, takes no
  inputs, and runs on a GitHub-hosted `macos-26` runner with a read-only
  `contents` token. It reports what the image exposes for the freeze's native
  toolchains and executes no repository or pull-request code, so it may be
  dispatched from any ref by a user who already has write access.
- The `real-project-v2` freeze may select an explicitly approved repository
  runner label. The input accepts only `macos-26` or a random, one-job label
  matching `usagebench-ephemeral-macos-arm64-<32 lowercase hex characters>`;
  no pull-request workflow can select that label.
- The trusted evaluation path validates its ref, corpus, protocol, and profiles
  on GitHub-hosted Ubuntu before queuing exactly six macOS jobs. Those jobs are
  read-only and cover Bifrost/Java, Bifrost/Rust, Bifrost/C++, JDT LS/Java,
  rust-analyzer/Rust, and Apple clangd 21/C++ independently. GitHub-hosted
  aggregation rejects missing, duplicate, stale, checksum-mismatched, or
  incorrectly covered reports before the protected publisher can run.
- `release.yml` accepts a verified release tag or a manual dispatch from
  `refs/heads/main`. The read-only build job must prove that the tag commit is
  reachable from `main` before the environment-gated job receives
  `contents: write`.
- Release and freeze artifacts are immutable within a workflow run. The
  publisher downloads the fixed-name artifact and verifies its SHA-256 sidecar
  before creating a tag or GitHub release.
- The manual-main development freeze receives `packages: write` only to publish
  the exact release/revision reference-image identity. Retries resolve its
  lookup tag to an immutable registry digest and verify platform plus identity
  labels before reuse; pull-request jobs never receive that authority.
- Pages build and deployment are separate jobs. Only the deployment job has
  `pages: write` and `id-token: write`.
- Third-party and GitHub-authored actions are pinned to full commit SHAs. The
  adjacent comments retain the reviewed release line for updates.
- Cargo caches are used only by trusted scheduled, manual-main, and release
  workflows. Pull-request workflows do not write a cache consumed by a
  privileged job.

The Bifrost source repository is public. Its benchmark checkout therefore uses
the job token supplied by `actions/checkout`, removes the credentials before
running repository code, and does not use a cross-repository personal access
token fallback.

## Observed GitHub settings

As of the snapshot date:

- Actions are enabled with `allowed_actions: selected`. The allowlist contains
  only the action families currently referenced by the workflows, and
  `sha_pinning_required` is `true`.
- The default workflow token permission is `read`; workflows do not have
  permission to approve pull-request reviews.
- All external contributors require approval before their pull-request
  workflows run.
- The repository has one manually launched, repository-scoped, ephemeral
  macOS ARM64 runner. It has no generic `self-hosted` label, uses a random
  one-job label, and is not installed as a service. Organization runner groups
  remain unavailable to the current repository administrator; moving this
  workload to a dedicated organization group or AWS-backed ephemeral runners
  remains the intended follow-up.
- The `release` environment requires review by `DavidBakerEffendi`, permits
  that sole reviewer to approve their own deployment, rejects all refs except
  `main` and `v*.*.*` tags, and does not allow administrator bypass.
- The `github-pages` environment allows only the `main` branch through a custom
  deployment policy. It has no required reviewer and does not allow
  administrator bypass.
- The active default-branch ruleset prevents deletion and non-fast-forward
  updates; requires a pull request, one approval, dismissal of stale approvals,
  and resolved review threads; and requires the four current check contexts.
  Organization administrators and the configured repository role may bypass
  only through a pull request.
- The repository exposes one Actions secret name,
  `SLACK_DAILY_USAGEBENCH_WEBHOOK_URL`; neither environment exposes an
  environment-secret name. Secret values are never observable through the API.
- GitHub Pages uses the Actions workflow build type, is public, and enforces
  HTTPS.

## Recommended administrative settings

The remaining changes affect collaborator or release authority and require an
explicit owner decision before they are applied:

1. Move the temporary repository runner into a dedicated organization runner
   group, or replace it with AWS-backed ephemeral runners, with access limited
   to this repository and trusted workflow paths.
2. Add an independent `github-pages` reviewer if Pages publication needs a
   second approval boundary.
3. Revisit the deliberate sole-reviewer/self-review release policy when another
   maintainer is available.

## Future self-hosted runners

The temporary repository runner is a deliberate, manually authorized exception
for the trusted `real-project-v2` freeze. It must remain single-job and
repository-scoped until it is replaced. For the durable runner design:

1. Create a dedicated organization runner group with repository access limited
   to `BrokkAi/usagebench`. If the GitHub plan supports workflow restrictions,
   start with no allowed workflows; when a runner-backed job is reviewed, add
   only its trusted workflow path from `.github/workflows/benchmark.yml`,
   `.github/workflows/freeze.yml`, or `.github/workflows/release.yml`.
2. Never allow `test.yml`, `reference-environments.yml`, or any workflow with a
   `pull_request`/`pull_request_target` trigger to select that group. A future
   self-hosted job must live in a trusted-only workflow and reject every ref
   other than the intended default branch or verified release tag.
3. Put any secret-bearing or release-authority job behind its protected
   environment in addition to the runner-group restriction. Runner-group access
   is not a substitute for environment approval.
4. Use ephemeral, single-job runners with a clean workspace, no inherited cloud
   credentials, least-privilege network access, and runner software updates
   controlled by an administrator. Do not route by the generic `self-hosted`
   label alone.
5. Review cache namespaces, artifact downloads, and checkout persistence again
   when a self-hosted job is proposed. Untrusted pull-request artifacts and
   caches must never cross into the trusted runner or release path.

These restrictions concern execution trust only; they do not add a two-host
benchmark evidence requirement.

## Manually arm the six one-job runners

The reviewed repository launcher is `scripts/usagebench-ephemeral-runner`. It
does not install a service, persist a token, or alter an existing external
launcher. To install or refresh it, inspect the merged file, then explicitly
copy it to the operator-managed tools directory and make that copy executable.
Set `USAGEBENCH_RUNNER_DIST` to a separately downloaded and checksum-verified,
unpacked GitHub Actions runner distribution. Authenticate `gh` interactively;
do not put a token in the script or command line.

After merging, sync the launcher from the exact main checkout, choose one fresh
random label, dispatch the trusted manual main workflow with that exact label,
and arm exactly six registrations in a second terminal:

```bash
label="usagebench-ephemeral-macos-arm64-$(openssl rand -hex 16)"
scripts/usagebench-ephemeral-runner batch 6 "$label"
```

The workflow matrix uses `max-parallel: 1`; each ephemeral registration takes
one queued shard, deregisters, deletes its per-run directory, and only then
registers the next. The launcher best-effort deregisters and cleans the active
directory on interruption. Do not run it before the parent task has synced the
merged launcher and is ready to dispatch; do not reuse the label for another
run.
