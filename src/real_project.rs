//! Source-only capture and deterministic selection for real-project slices.
//!
//! This module intentionally has no dependency on Bifrost or an LSP. Capture
//! records the GitHub evidence needed for eligibility, while selection reads a
//! committed snapshot and performs the protocol's deterministic repository draw.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

const MAX_ARCHIVE_BYTES: u64 = 150 * 1024 * 1024;
const SOURCE_PREFIX: &[u8] = b"usagebench-real-project-v1\0";
const GITHUB_REQUEST_INTERVAL: Duration = Duration::from_millis(800);
const GITHUB_RETRY_ATTEMPTS: u32 = 3;
static LAST_GITHUB_REQUEST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct CapturePopulationOptions {
    pub protocol: PathBuf,
    pub output: PathBuf,
    pub github_api_base: String,
    pub captured_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DrawSelectionOptions {
    pub protocol: PathBuf,
    pub population: PathBuf,
    pub output: PathBuf,
    pub protocol_commit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Protocol {
    schema_version: u32,
    freeze_id: String,
    target_profiles: Vec<TargetProfile>,
    population: PopulationRules,
    sampling: Sampling,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PopulationRules {
    minimum_stars: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetProfile {
    language: String,
    candidate_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sampling {
    repositories_per_profile: usize,
    declarations_per_repository: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactLink {
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopulationSnapshot {
    pub schema_version: u32,
    pub freeze_id: String,
    pub protocol: ArtifactLink,
    pub captured_at: String,
    pub profiles: Vec<PopulationProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PopulationProfile {
    pub language: String,
    pub candidate_id: String,
    pub requests: Vec<ApiRequest>,
    pub repositories: Vec<CapturedRepository>,
}

/// Durable, private-in-progress capture state. This is deliberately separate
/// from `population.json`: the public snapshot is written only once every
/// profile's captured search frame has completed its source-only inspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureCheckpoint {
    schema_version: u32,
    freeze_id: String,
    protocol: ArtifactLink,
    captured_at: String,
    profiles: Vec<CheckpointProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointProfile {
    language: String,
    candidate_id: String,
    requests: Vec<ApiRequest>,
    repositories: Vec<CapturedRepository>,
    pending_repositories: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiRequest {
    pub method: String,
    pub url: String,
    pub page: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedRepository {
    pub full_name: String,
    pub html_url: String,
    pub default_branch: String,
    pub commit: String,
    pub repository: Value,
    pub source: SourceInspection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInspection {
    pub root_build_markers: Vec<String>,
    pub source_file_count: u64,
    pub tree_truncated: bool,
    /// GitHub's repository `size` value, converted from KiB to bytes.
    pub repository_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionManifest {
    pub schema_version: u32,
    pub freeze_id: String,
    pub protocol: ArtifactLink,
    pub population: ArtifactLink,
    pub protocol_commit: String,
    pub profiles: Vec<ProfileSelection>,
    pub replacements: Vec<ReplacementDecision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSelection {
    pub language: String,
    pub candidate_id: String,
    pub ranked: Vec<RankedRepository>,
    pub selected: Vec<SelectedRepository>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedRepository {
    pub full_name: String,
    pub source: PortableGitSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    pub eligibility: Eligibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableGitSource {
    pub repo: String,
    pub commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eligibility {
    pub eligible: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedRepository {
    pub rank: usize,
    pub full_name: String,
    pub source: PortableGitSource,
    pub case_file: String,
    pub case_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacementDecision {
    pub language: String,
    pub candidate_id: String,
    pub status: String,
    pub rule: String,
}

pub fn capture_population(options: CapturePopulationOptions) -> Result<PopulationSnapshot> {
    let (protocol, protocol_bytes) = load_protocol(&options.protocol)?;
    let api_base = options.github_api_base.trim_end_matches('/');
    let protocol_link = artifact_link(&options.protocol, &protocol_bytes)?;
    let checkpoint_path = checkpoint_path(&options.output);
    let mut checkpoint = if checkpoint_path.exists() {
        load_checkpoint(&checkpoint_path, &protocol, &protocol_link)?
    } else {
        let mut profiles = Vec::new();
        for profile in &protocol.target_profiles {
            let (requests, pending_repositories) = capture_search_population(
                api_base,
                &profile.language,
                protocol.population.minimum_stars,
            )?;
            profiles.push(CheckpointProfile {
                language: profile.language.clone(),
                candidate_id: profile.candidate_id.clone(),
                requests,
                repositories: Vec::new(),
                pending_repositories,
            });
        }
        let checkpoint = CaptureCheckpoint {
            schema_version: 1,
            freeze_id: protocol.freeze_id.clone(),
            protocol: protocol_link.clone(),
            captured_at: options.captured_at.unwrap_or_else(current_utc_timestamp),
            profiles,
        };
        write_json(&checkpoint_path, &checkpoint)?;
        checkpoint
    };

    for index in 0..checkpoint.profiles.len() {
        while let Some(repository) = checkpoint.profiles[index].pending_repositories.pop() {
            let language = checkpoint.profiles[index].language.clone();
            let (captured, requests) = capture_repository(api_base, &language, repository)?;
            checkpoint.profiles[index].requests.extend(requests);
            checkpoint.profiles[index].repositories.push(captured);
            write_json(&checkpoint_path, &checkpoint)?;
        }
    }

    let snapshot = PopulationSnapshot {
        schema_version: 1,
        freeze_id: protocol.freeze_id,
        protocol: protocol_link,
        captured_at: checkpoint.captured_at,
        profiles: checkpoint
            .profiles
            .into_iter()
            .map(|profile| PopulationProfile {
                language: profile.language,
                candidate_id: profile.candidate_id,
                requests: profile.requests,
                repositories: profile.repositories,
            })
            .collect(),
    };
    write_json(&options.output, &snapshot)?;
    fs::remove_file(&checkpoint_path).with_context(|| {
        format!(
            "remove completed capture checkpoint {}",
            checkpoint_path.display()
        )
    })?;
    Ok(snapshot)
}

fn capture_repository(
    api_base: &str,
    language: &str,
    repository: Value,
) -> Result<(CapturedRepository, Vec<ApiRequest>)> {
    let full_name = repository_string(&repository, "full_name")?;
    let html_url = repository_string(&repository, "html_url")?;
    let default_branch = repository_string(&repository, "default_branch")?;
    let commit_url = format!("{api_base}/repos/{full_name}/commits/{default_branch}");
    let commit_response = github_json(&commit_url)?;
    let mut requests = vec![api_request(&commit_url, 1, &commit_response)];
    let commit = repository_string(&commit_response, "sha")?;
    if !is_exact_git_commit(&commit) {
        bail!("GitHub returned non-exact commit {commit} for {full_name}");
    }
    let tree_url = format!("{api_base}/repos/{full_name}/git/trees/{commit}?recursive=1");
    let tree_response = github_json(&tree_url)?;
    requests.push(api_request(&tree_url, 1, &tree_response));
    let source = inspect_source(language, &tree_response, &repository)?;
    Ok((
        CapturedRepository {
            full_name,
            html_url,
            default_branch,
            commit,
            repository,
            source,
        },
        requests,
    ))
}

fn checkpoint_path(output: &Path) -> PathBuf {
    output.with_extension("partial.json")
}

fn load_checkpoint(
    path: &Path,
    protocol: &Protocol,
    protocol_link: &ArtifactLink,
) -> Result<CaptureCheckpoint> {
    let bytes =
        fs::read(path).with_context(|| format!("read capture checkpoint {}", path.display()))?;
    let checkpoint: CaptureCheckpoint = serde_json::from_slice(&bytes)
        .with_context(|| format!("deserialize capture checkpoint {}", path.display()))?;
    if checkpoint.schema_version != 1
        || checkpoint.freeze_id != protocol.freeze_id
        || checkpoint.protocol.file != protocol_link.file
        || checkpoint.protocol.sha256 != protocol_link.sha256
    {
        bail!(
            "capture checkpoint {} does not match the supplied protocol",
            path.display()
        );
    }
    Ok(checkpoint)
}

pub fn draw_selection(options: DrawSelectionOptions) -> Result<SelectionManifest> {
    if !is_exact_git_commit(&options.protocol_commit) {
        bail!("protocol commit must be exactly 40 lowercase hexadecimal characters");
    }
    let (protocol, protocol_bytes) = load_protocol(&options.protocol)?;
    let population_bytes = fs::read(&options.population)
        .with_context(|| format!("read population snapshot {}", options.population.display()))?;
    let population: PopulationSnapshot =
        serde_json::from_slice(&population_bytes).with_context(|| {
            format!(
                "deserialize population snapshot {}",
                options.population.display()
            )
        })?;
    validate_population(&protocol, &population, &options.protocol, &protocol_bytes)?;

    let mut profiles = Vec::new();
    for target in &protocol.target_profiles {
        let captured = population
            .profiles
            .iter()
            .find(|profile| {
                profile.language == target.language && profile.candidate_id == target.candidate_id
            })
            .with_context(|| {
                format!(
                    "population is missing {}/{}",
                    target.language, target.candidate_id
                )
            })?;
        profiles.push(select_profile(
            target,
            captured,
            &options.protocol_commit,
            protocol.population.minimum_stars,
            protocol.sampling.repositories_per_profile,
            protocol.sampling.declarations_per_repository,
        )?);
    }

    let manifest = SelectionManifest {
        schema_version: 1,
        freeze_id: protocol.freeze_id,
        protocol: artifact_link(&options.protocol, &protocol_bytes)?,
        population: artifact_link(&options.population, &population_bytes)?,
        protocol_commit: options.protocol_commit,
        profiles,
        replacements: protocol
            .target_profiles
            .iter()
            .map(|profile| ReplacementDecision {
                language: profile.language.clone(),
                candidate_id: profile.candidate_id.clone(),
                status: "no-replacements".to_string(),
                rule: "No selected repository has been replaced; the next eligible ranked repository is reserved for a future source-only replacement decision.".to_string(),
            })
            .collect(),
        documents: Vec::new(),
    };
    write_json(&options.output, &manifest)?;
    Ok(manifest)
}

fn select_profile(
    target: &TargetProfile,
    captured: &PopulationProfile,
    protocol_commit: &str,
    minimum_stars: u64,
    repositories_per_profile: usize,
    declarations_per_repository: usize,
) -> Result<ProfileSelection> {
    let mut ranked = captured
        .repositories
        .iter()
        .map(|repository| ranked_repository(target, repository, protocol_commit, minimum_stars))
        .collect::<Vec<_>>();
    let mut eligible = ranked
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| candidate.eligibility.eligible.then_some(index))
        .collect::<Vec<_>>();
    eligible.sort_by(|left, right| ranked[*left].digest.cmp(&ranked[*right].digest));
    for (rank, index) in eligible.iter().enumerate() {
        ranked[*index].rank = Some(rank + 1);
    }
    ranked.sort_by(|left, right| {
        left.rank
            .unwrap_or(usize::MAX)
            .cmp(&right.rank.unwrap_or(usize::MAX))
            .then_with(|| left.full_name.cmp(&right.full_name))
    });
    if eligible.len() < repositories_per_profile {
        bail!(
            "{} has only {} eligible repositories; protocol requires {}",
            target.language,
            eligible.len(),
            repositories_per_profile
        );
    }
    let selected = ranked
        .iter()
        .filter(|candidate| candidate.eligibility.eligible)
        .take(repositories_per_profile)
        .enumerate()
        .map(|(index, candidate)| {
            selected_repository(target, candidate, index + 1, declarations_per_repository)
        })
        .collect::<Vec<_>>();
    Ok(ProfileSelection {
        language: target.language.clone(),
        candidate_id: target.candidate_id.clone(),
        ranked,
        selected,
    })
}

/// The protocol requires a public, immutable population checkpoint before a
/// repository draw. The CLI calls this before it invokes [`draw_selection`].
pub fn require_committed_population(path: &Path) -> Result<()> {
    let root = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("locate Git repository for population checkpoint")?;
    if !root.status.success() {
        bail!("draw-real-project-selection must run inside the UsageBench Git repository");
    }
    let root = PathBuf::from(
        String::from_utf8(root.stdout)
            .context("decode repository root")?
            .trim(),
    );
    let absolute = path
        .canonicalize()
        .with_context(|| format!("resolve population snapshot {}", path.display()))?;
    let relative = absolute
        .strip_prefix(&root)
        .with_context(|| {
            format!(
                "population snapshot {} is outside repository",
                path.display()
            )
        })?
        .to_str()
        .context("population path is not UTF-8")?
        .replace('\\', "/");
    let committed = Command::new("git")
        .args(["show", &format!("HEAD:{relative}")])
        .output()
        .with_context(|| format!("read committed population snapshot {relative}"))?;
    if !committed.status.success() {
        bail!("population snapshot {relative} is not committed at HEAD; commit it before drawing");
    }
    let current = fs::read(&absolute)
        .with_context(|| format!("read population snapshot {}", absolute.display()))?;
    if current != committed.stdout {
        bail!("population snapshot {relative} differs from committed HEAD; commit the exact snapshot before drawing");
    }
    Ok(())
}

fn selected_repository(
    target: &TargetProfile,
    candidate: &RankedRepository,
    ordinal: usize,
    declaration_count: usize,
) -> SelectedRepository {
    let slug = format!("{}-{:02}", target.language, ordinal);
    SelectedRepository {
        rank: candidate.rank.expect("selected candidates are ranked"),
        full_name: candidate.full_name.clone(),
        source: candidate.source.clone(),
        case_file: format!("benchmarks/cases/evaluation/real-project-v1/{slug}.yaml"),
        case_ids: (1..=declaration_count)
            .map(|declaration| format!("real-project-v1-{slug}-{declaration}"))
            .collect(),
    }
}

fn ranked_repository(
    target: &TargetProfile,
    repository: &CapturedRepository,
    protocol_commit: &str,
    minimum_stars: u64,
) -> RankedRepository {
    let reasons = eligibility_reasons(target, repository, minimum_stars);
    let eligible = reasons.is_empty();
    let digest = eligible.then(|| ranking_digest(target, protocol_commit, &repository.full_name));
    RankedRepository {
        full_name: repository.full_name.clone(),
        source: PortableGitSource {
            repo: format!("https://github.com/{}", repository.full_name),
            commit: repository.commit.clone(),
        },
        rank: None,
        digest,
        eligibility: Eligibility { eligible, reasons },
    }
}

fn eligibility_reasons(
    target: &TargetProfile,
    repository: &CapturedRepository,
    minimum_stars: u64,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if repository
        .repository
        .get("private")
        .and_then(Value::as_bool)
        != Some(false)
    {
        reasons.push("repository is not public".to_string());
    }
    if repository.repository.get("fork").and_then(Value::as_bool) != Some(false) {
        reasons.push("repository is a fork".to_string());
    }
    if repository
        .repository
        .get("archived")
        .and_then(Value::as_bool)
        != Some(false)
    {
        reasons.push("repository is archived".to_string());
    }
    if repository
        .repository
        .get("mirror_url")
        .is_some_and(|value| !value.is_null())
    {
        reasons.push("repository is a mirror".to_string());
    }
    if repository
        .repository
        .get("stargazers_count")
        .and_then(Value::as_u64)
        .unwrap_or_default()
        < minimum_stars
    {
        reasons.push(format!("repository has fewer than {minimum_stars} stars"));
    }
    let spdx = repository
        .repository
        .get("license")
        .and_then(|license| license.get("spdx_id"))
        .and_then(Value::as_str);
    if spdx.is_none_or(|value| value == "NOASSERTION") {
        reasons.push("repository lacks a GitHub-reported SPDX license".to_string());
    }
    let required_markers =
        build_markers(&target.language).expect("protocol language was validated");
    if !required_markers.iter().any(|marker| {
        repository
            .source
            .root_build_markers
            .iter()
            .any(|observed| observed == *marker)
    }) {
        reasons.push(format!(
            "repository lacks root build marker for {}",
            target.language
        ));
    }
    if repository.source.tree_truncated {
        reasons.push("repository source tree is truncated by GitHub".to_string());
    }
    if repository.source.source_file_count < 20 {
        reasons.push("repository has fewer than 20 eligible source files".to_string());
    }
    if repository.source.repository_size_bytes > MAX_ARCHIVE_BYTES {
        reasons.push("repository reported source size exceeds 150 MiB".to_string());
    }
    if !is_exact_git_commit(&repository.commit) {
        reasons.push("repository default-branch commit is not exact".to_string());
    }
    reasons
}

fn validate_population(
    protocol: &Protocol,
    population: &PopulationSnapshot,
    protocol_path: &Path,
    protocol_bytes: &[u8],
) -> Result<()> {
    if population.schema_version != 1 || protocol.schema_version != 1 {
        bail!("real-project population and protocol schemas must be version 1");
    }
    if population.freeze_id != protocol.freeze_id {
        bail!("population freezeId does not match protocol");
    }
    let expected = artifact_link(protocol_path, protocol_bytes)?;
    if population.protocol.file != expected.file || population.protocol.sha256 != expected.sha256 {
        bail!("population is not bound to the supplied protocol bytes");
    }
    let profile_ids = population
        .profiles
        .iter()
        .map(|profile| format!("{}/{}", profile.language, profile.candidate_id))
        .collect::<BTreeSet<_>>();
    let expected_ids = protocol
        .target_profiles
        .iter()
        .map(|profile| format!("{}/{}", profile.language, profile.candidate_id))
        .collect::<BTreeSet<_>>();
    if profile_ids != expected_ids {
        bail!("population profiles do not match the protocol");
    }
    Ok(())
}

fn inspect_source(language: &str, tree: &Value, repository: &Value) -> Result<SourceInspection> {
    let entries = tree
        .get("tree")
        .and_then(Value::as_array)
        .context("GitHub tree response missing tree")?;
    let root_build_markers = entries
        .iter()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .filter(|path| !path.contains('/'))
        .filter(|path| {
            build_markers(language)
                .expect("language was validated")
                .iter()
                .any(|marker| *marker == *path)
        })
        .map(ToOwned::to_owned)
        .collect();
    let source_file_count = entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("blob"))
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .filter(|path| is_source_file(language, path))
        .count() as u64;
    let repository_size_bytes = repository
        .get("size")
        .and_then(Value::as_u64)
        .context("GitHub repository response missing numeric size")?
        .saturating_mul(1024);
    Ok(SourceInspection {
        root_build_markers,
        source_file_count,
        tree_truncated: tree
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        repository_size_bytes,
    })
}

fn is_source_file(language: &str, path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    if path.split('/').any(|component| {
        matches!(
            component,
            "vendor"
                | "node_modules"
                | "dist"
                | "build"
                | "generated"
                | "gen"
                | "test"
                | "tests"
                | "__tests__"
                | "examples"
                | "example"
                | "benchmarks"
                | "benchmark"
        )
    }) {
        return false;
    }
    if path.ends_with("_test.go") || path.ends_with(".d.ts") {
        return false;
    }
    match language {
        "go" => path.ends_with(".go"),
        "python" => path.ends_with(".py"),
        "typescript" => path.ends_with(".ts") || path.ends_with(".tsx"),
        _ => false,
    }
}

fn build_markers(language: &str) -> Result<&'static [&'static str]> {
    match language {
        "go" => Ok(&["go.mod"]),
        "python" => Ok(&["pyproject.toml", "setup.cfg"]),
        "typescript" => Ok(&["package.json", "tsconfig.json"]),
        other => bail!("unsupported real-project language {other}"),
    }
}

fn github_language(language: &str) -> Result<&'static str> {
    match language {
        "go" => Ok("Go"),
        "python" => Ok("Python"),
        "typescript" => Ok("TypeScript"),
        other => bail!("unsupported real-project language {other}"),
    }
}

fn github_json(url: &str) -> Result<Value> {
    retry_github_request("GitHub request", || github_json_once(url))
}

fn github_json_once(url: &str) -> Result<Value> {
    rate_limit_github_api();
    let mut command = Command::new("curl");
    command.args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--header",
        "Accept: application/vnd.github+json",
    ]);
    if let Some(token) = github_token() {
        command
            .arg("--header")
            .arg(format!("Authorization: Bearer {token}"));
    }
    let output = command
        .arg(url)
        .output()
        .with_context(|| format!("run curl for {url}"))?;
    if !output.status.success() {
        bail!(
            "GitHub request failed for {url}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    serde_json::from_slice(&output.stdout).with_context(|| format!("decode GitHub JSON from {url}"))
}

fn rate_limit_github_api() {
    let last_request = LAST_GITHUB_REQUEST.get_or_init(|| Mutex::new(None));
    let mut last_request = last_request
        .lock()
        .expect("GitHub rate limiter is not poisoned");
    if let Some(last_request_at) = *last_request {
        let elapsed = last_request_at.elapsed();
        if elapsed < GITHUB_REQUEST_INTERVAL {
            thread::sleep(GITHUB_REQUEST_INTERVAL - elapsed);
        }
    }
    *last_request = Some(Instant::now());
}

fn retry_github_request<T>(description: &str, mut request: impl FnMut() -> Result<T>) -> Result<T> {
    let mut last_error = None;
    for attempt in 1..=GITHUB_RETRY_ATTEMPTS {
        match request() {
            Ok(response) => return Ok(response),
            Err(error) => {
                last_error = Some(error);
                if attempt < GITHUB_RETRY_ATTEMPTS {
                    thread::sleep(Duration::from_secs(u64::from(attempt)));
                }
            }
        }
    }
    Err(last_error.expect("a retry loop always records an error"))
        .with_context(|| format!("{description} failed after {GITHUB_RETRY_ATTEMPTS} attempts"))
}

fn github_token() -> Option<String> {
    ["GITHUB_TOKEN", "GH_TOKEN"]
        .into_iter()
        .find_map(|name| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            let output = Command::new("gh").args(["auth", "token"]).output().ok()?;
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            } else {
                None
            }
        })
}

/// GitHub Search returns at most 1,000 results per query. Partitioning on the
/// integer star count makes the complete returned population auditable without
/// silently dropping the tail of a broad language query.
fn capture_search_population(
    api_base: &str,
    language: &str,
    minimum_stars: u64,
) -> Result<(Vec<ApiRequest>, Vec<Value>)> {
    let github_language = github_language(language)?;
    let discovery_query =
        format!("language:{github_language} stars:>={minimum_stars} archived:false fork:false");
    let discovery_url = search_url(api_base, &discovery_query, 1);
    let discovery = github_json(&discovery_url)?;
    let mut requests = vec![api_request(&discovery_url, 1, &discovery)];
    let highest_stars = discovery
        .get("items")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| item.get("stargazers_count"))
        .and_then(Value::as_u64)
        .with_context(|| {
            format!("GitHub search returned no repository at or above {minimum_stars} stars")
        })?;
    let mut repositories = BTreeMap::new();
    capture_star_window(
        api_base,
        github_language,
        minimum_stars,
        highest_stars,
        &mut requests,
        &mut repositories,
    )?;
    Ok((requests, repositories.into_values().collect()))
}

fn capture_star_window(
    api_base: &str,
    language: &str,
    low: u64,
    high: u64,
    requests: &mut Vec<ApiRequest>,
    repositories: &mut BTreeMap<String, Value>,
) -> Result<()> {
    let query = format!("language:{language} stars:{low}..{high} archived:false fork:false");
    let first_url = search_url(api_base, &query, 1);
    let first = github_json(&first_url)?;
    let total_count = first
        .get("total_count")
        .and_then(Value::as_u64)
        .context("GitHub search response missing total_count")?;
    requests.push(api_request(&first_url, 1, &first));
    if total_count > 1000 {
        if low == high {
            bail!(
                "GitHub search star bucket {low} has {total_count} repositories; the API cannot return a complete population without an additional recorded partition"
            );
        }
        let middle = low + (high - low) / 2;
        capture_star_window(api_base, language, low, middle, requests, repositories)?;
        capture_star_window(api_base, language, middle + 1, high, requests, repositories)?;
        return Ok(());
    }
    add_search_items(&first, repositories)?;
    let mut page = 2;
    while (page - 1) * 100 < total_count as u32 {
        let url = search_url(api_base, &query, page);
        let response = github_json(&url)?;
        requests.push(api_request(&url, page, &response));
        add_search_items(&response, repositories)?;
        page += 1;
    }
    Ok(())
}

fn add_search_items(response: &Value, repositories: &mut BTreeMap<String, Value>) -> Result<()> {
    let items = response
        .get("items")
        .and_then(Value::as_array)
        .context("GitHub search response missing items")?;
    for item in items {
        repositories.insert(repository_string(item, "full_name")?, item.clone());
    }
    Ok(())
}

fn search_url(api_base: &str, query: &str, page: u32) -> String {
    format!(
        "{api_base}/search/repositories?q={}&sort=stars&order=desc&per_page=100&page={page}",
        percent_encode_query(query)
    )
}

fn api_request(url: &str, page: u32, response: &Value) -> ApiRequest {
    ApiRequest {
        method: "GET".to_string(),
        url: url.to_string(),
        page,
        response_sha256: Some(sha256(
            &serde_json::to_vec(response).expect("serialize GitHub response"),
        )),
        response_bytes: None,
    }
}

fn load_protocol(path: &Path) -> Result<(Protocol, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read protocol {}", path.display()))?;
    let protocol = serde_json::from_slice(&bytes)
        .with_context(|| format!("deserialize protocol {}", path.display()))?;
    Ok((protocol, bytes))
}

fn artifact_link(path: &Path, bytes: &[u8]) -> Result<ArtifactLink> {
    let file = path
        .to_str()
        .context("artifact path is not UTF-8")?
        .replace('\\', "/");
    Ok(ArtifactLink {
        file,
        sha256: sha256(bytes),
    })
}

fn repository_string(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .with_context(|| format!("GitHub response missing string {key}"))
}

fn ranking_digest(target: &TargetProfile, protocol_commit: &str, full_name: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [
        SOURCE_PREFIX,
        protocol_commit.as_bytes(),
        b"\0",
        target.language.as_bytes(),
        b"\0",
        target.candidate_id.as_bytes(),
        b"\0",
        full_name.as_bytes(),
    ] {
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

fn is_exact_git_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn percent_encode_query(query: &str) -> String {
    query
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

fn current_utc_timestamp() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .expect("run date for capture timestamp");
    String::from_utf8(output.stdout)
        .expect("date timestamp is UTF-8")
        .trim()
        .to_string()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("serialize JSON artifact")?;
    fs::write(
        path,
        format!("{}\n", String::from_utf8(bytes).expect("JSON is UTF-8")),
    )
    .with_context(|| format!("write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn protocol() -> Protocol {
        Protocol {
            schema_version: 1,
            freeze_id: "real-project-v1".to_string(),
            target_profiles: vec![TargetProfile {
                language: "go".to_string(),
                candidate_id: "gopls".to_string(),
            }],
            population: PopulationRules { minimum_stars: 100 },
            sampling: Sampling {
                repositories_per_profile: 2,
                declarations_per_repository: 3,
            },
        }
    }

    fn repository(name: &str, stars: u64) -> CapturedRepository {
        CapturedRepository {
            full_name: name.to_string(),
            html_url: format!("https://github.com/{name}"),
            default_branch: "main".to_string(),
            commit: "a".repeat(40),
            repository: serde_json::json!({"private": false, "fork": false, "archived": false, "mirror_url": null, "stargazers_count": stars, "license": {"spdx_id": "MIT"}}),
            source: SourceInspection {
                root_build_markers: vec!["go.mod".to_string()],
                source_file_count: 20,
                tree_truncated: false,
                repository_size_bytes: 42,
            },
        }
    }

    #[test]
    fn source_inspection_uses_repository_size_metadata() {
        let tree = serde_json::json!({
            "truncated": false,
            "tree": [{"type": "blob", "path": "go.mod"}, {"type": "blob", "path": "main.go"}]
        });
        let repository = serde_json::json!({"size": 42});

        assert_eq!(
            inspect_source("go", &tree, &repository)
                .unwrap()
                .repository_size_bytes,
            42 * 1024
        );
    }

    #[test]
    fn draw_ranks_only_eligible_repositories_and_preserves_exclusions() {
        let directory = tempdir().unwrap();
        let protocol_path = directory.path().join("protocol.json");
        let protocol_bytes = serde_json::to_vec(&serde_json::json!({"schemaVersion": 1, "freezeId": "real-project-v1", "targetProfiles": [{"language": "go", "candidateId": "gopls"}], "population": {"minimumStars": 100}, "sampling": {"repositoriesPerProfile": 2, "declarationsPerRepository": 3}})).unwrap();
        fs::write(&protocol_path, &protocol_bytes).unwrap();
        let population_path = directory.path().join("population.json");
        let snapshot = PopulationSnapshot {
            schema_version: 1,
            freeze_id: "real-project-v1".to_string(),
            protocol: artifact_link(&protocol_path, &protocol_bytes).unwrap(),
            captured_at: "2026-07-29T00:00:00Z".to_string(),
            profiles: vec![PopulationProfile {
                language: "go".to_string(),
                candidate_id: "gopls".to_string(),
                requests: vec![],
                repositories: vec![
                    repository("owner/eligible-a", 100),
                    repository("owner/excluded", 99),
                    repository("owner/eligible-b", 101),
                ],
            }],
        };
        write_json(&population_path, &snapshot).unwrap();
        let output = directory.path().join("selection.json");
        let selection = draw_selection(DrawSelectionOptions {
            protocol: protocol_path,
            population: population_path,
            output,
            protocol_commit: "b".repeat(40),
        })
        .unwrap();
        assert_eq!(selection.profiles[0].selected.len(), 2);
        assert_eq!(
            selection.profiles[0]
                .ranked
                .iter()
                .filter(|candidate| candidate.eligibility.eligible)
                .count(),
            2
        );
        assert_eq!(
            selection.profiles[0]
                .ranked
                .iter()
                .filter(|candidate| !candidate.eligibility.eligible)
                .count(),
            1
        );
        assert!(selection.profiles[0]
            .selected
            .iter()
            .all(|selected| selected.case_ids.len() == 3));
    }

    #[test]
    fn source_filter_excludes_generated_and_test_files() {
        assert!(is_source_file("go", "cmd/main.go"));
        assert!(!is_source_file("go", "cmd/main_test.go"));
        assert!(!is_source_file("typescript", "node_modules/pkg/index.ts"));
        assert!(!is_source_file("python", "examples/demo.py"));
    }

    #[test]
    fn rank_digest_uses_nul_delimiters() {
        let target = TargetProfile {
            language: "go".to_string(),
            candidate_id: "gopls".to_string(),
        };
        assert_eq!(
            ranking_digest(&target, &"b".repeat(40), "owner/repo"),
            "9cd334272668821f12055d7a7265038e5f81900fcc6378e8a54803df9f63e652"
        );
    }

    #[test]
    fn protocol_fixture_is_well_formed() {
        assert_eq!(protocol().sampling.repositories_per_profile, 2);
    }

    #[test]
    fn checkpoint_round_trips_only_when_bound_to_the_same_protocol() {
        let directory = tempdir().unwrap();
        let protocol_path = directory.path().join("protocol.json");
        let protocol_bytes = serde_json::to_vec(&serde_json::json!({"schemaVersion": 1, "freezeId": "real-project-v1", "targetProfiles": [{"language": "go", "candidateId": "gopls"}], "population": {"minimumStars": 100}, "sampling": {"repositoriesPerProfile": 2, "declarationsPerRepository": 3}})).unwrap();
        fs::write(&protocol_path, &protocol_bytes).unwrap();
        let checkpoint_path = directory.path().join("population.partial.json");
        let checkpoint = CaptureCheckpoint {
            schema_version: 1,
            freeze_id: "real-project-v1".to_string(),
            protocol: artifact_link(&protocol_path, &protocol_bytes).unwrap(),
            captured_at: "2026-07-29T00:00:00Z".to_string(),
            profiles: vec![],
        };
        write_json(&checkpoint_path, &checkpoint).unwrap();
        let loaded = load_checkpoint(&checkpoint_path, &protocol(), &checkpoint.protocol).unwrap();
        assert_eq!(loaded.captured_at, checkpoint.captured_at);
    }
}
