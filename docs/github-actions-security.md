# GitHub Actions security operations

This document separates controls enforced by repository files from controls
that must be configured in GitHub. The settings snapshot below was observed on
2026-08-13 and must be rechecked before an administrative change.

## Workflow trust boundaries

- `test.yml` and `reference-environments.yml` run pull-request code only on
  GitHub-hosted runners. They receive a read-only `contents` token, no repository
  secrets, non-persistent checkout credentials, and explicit time limits.
- `benchmark.yml`, `docs.yml`, and `freeze.yml` accept manual or scheduled work
  only from `refs/heads/main`. User inputs are passed through action inputs or
  environment variables rather than interpolated into shell programs.
- `release.yml` accepts a verified release tag or a manual dispatch from
  `refs/heads/main`. The read-only build job must prove that the tag commit is
  reachable from `main` before the environment-gated job receives
  `contents: write`.
- Release and freeze artifacts are immutable within a workflow run. The
  publisher downloads the fixed-name artifact and verifies its SHA-256 sidecar
  before creating a tag or GitHub release.
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
  only the action families currently referenced by the workflows, but
  `sha_pinning_required` is `false`.
- The default workflow token permission is `write`; workflows do not have
  permission to approve pull-request reviews.
- All external contributors require approval before their pull-request
  workflows run.
- The repository has no registered self-hosted runners. Organization runner
  groups were not observable with the available `read:org`/repository token;
  the API required organization-runner administration permission.
- The `release` environment has no protection rules or deployment branch/tag
  policy, and administrators may bypass it.
- The `github-pages` environment allows only the `main` branch through a custom
  deployment policy. It has no required reviewer and administrators may bypass
  it.
- The active default-branch ruleset prevents deletion and non-fast-forward
  updates, but does not require pull requests or status checks. Organization
  administrators and the configured repository role may bypass it.
- The repository exposes one Actions secret name,
  `SLACK_DAILY_USAGEBENCH_WEBHOOK_URL`; neither environment exposes an
  environment-secret name. Secret values are never observable through the API.
- GitHub Pages uses the Actions workflow build type, is public, and enforces
  HTTPS.

## Recommended administrative settings

These changes affect collaborator or release authority and require an explicit
owner decision before they are applied:

1. Set **Workflow permissions** to **Read repository contents and packages**;
   keep **Allow GitHub Actions to create and approve pull requests** disabled.
2. Keep **Allow select actions** and enable **Require actions to be pinned to a
   full-length commit SHA**. Retain only action families referenced by the
   repository workflows.
3. Protect the `release` environment with at least one maintainer reviewer who
   is not the deployment initiator, disable administrator bypass, and add
   custom deployment policies for branch `main` and tag pattern `v*.*.*`.
4. Retain the `github-pages` `main`-only branch policy; add a maintainer reviewer
   and disable administrator bypass if Pages publication needs independent
   approval.
5. Extend the `main` ruleset to require pull requests and the exact current
   check contexts `usagebench`, `validate reproduction contract`,
   `bifrost reference image`, and `gopls reference image`. Require dismissal of
   stale approvals and conversation resolution, and limit bypass to a narrowly
   held emergency role.

## Future self-hosted runners

Do not register or route a runner as part of workflow maintenance alone. Before
introducing one:

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
