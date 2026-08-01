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

const LEGACY_MAX_ARCHIVE_BYTES: u64 = 150 * 1024 * 1024;
const LEGACY_MINIMUM_SOURCE_FILES: u64 = 20;
const LEGACY_EXCLUDED_PATH_COMPONENTS: &[&str] = &[
    "vendor",
    "node_modules",
    "dist",
    "build",
    "generated",
    "gen",
    "test",
    "tests",
    "__tests__",
    "examples",
    "example",
    "benchmarks",
    "benchmark",
];
const GITHUB_REQUEST_INTERVAL: Duration = Duration::from_millis(800);
const GITHUB_RETRY_ATTEMPTS: u32 = 3;
const MAX_PRIOR_SELECTION_BYTES: u64 = 16 * 1024 * 1024;
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
    #[serde(default = "legacy_minimum_source_files")]
    minimum_source_files: u64,
    #[serde(default = "legacy_maximum_repository_size_bytes")]
    maximum_repository_size_bytes: u64,
    #[serde(default)]
    excluded_path_components: Vec<String>,
    #[serde(default)]
    prior_selections: Vec<ArtifactLink>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetProfile {
    language: String,
    candidate_id: String,
    #[serde(default)]
    github_language: Option<String>,
    #[serde(default)]
    root_build_markers: Vec<String>,
    #[serde(default)]
    source_extensions: Vec<String>,
    #[serde(default)]
    excluded_file_suffixes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sampling {
    repositories_per_profile: usize,
    declarations_per_repository: usize,
    #[serde(default)]
    minimum_replacement_repositories: usize,
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
                profile,
                protocol.schema_version,
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
        let target = protocol
            .target_profiles
            .iter()
            .find(|target| {
                target.language == checkpoint.profiles[index].language
                    && target.candidate_id == checkpoint.profiles[index].candidate_id
            })
            .cloned()
            .context("capture checkpoint profile is not present in the protocol")?;
        while let Some(repository) = checkpoint.profiles[index].pending_repositories.pop() {
            let (captured, requests) = capture_repository(
                api_base,
                &target,
                &protocol.population,
                protocol.schema_version,
                repository,
            )?;
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
    target: &TargetProfile,
    population: &PopulationRules,
    protocol_schema_version: u32,
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
    let source = inspect_source(
        target,
        population,
        protocol_schema_version,
        &tree_response,
        &repository,
    )?;
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
    draw_selection_with_options(options, true, true)
}

fn draw_selection_with_options(
    options: DrawSelectionOptions,
    verify_commit: bool,
    validate_schema: bool,
) -> Result<SelectionManifest> {
    if !is_exact_git_commit(&options.protocol_commit) {
        bail!("protocol commit must be exactly 40 lowercase hexadecimal characters");
    }
    let (protocol, protocol_bytes) = if validate_schema {
        load_protocol(&options.protocol)?
    } else {
        load_protocol_unchecked(&options.protocol)?
    };
    if verify_commit {
        verify_protocol_commit(&options.protocol, &protocol_bytes, &options.protocol_commit)?;
    }
    let (population, population_bytes): (PopulationSnapshot, Vec<u8>) = if validate_schema {
        crate::evaluation::load_evaluation_population_checked(&options.population)?
    } else {
        let bytes = fs::read(&options.population).with_context(|| {
            format!("read population snapshot {}", options.population.display())
        })?;
        let population = serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "deserialize population snapshot {}",
                options.population.display()
            )
        })?;
        (population, bytes)
    };
    validate_population(&protocol, &population, &options.protocol, &protocol_bytes)?;
    let prior_repositories = load_prior_repositories(&protocol, &options.protocol)?;

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
            &protocol,
            target,
            captured,
            &options.protocol_commit,
            &prior_repositories,
        )?);
    }

    let replacement_rule = replacement_rule(&protocol);
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
                rule: replacement_rule.clone(),
            })
            .collect(),
        documents: Vec::new(),
    };
    write_json(&options.output, &manifest)?;
    Ok(manifest)
}

fn replacement_rule(protocol: &Protocol) -> String {
    if protocol.schema_version == 1 && protocol.sampling.minimum_replacement_repositories == 0 {
        return "No selected repository has been replaced; the next eligible ranked repository is reserved for a future source-only replacement decision.".to_string();
    }
    format!(
        "No selected repository has been replaced; at least {} eligible repositories after the selected prefix are reserved for future source-only replacement decisions.",
        protocol.sampling.minimum_replacement_repositories
    )
}

fn select_profile(
    protocol: &Protocol,
    target: &TargetProfile,
    captured: &PopulationProfile,
    protocol_commit: &str,
    prior_repositories: &BTreeSet<String>,
) -> Result<ProfileSelection> {
    let mut ranked = captured
        .repositories
        .iter()
        .map(|repository| {
            ranked_repository(
                protocol,
                target,
                repository,
                protocol_commit,
                prior_repositories,
            )
        })
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
    let required_eligible = protocol
        .sampling
        .repositories_per_profile
        .checked_add(protocol.sampling.minimum_replacement_repositories)
        .context("real-project repository count overflow")?;
    if eligible.len() < required_eligible {
        bail!(
            "{} has only {} eligible repositories; protocol requires {} selected plus {} replacements",
            target.language,
            eligible.len(),
            protocol.sampling.repositories_per_profile,
            protocol.sampling.minimum_replacement_repositories,
        );
    }
    let selected = ranked
        .iter()
        .filter(|candidate| candidate.eligibility.eligible)
        .take(protocol.sampling.repositories_per_profile)
        .enumerate()
        .map(|(index, candidate)| {
            selected_repository(
                &protocol.freeze_id,
                target,
                candidate,
                index + 1,
                protocol.sampling.declarations_per_repository,
            )
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
    freeze_id: &str,
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
        case_file: format!("benchmarks/cases/evaluation/{freeze_id}/{slug}.yaml"),
        case_ids: (1..=declaration_count)
            .map(|declaration| format!("{freeze_id}-{slug}-{declaration}"))
            .collect(),
    }
}

fn ranked_repository(
    protocol: &Protocol,
    target: &TargetProfile,
    repository: &CapturedRepository,
    protocol_commit: &str,
    prior_repositories: &BTreeSet<String>,
) -> RankedRepository {
    let reasons = eligibility_reasons(protocol, target, repository, prior_repositories);
    let eligible = reasons.is_empty();
    let digest = eligible.then(|| {
        ranking_digest(
            &protocol.freeze_id,
            target,
            protocol_commit,
            &repository.full_name,
        )
    });
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
    protocol: &Protocol,
    target: &TargetProfile,
    repository: &CapturedRepository,
    prior_repositories: &BTreeSet<String>,
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
        < protocol.population.minimum_stars
    {
        reasons.push(format!(
            "repository has fewer than {} stars",
            protocol.population.minimum_stars
        ));
    }
    let spdx = repository
        .repository
        .get("license")
        .and_then(|license| license.get("spdx_id"))
        .and_then(Value::as_str);
    if spdx.is_none_or(|value| value == "NOASSERTION") {
        reasons.push("repository lacks a GitHub-reported SPDX license".to_string());
    }
    let required_markers = target
        .resolved_build_markers(protocol.schema_version)
        .expect("protocol language was validated");
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
    if repository.source.source_file_count < protocol.population.minimum_source_files {
        reasons.push(format!(
            "repository has fewer than {} eligible source files",
            protocol.population.minimum_source_files
        ));
    }
    if repository.source.repository_size_bytes > protocol.population.maximum_repository_size_bytes {
        reasons.push(format!(
            "repository reported source size exceeds {} bytes",
            protocol.population.maximum_repository_size_bytes
        ));
    }
    if !is_exact_git_commit(&repository.commit) {
        reasons.push("repository default-branch commit is not exact".to_string());
    }
    if prior_repositories.contains(&repository.full_name.to_ascii_lowercase()) {
        reasons.push("repository was selected by a prior evaluation slice".to_string());
    }
    reasons
}

fn validate_population(
    protocol: &Protocol,
    population: &PopulationSnapshot,
    protocol_path: &Path,
    protocol_bytes: &[u8],
) -> Result<()> {
    if population.schema_version != 1 || !matches!(protocol.schema_version, 1 | 2) {
        bail!("real-project population schema must be version 1 and protocol schema must be version 1 or 2");
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

fn inspect_source(
    target: &TargetProfile,
    population: &PopulationRules,
    protocol_schema_version: u32,
    tree: &Value,
    repository: &Value,
) -> Result<SourceInspection> {
    let entries = tree
        .get("tree")
        .and_then(Value::as_array)
        .context("GitHub tree response missing tree")?;
    let build_markers = target.resolved_build_markers(protocol_schema_version)?;
    let root_build_markers = entries
        .iter()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .filter(|path| !path.contains('/'))
        .filter(|path| build_markers.iter().any(|marker| *marker == *path))
        .map(ToOwned::to_owned)
        .collect();
    let source_file_count = entries
        .iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("blob"))
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .filter(|path| is_source_file(target, population, protocol_schema_version, path))
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

fn is_source_file(
    target: &TargetProfile,
    population: &PopulationRules,
    protocol_schema_version: u32,
    path: &str,
) -> bool {
    let path = path.to_ascii_lowercase();
    let excluded_components = population.resolved_excluded_path_components();
    if path.split('/').any(|component| {
        excluded_components
            .iter()
            .any(|excluded| *excluded == component)
    }) {
        return false;
    }
    let excluded_suffixes = target
        .resolved_excluded_file_suffixes(protocol_schema_version)
        .expect("protocol language was validated");
    if excluded_suffixes
        .iter()
        .any(|suffix| path.ends_with(suffix))
    {
        return false;
    }
    target
        .resolved_source_extensions(protocol_schema_version)
        .expect("protocol language was validated")
        .iter()
        .any(|extension| path.ends_with(extension))
}

impl PopulationRules {
    fn resolved_excluded_path_components(&self) -> Vec<&str> {
        if self.excluded_path_components.is_empty() {
            LEGACY_EXCLUDED_PATH_COMPONENTS.to_vec()
        } else {
            self.excluded_path_components
                .iter()
                .map(String::as_str)
                .collect()
        }
    }
}

impl TargetProfile {
    fn resolved_github_language(&self, protocol_schema_version: u32) -> Result<&str> {
        if let Some(language) = self.github_language.as_deref() {
            return Ok(language);
        }
        if protocol_schema_version == 1 {
            return legacy_github_language(&self.language);
        }
        bail!(
            "real-project-v2 profile {}/{} is missing githubLanguage",
            self.language,
            self.candidate_id
        )
    }

    fn resolved_build_markers(&self, protocol_schema_version: u32) -> Result<Vec<&str>> {
        if !self.root_build_markers.is_empty() {
            return Ok(self.root_build_markers.iter().map(String::as_str).collect());
        }
        if protocol_schema_version == 1 {
            return legacy_build_markers(&self.language).map(<[_]>::to_vec);
        }
        bail!(
            "real-project-v2 profile {}/{} is missing rootBuildMarkers",
            self.language,
            self.candidate_id
        )
    }

    fn resolved_source_extensions(&self, protocol_schema_version: u32) -> Result<Vec<&str>> {
        if !self.source_extensions.is_empty() {
            return Ok(self.source_extensions.iter().map(String::as_str).collect());
        }
        if protocol_schema_version == 1 {
            return legacy_source_extensions(&self.language).map(<[_]>::to_vec);
        }
        bail!(
            "real-project-v2 profile {}/{} is missing sourceExtensions",
            self.language,
            self.candidate_id
        )
    }

    fn resolved_excluded_file_suffixes(&self, protocol_schema_version: u32) -> Result<Vec<&str>> {
        if !self.excluded_file_suffixes.is_empty() {
            return Ok(self
                .excluded_file_suffixes
                .iter()
                .map(String::as_str)
                .collect());
        }
        if protocol_schema_version == 1 {
            return legacy_excluded_file_suffixes(&self.language).map(<[_]>::to_vec);
        }
        Ok(Vec::new())
    }
}

fn legacy_github_language(language: &str) -> Result<&'static str> {
    match language {
        "go" => Ok("Go"),
        "python" => Ok("Python"),
        "typescript" => Ok("TypeScript"),
        other => bail!("unsupported real-project-v1 language {other}"),
    }
}

fn legacy_build_markers(language: &str) -> Result<&'static [&'static str]> {
    match language {
        "go" => Ok(&["go.mod"]),
        "python" => Ok(&["pyproject.toml", "setup.cfg"]),
        "typescript" => Ok(&["package.json", "tsconfig.json"]),
        other => bail!("unsupported real-project-v1 language {other}"),
    }
}

fn legacy_source_extensions(language: &str) -> Result<&'static [&'static str]> {
    match language {
        "go" => Ok(&[".go"]),
        "python" => Ok(&[".py"]),
        "typescript" => Ok(&[".ts", ".tsx"]),
        other => bail!("unsupported real-project-v1 language {other}"),
    }
}

fn legacy_excluded_file_suffixes(language: &str) -> Result<&'static [&'static str]> {
    match language {
        "go" => Ok(&["_test.go"]),
        "python" => Ok(&[]),
        "typescript" => Ok(&[".d.ts"]),
        other => bail!("unsupported real-project-v1 language {other}"),
    }
}

fn legacy_minimum_source_files() -> u64 {
    LEGACY_MINIMUM_SOURCE_FILES
}

fn legacy_maximum_repository_size_bytes() -> u64 {
    LEGACY_MAX_ARCHIVE_BYTES
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
    target: &TargetProfile,
    protocol_schema_version: u32,
    minimum_stars: u64,
) -> Result<(Vec<ApiRequest>, Vec<Value>)> {
    let github_language = target.resolved_github_language(protocol_schema_version)?;
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
    let (protocol, bytes) = crate::evaluation::load_evaluation_protocol_checked(path)?;
    validate_protocol(&protocol)?;
    Ok((protocol, bytes))
}

fn load_protocol_unchecked(path: &Path) -> Result<(Protocol, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read protocol {}", path.display()))?;
    let protocol = serde_json::from_slice(&bytes)
        .with_context(|| format!("deserialize protocol {}", path.display()))?;
    validate_protocol(&protocol)?;
    Ok((protocol, bytes))
}

fn validate_protocol(protocol: &Protocol) -> Result<()> {
    if !matches!(protocol.schema_version, 1 | 2) {
        bail!("real-project protocol schemaVersion must be 1 or 2");
    }
    if protocol.freeze_id.is_empty()
        || !protocol
            .freeze_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        bail!("real-project protocol freezeId must be a lowercase path-safe slug");
    }
    if protocol.population.minimum_stars == 0
        || protocol.population.minimum_source_files == 0
        || protocol.population.maximum_repository_size_bytes == 0
        || protocol.sampling.repositories_per_profile == 0
        || protocol.sampling.declarations_per_repository == 0
    {
        bail!("real-project protocol thresholds and sample counts must be positive");
    }
    if protocol.schema_version == 2 {
        if protocol.population.excluded_path_components.is_empty()
            || protocol.population.prior_selections.is_empty()
            || protocol.sampling.minimum_replacement_repositories == 0
        {
            bail!("real-project-v2 protocol must record source exclusions, prior selections, and replacement headroom");
        }
        for target in &protocol.target_profiles {
            target.resolved_github_language(protocol.schema_version)?;
            target.resolved_build_markers(protocol.schema_version)?;
            target.resolved_source_extensions(protocol.schema_version)?;
        }
    }
    let identities = protocol
        .target_profiles
        .iter()
        .map(|target| (&target.language, &target.candidate_id))
        .collect::<BTreeSet<_>>();
    if identities.len() != protocol.target_profiles.len() {
        bail!("real-project protocol contains a duplicate language/candidate profile");
    }
    Ok(())
}

fn verify_protocol_commit(path: &Path, bytes: &[u8], commit: &str) -> Result<()> {
    let repo_root = crate::find_repo_root_for_path(path)?;
    let canonical_root = repo_root
        .canonicalize()
        .with_context(|| format!("resolve repository root {}", repo_root.display()))?;
    let absolute = path
        .canonicalize()
        .with_context(|| format!("resolve protocol {}", path.display()))?;
    let relative = absolute
        .strip_prefix(&canonical_root)
        .with_context(|| format!("protocol {} is outside repository", path.display()))?
        .to_str()
        .context("protocol path is not UTF-8")?
        .replace('\\', "/");
    let committed = Command::new("git")
        .arg("-C")
        .arg(&repo_root)
        .args(["show", &format!("{commit}:{relative}")])
        .output()
        .with_context(|| format!("read protocol {relative} from commit {commit}"))?;
    if !committed.status.success() {
        bail!("protocol commit {commit} does not contain {relative}");
    }
    verify_protocol_commit_bytes(bytes, &committed.stdout, commit)
}

fn verify_protocol_commit_bytes(current: &[u8], committed: &[u8], commit: &str) -> Result<()> {
    if current != committed {
        bail!("protocol bytes do not match protocol commit {commit}");
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PriorSelection {
    profiles: Vec<PriorSelectionProfile>,
}

#[derive(Debug, Deserialize)]
struct PriorSelectionProfile {
    selected: Vec<PriorSelectedRepository>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PriorSelectedRepository {
    full_name: String,
}

fn load_prior_repositories(protocol: &Protocol, protocol_path: &Path) -> Result<BTreeSet<String>> {
    let mut repositories = BTreeSet::new();
    if protocol.population.prior_selections.is_empty() {
        return Ok(repositories);
    }
    let repo_root = crate::find_repo_root_for_path(protocol_path)?;
    let canonical_root = repo_root
        .canonicalize()
        .with_context(|| format!("resolve repository root {}", repo_root.display()))?;
    for link in &protocol.population.prior_selections {
        let relative = crate::evaluation::safe_repo_relative_path(
            &link.file,
            "prior selection artifact path",
        )?;
        let path = repo_root.join(relative);
        let canonical = path
            .canonicalize()
            .with_context(|| format!("resolve prior selection {}", path.display()))?;
        if !canonical.starts_with(&canonical_root) {
            bail!("prior selection {} escapes the repository", path.display());
        }
        let metadata = canonical
            .metadata()
            .with_context(|| format!("inspect prior selection {}", path.display()))?;
        if !metadata.is_file() {
            bail!("prior selection {} is not a regular file", path.display());
        }
        if metadata.len() > MAX_PRIOR_SELECTION_BYTES {
            bail!(
                "prior selection {} exceeds {} bytes",
                path.display(),
                MAX_PRIOR_SELECTION_BYTES
            );
        }
        let bytes = fs::read(&canonical)
            .with_context(|| format!("read prior selection {}", path.display()))?;
        if sha256(&bytes) != link.sha256 {
            bail!("prior selection sha256 does not match {}", path.display());
        }
        let selection: PriorSelection = serde_json::from_slice(&bytes)
            .with_context(|| format!("deserialize prior selection {}", path.display()))?;
        repositories.extend(selection.profiles.into_iter().flat_map(|profile| {
            profile
                .selected
                .into_iter()
                .map(|selected| selected.full_name.to_ascii_lowercase())
        }));
    }
    Ok(repositories)
}

fn artifact_link(path: &Path, bytes: &[u8]) -> Result<ArtifactLink> {
    let root = crate::find_repo_root_for_path(path).with_context(|| {
        format!(
            "artifact {} must be inside the UsageBench repository",
            path.display()
        )
    })?;
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("resolve repository root {}", root.display()))?;
    let file = path
        .canonicalize()
        .with_context(|| format!("resolve artifact {}", path.display()))?
        .strip_prefix(&canonical_root)
        .with_context(|| format!("artifact {} is outside repository", path.display()))?
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

fn ranking_digest(
    freeze_id: &str,
    target: &TargetProfile,
    protocol_commit: &str,
    full_name: &str,
) -> String {
    let mut hasher = Sha256::new();
    for part in [b"usagebench-".as_slice(), freeze_id.as_bytes(), b"\0"] {
        hasher.update(part);
    }
    for part in [
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

    fn initialize_fixture_root(directory: &Path) {
        fs::write(
            directory.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.0.0'\n",
        )
        .unwrap();
        fs::create_dir(directory.join("schema")).unwrap();
    }

    fn protocol() -> Protocol {
        Protocol {
            schema_version: 1,
            freeze_id: "real-project-v1".to_string(),
            target_profiles: vec![TargetProfile {
                language: "go".to_string(),
                candidate_id: "gopls".to_string(),
                github_language: None,
                root_build_markers: Vec::new(),
                source_extensions: Vec::new(),
                excluded_file_suffixes: Vec::new(),
            }],
            population: PopulationRules {
                minimum_stars: 100,
                minimum_source_files: LEGACY_MINIMUM_SOURCE_FILES,
                maximum_repository_size_bytes: LEGACY_MAX_ARCHIVE_BYTES,
                excluded_path_components: Vec::new(),
                prior_selections: Vec::new(),
            },
            sampling: Sampling {
                repositories_per_profile: 2,
                declarations_per_repository: 3,
                minimum_replacement_repositories: 0,
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
            inspect_source(
                &protocol().target_profiles[0],
                &protocol().population,
                1,
                &tree,
                &repository,
            )
            .unwrap()
            .repository_size_bytes,
            42 * 1024
        );
    }

    #[test]
    fn draw_ranks_only_eligible_repositories_and_preserves_exclusions() {
        let directory = tempdir().unwrap();
        initialize_fixture_root(directory.path());
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
        let selection = draw_selection_with_options(
            DrawSelectionOptions {
                protocol: protocol_path,
                population: population_path,
                output,
                protocol_commit: "b".repeat(40),
            },
            false,
            false,
        )
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
        let protocol = protocol();
        let target = &protocol.target_profiles[0];
        assert!(is_source_file(
            target,
            &protocol.population,
            1,
            "cmd/main.go"
        ));
        assert!(!is_source_file(
            target,
            &protocol.population,
            1,
            "cmd/main_test.go"
        ));
        assert!(!is_source_file(
            target,
            &protocol.population,
            1,
            "vendor/pkg/index.go"
        ));
        assert!(!is_source_file(
            target,
            &protocol.population,
            1,
            "examples/demo.go"
        ));
    }

    #[test]
    fn rank_digest_uses_nul_delimiters() {
        let target = TargetProfile {
            language: "go".to_string(),
            candidate_id: "gopls".to_string(),
            github_language: None,
            root_build_markers: Vec::new(),
            source_extensions: Vec::new(),
            excluded_file_suffixes: Vec::new(),
        };
        assert_eq!(
            ranking_digest("real-project-v1", &target, &"b".repeat(40), "owner/repo"),
            "9cd334272668821f12055d7a7265038e5f81900fcc6378e8a54803df9f63e652"
        );
    }

    #[test]
    fn protocol_fixture_is_well_formed() {
        assert_eq!(protocol().sampling.repositories_per_profile, 2);
    }

    #[test]
    fn protocol_loader_enforces_the_checked_schema() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("protocol.json");
        fs::write(
            &path,
            br#"{"schemaVersion":2,"freezeId":"real-project-v2"}"#,
        )
        .unwrap();

        let error = load_protocol(&path).unwrap_err();
        assert!(error.to_string().contains("failed schema validation"));
    }

    #[test]
    fn protocol_commit_bytes_must_match() {
        let error =
            verify_protocol_commit_bytes(b"current", b"committed", &"b".repeat(40)).unwrap_err();
        assert!(error.to_string().contains("protocol bytes do not match"));
    }

    #[test]
    fn artifact_links_reject_absolute_paths_outside_the_repository() {
        let error =
            artifact_link(Path::new("/outside-usagebench/protocol.json"), b"{}").unwrap_err();
        assert!(error
            .to_string()
            .contains("must be inside the UsageBench repository"));
    }

    #[test]
    fn v1_replacement_rule_text_is_stable() {
        assert_eq!(
            replacement_rule(&protocol()),
            "No selected repository has been replaced; the next eligible ranked repository is reserved for a future source-only replacement decision."
        );
    }

    #[test]
    fn checked_in_v2_repository_selection_is_reproducible() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let directory = root.join("benchmarks/evaluation/real-project-v2");
        let (expected, expected_bytes): (SelectionManifest, Vec<u8>) =
            crate::evaluation::load_evaluation_selection_checked(&directory.join("selection.json"))
                .unwrap();
        let output_directory = tempdir().unwrap();
        let output = output_directory.path().join("selection.json");
        let actual = draw_selection_with_options(
            DrawSelectionOptions {
                protocol: directory.join("protocol.json"),
                population: directory.join("population.json"),
                output: output.clone(),
                protocol_commit: expected.protocol_commit.clone(),
            },
            false,
            true,
        )
        .unwrap();

        assert_eq!(fs::read(output).unwrap(), expected_bytes);
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    #[test]
    fn v2_draw_excludes_prior_slice_and_uses_v2_identity() {
        let directory = tempdir().unwrap();
        initialize_fixture_root(directory.path());
        let prior_path = directory.path().join("prior-selection.json");
        let prior_bytes = serde_json::to_vec(&serde_json::json!({
            "profiles": [{"selected": [{"fullName": "owner/prior"}]}]
        }))
        .unwrap();
        fs::write(&prior_path, &prior_bytes).unwrap();
        let protocol_path = directory.path().join("protocol.json");
        let protocol_bytes = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "freezeId": "real-project-v2",
            "targetProfiles": [{
                "language": "go",
                "candidateId": "gopls",
                "githubLanguage": "Go",
                "rootBuildMarkers": ["go.mod"],
                "sourceExtensions": [".go"],
                "excludedFileSuffixes": ["_test.go"]
            }],
            "population": {
                "minimumStars": 100,
                "minimumSourceFiles": 20,
                "maximumRepositorySizeBytes": 157286400,
                "excludedPathComponents": ["vendor"],
                "priorSelections": [{
                    "file": "prior-selection.json",
                    "sha256": sha256(&prior_bytes)
                }]
            },
            "sampling": {
                "repositoriesPerProfile": 1,
                "declarationsPerRepository": 3,
                "minimumReplacementRepositories": 1
            }
        }))
        .unwrap();
        fs::write(&protocol_path, &protocol_bytes).unwrap();
        let population_path = directory.path().join("population.json");
        let snapshot = PopulationSnapshot {
            schema_version: 1,
            freeze_id: "real-project-v2".to_string(),
            protocol: artifact_link(&protocol_path, &protocol_bytes).unwrap(),
            captured_at: "2026-08-01T00:00:00Z".to_string(),
            profiles: vec![PopulationProfile {
                language: "go".to_string(),
                candidate_id: "gopls".to_string(),
                requests: vec![],
                repositories: vec![
                    repository("owner/eligible-a", 100),
                    repository("owner/prior", 100),
                    repository("owner/eligible-b", 100),
                ],
            }],
        };
        write_json(&population_path, &snapshot).unwrap();

        let selection = draw_selection_with_options(
            DrawSelectionOptions {
                protocol: protocol_path,
                population: population_path,
                output: directory.path().join("selection.json"),
                protocol_commit: "b".repeat(40),
            },
            false,
            false,
        )
        .unwrap();

        assert_eq!(selection.profiles[0].selected.len(), 1);
        assert!(selection.profiles[0].selected[0]
            .case_file
            .contains("real-project-v2"));
        let prior = selection.profiles[0]
            .ranked
            .iter()
            .find(|candidate| candidate.full_name == "owner/prior")
            .unwrap();
        assert!(!prior.eligibility.eligible);
        assert!(prior
            .eligibility
            .reasons
            .contains(&"repository was selected by a prior evaluation slice".to_string()));
    }

    #[test]
    fn v2_draw_fails_without_replacement_headroom() {
        let mut protocol = protocol();
        protocol.schema_version = 2;
        protocol.freeze_id = "real-project-v2".to_string();
        protocol.target_profiles[0].github_language = Some("Go".to_string());
        protocol.target_profiles[0].root_build_markers = vec!["go.mod".to_string()];
        protocol.target_profiles[0].source_extensions = vec![".go".to_string()];
        protocol.population.excluded_path_components = vec!["vendor".to_string()];
        protocol.sampling.repositories_per_profile = 1;
        protocol.sampling.minimum_replacement_repositories = 1;
        let captured = PopulationProfile {
            language: "go".to_string(),
            candidate_id: "gopls".to_string(),
            requests: vec![],
            repositories: vec![repository("owner/only", 100)],
        };

        let error = select_profile(
            &protocol,
            &protocol.target_profiles[0],
            &captured,
            &"b".repeat(40),
            &BTreeSet::new(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("plus 1 replacements"));
    }

    #[test]
    fn checkpoint_round_trips_only_when_bound_to_the_same_protocol() {
        let directory = tempdir().unwrap();
        initialize_fixture_root(directory.path());
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
