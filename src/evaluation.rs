//! Evidence contracts for prospectively selected real-project evaluation slices.
//!
//! Evaluation case YAML remains the runner input. These manifests bind that
//! YAML to the protocol, independent review record, and materialized source
//! identities that were fixed before analyzer outcomes are inspected.

use crate::{
    is_exact_git_commit, BenchmarkDocument, CorpusPartition, GroundTruthReviewStatus, Source,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::{de::DeserializeOwned, Deserialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};
use url::Url;

const EVALUATION_PROTOCOL_SCHEMA: &str = include_str!("../schema/evaluation-protocol.schema.json");
const EVALUATION_SELECTION_SCHEMA: &str =
    include_str!("../schema/evaluation-selection.schema.json");
const EVALUATION_REVIEW_SCHEMA: &str = include_str!("../schema/evaluation-review.schema.json");
const SOURCE_MATERIALIZATION_SCHEMA: &str =
    include_str!("../schema/source-materialization.schema.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationProtocol {
    schema_version: u32,
    freeze_id: String,
    target_profiles: Vec<TargetProfile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetProfile {
    language: String,
    candidate_id: String,
    profile: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactLink {
    file: String,
    sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationSelection {
    schema_version: u32,
    freeze_id: String,
    protocol: ArtifactLink,
    documents: Vec<SelectedDocument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectedDocument {
    case_file: String,
    language: String,
    candidate_id: String,
    source: GitSource,
    case_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct GitSource {
    repo: Url,
    commit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluationReview {
    schema_version: u32,
    freeze_id: String,
    selection: ArtifactLink,
    reviewers: Vec<ReviewArtifact>,
    adjudication: ReviewArtifact,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewArtifact {
    id: String,
    file: String,
    sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceMaterialization {
    schema_version: u32,
    freeze_id: String,
    selection: ArtifactLink,
    sources: Vec<MaterializedSource>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct MaterializedSource {
    repo: Url,
    commit: String,
    tree: String,
    archive: String,
    sha256: String,
}

/// Validate the evidence linked from one evaluation document.
///
/// This checks metadata, review records, and the archived source bytes. The
/// source archive is part of the frozen evaluation artifact, so runner
/// execution never needs to resolve a mutable Git ref or clone from the
/// network.
pub fn validate_document_evidence(
    document: &BenchmarkDocument,
    case_file: &Path,
    repo_root: &Path,
) -> Result<()> {
    if document.corpus.partition != CorpusPartition::Evaluation {
        return Ok(());
    }

    let freeze_id = required_metadata(&document.corpus.freeze_id, "freezeId")?;
    let selection_file =
        required_metadata(&document.corpus.selection_manifest, "selectionManifest")?;
    let selection_path = evidence_path(repo_root, selection_file)?;
    let review_path = evidence_path(
        repo_root,
        required_metadata(&document.corpus.review_manifest, "reviewManifest")?,
    )?;
    let source_lock_path = evidence_path(
        repo_root,
        required_metadata(&document.corpus.source_lock, "sourceLock")?,
    )?;

    let (selection, selection_bytes) = load_checked::<EvaluationSelection>(
        &selection_path,
        EVALUATION_SELECTION_SCHEMA,
        "evaluation selection manifest",
    )?;
    validate_schema_version(selection.schema_version, "evaluation selection manifest")?;
    require_same("selection freezeId", &selection.freeze_id, freeze_id)?;

    let protocol_path = evidence_path(repo_root, &selection.protocol.file)?;
    let (protocol, protocol_bytes) = load_checked::<EvaluationProtocol>(
        &protocol_path,
        EVALUATION_PROTOCOL_SCHEMA,
        "evaluation protocol",
    )?;
    validate_schema_version(protocol.schema_version, "evaluation protocol")?;
    require_same("protocol freezeId", &protocol.freeze_id, freeze_id)?;
    validate_link(
        &selection.protocol,
        &protocol_path,
        &protocol_bytes,
        "evaluation selection protocol",
    )?;
    validate_profiles(&protocol.target_profiles)?;

    let case_file = repo_relative(case_file, repo_root)?;
    let selected = selection
        .documents
        .iter()
        .filter(|selected| selected.case_file == case_file)
        .collect::<Vec<_>>();
    let [selected] = selected.as_slice() else {
        bail!("evaluation selection does not contain exactly one entry for {case_file}");
    };
    if selected.language != document.language {
        bail!(
            "evaluation selection language {} does not match document language {} for {case_file}",
            selected.language,
            document.language
        );
    }
    if !protocol.target_profiles.iter().any(|profile| {
        profile.language == selected.language && profile.candidate_id == selected.candidate_id
    }) {
        bail!(
            "evaluation selection candidate {} is not registered for language {} in the protocol",
            selected.candidate_id,
            selected.language
        );
    }
    validate_document_source(document, &selected.source)?;
    validate_case_ids(document, &selected.case_ids, &case_file)?;

    let (review, review_bytes) = load_checked::<EvaluationReview>(
        &review_path,
        EVALUATION_REVIEW_SCHEMA,
        "evaluation review manifest",
    )?;
    validate_schema_version(review.schema_version, "evaluation review manifest")?;
    require_same("review freezeId", &review.freeze_id, freeze_id)?;
    require_same(
        "evaluation review selection file",
        &review.selection.file,
        selection_file,
    )?;
    validate_link(
        &review.selection,
        &selection_path,
        &selection_bytes,
        "evaluation review selection",
    )?;
    validate_reviewers(document, &review, repo_root)?;

    let (source_lock, _) = load_checked::<SourceMaterialization>(
        &source_lock_path,
        SOURCE_MATERIALIZATION_SCHEMA,
        "evaluation source lock",
    )?;
    validate_schema_version(source_lock.schema_version, "evaluation source lock")?;
    require_same("source lock freezeId", &source_lock.freeze_id, freeze_id)?;
    require_same(
        "evaluation source lock selection file",
        &source_lock.selection.file,
        selection_file,
    )?;
    validate_link(
        &source_lock.selection,
        &selection_path,
        &selection_bytes,
        "evaluation source lock selection",
    )?;
    validate_source_lock(&source_lock.sources, &selected.source, repo_root)?;

    let _ = review_bytes;
    Ok(())
}

/// Validate a path containing only promoted evaluation documents.
pub fn validate_path(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let files = crate::validate_path(path)?;
    for file in &files {
        let yaml = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
        let document: BenchmarkDocument = serde_yaml::from_str(&yaml)
            .with_context(|| format!("deserialize benchmark document {}", file.display()))?;
        if document.corpus.partition != CorpusPartition::Evaluation {
            bail!("{} is not an evaluation document", file.display());
        }
    }
    Ok(files)
}

/// Extract the locked source archive for an evaluation document into a runner
/// workspace. Non-evaluation documents retain their existing source handling.
pub fn materialized_source_root(
    document: &BenchmarkDocument,
    repo_root: &Path,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    if document.corpus.partition != CorpusPartition::Evaluation {
        return Ok(None);
    }

    let source_lock_path = evidence_path(
        repo_root,
        required_metadata(&document.corpus.source_lock, "sourceLock")?,
    )?;
    let (source_lock, _) = load_checked::<SourceMaterialization>(
        &source_lock_path,
        SOURCE_MATERIALIZATION_SCHEMA,
        "evaluation source lock",
    )?;
    let Source::Git { repo, commit } = &document.source else {
        bail!("evaluation document must use a git source");
    };
    let matching = source_lock
        .sources
        .iter()
        .filter(|source| source.repo == *repo && source.commit == *commit)
        .collect::<Vec<_>>();
    let [source] = matching.as_slice() else {
        bail!("source lock does not contain exactly one entry for the evaluation document source");
    };
    let archive_path = evidence_path(repo_root, &source.archive)?;
    let destination = work_dir.join("sources").join(&source.sha256);
    if destination.is_dir() {
        return destination
            .canonicalize()
            .with_context(|| format!("canonicalize {}", destination.display()))
            .map(Some);
    }
    fs::create_dir_all(&destination)
        .with_context(|| format!("create materialized source root {}", destination.display()))?;
    let status = Command::new("tar")
        .arg("-xf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&destination)
        .status()
        .with_context(|| {
            format!(
                "extract materialized source archive {}",
                archive_path.display()
            )
        })?;
    if !status.success() {
        bail!(
            "extract materialized source archive {}",
            archive_path.display()
        );
    }
    destination
        .canonicalize()
        .with_context(|| format!("canonicalize {}", destination.display()))
        .map(Some)
}

fn load_checked<T: DeserializeOwned>(
    path: &Path,
    schema_source: &str,
    kind: &str,
) -> Result<(T, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read {kind} {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {kind} {}", path.display()))?;
    let schema: serde_json::Value = serde_json::from_str(schema_source)
        .with_context(|| format!("parse bundled {kind} schema"))?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow!("compile bundled {kind} schema: {error}"))?;
    if let Err(errors) = compiled.validate(&value) {
        let messages = errors
            .map(|error| format!("{}: {error}", error.instance_path))
            .collect::<Vec<_>>()
            .join("\n");
        bail!("{} failed schema validation:\n{messages}", path.display());
    }
    let parsed = serde_json::from_value(value)
        .with_context(|| format!("deserialize {kind} {}", path.display()))?;
    Ok((parsed, bytes))
}

fn required_metadata<'a>(value: &'a Option<String>, field: &str) -> Result<&'a str> {
    value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("evaluation corpus documents require a non-empty {field}"))
}

fn evidence_path(repo_root: &Path, value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("evaluation evidence path {value} must be a safe repository-relative path");
    }
    Ok(repo_root.join(path))
}

fn repo_relative(path: &Path, repo_root: &Path) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("read current directory")?
            .join(path)
    };
    let relative = absolute.strip_prefix(repo_root).with_context(|| {
        format!(
            "case file {} is not under repository root {}",
            absolute.display(),
            repo_root.display()
        )
    })?;
    relative
        .to_str()
        .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| anyhow!("case file {} is not valid UTF-8", absolute.display()))
}

fn validate_schema_version(version: u32, kind: &str) -> Result<()> {
    if version != 1 {
        bail!("{kind} schemaVersion must be 1");
    }
    Ok(())
}

fn require_same(field: &str, actual: &str, expected: &str) -> Result<()> {
    if actual != expected {
        bail!("{field} {actual} does not match {expected}");
    }
    Ok(())
}

fn validate_link(link: &ArtifactLink, path: &Path, bytes: &[u8], kind: &str) -> Result<()> {
    if !is_hex_digest(&link.sha256) {
        bail!("{kind} has an invalid sha256");
    }
    let actual = sha256(bytes);
    if link.sha256 != actual {
        bail!("{kind} sha256 does not match {}", path.display());
    }
    Ok(())
}

fn validate_profiles(profiles: &[TargetProfile]) -> Result<()> {
    let mut identities = BTreeSet::new();
    for profile in profiles {
        if profile.language.trim().is_empty()
            || profile.candidate_id.trim().is_empty()
            || profile.profile.trim().is_empty()
        {
            bail!("evaluation protocol target profiles must be non-empty");
        }
        if !identities.insert((profile.language.as_str(), profile.candidate_id.as_str())) {
            bail!("evaluation protocol contains a duplicate language/candidate profile");
        }
    }
    Ok(())
}

fn validate_document_source(document: &BenchmarkDocument, selected: &GitSource) -> Result<()> {
    if !is_exact_git_commit(&selected.commit) {
        bail!("evaluation selection source commit must be an exact 40-character lowercase hexadecimal ID");
    }
    match &document.source {
        Source::Git { repo, commit } if repo == &selected.repo && commit == &selected.commit => {
            Ok(())
        }
        Source::Git { .. } => bail!("evaluation document git source does not match the selection"),
        Source::Fixture { .. } => bail!("evaluation document must use a git source"),
    }
}

fn validate_case_ids(
    document: &BenchmarkDocument,
    selected: &[String],
    case_file: &str,
) -> Result<()> {
    let actual = document
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let expected = selected.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if actual.len() != document.cases.len() {
        bail!("evaluation document {case_file} contains duplicate case IDs");
    }
    if expected.len() != selected.len() {
        bail!("evaluation selection contains duplicate case IDs for {case_file}");
    }
    if actual != expected {
        bail!("evaluation document case IDs do not match the selection for {case_file}");
    }
    Ok(())
}

fn validate_reviewers(
    document: &BenchmarkDocument,
    review: &EvaluationReview,
    repo_root: &Path,
) -> Result<()> {
    if document.ground_truth.status != GroundTruthReviewStatus::IndependentlyReviewed {
        bail!("evaluation review evidence requires independently_reviewed ground truth");
    }
    let expected = document
        .ground_truth
        .reviewers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let actual = review
        .reviewers
        .iter()
        .map(|reviewer| reviewer.id.as_str())
        .collect::<BTreeSet<_>>();
    if actual.len() != review.reviewers.len() || actual != expected {
        bail!("evaluation review evidence reviewers do not match groundTruth reviewers");
    }
    for reviewer in &review.reviewers {
        validate_review_artifact(reviewer, repo_root, "reviewer evidence")?;
    }
    validate_review_artifact(&review.adjudication, repo_root, "adjudication evidence")
}

fn validate_review_artifact(artifact: &ReviewArtifact, repo_root: &Path, kind: &str) -> Result<()> {
    let path = evidence_path(repo_root, &artifact.file)?;
    let bytes = fs::read(&path).with_context(|| format!("read {kind} {}", path.display()))?;
    if !is_hex_digest(&artifact.sha256) || sha256(&bytes) != artifact.sha256 {
        bail!("{kind} sha256 does not match {}", path.display());
    }
    Ok(())
}

fn validate_source_lock(
    sources: &[MaterializedSource],
    selected: &GitSource,
    repo_root: &Path,
) -> Result<()> {
    let matching = sources
        .iter()
        .filter(|source| source.repo == selected.repo && source.commit == selected.commit)
        .collect::<Vec<_>>();
    let [source] = matching.as_slice() else {
        bail!("source lock does not contain exactly one entry for the selected git source");
    };
    if !is_exact_git_commit(&source.tree)
        || !is_hex_digest(&source.sha256)
        || Path::new(&source.archive).is_absolute()
        || Path::new(&source.archive).components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!("source lock contains an invalid materialized source entry");
    }
    let archive_path = evidence_path(repo_root, &source.archive)?;
    let archive = fs::read(&archive_path).with_context(|| {
        format!(
            "read materialized source archive {}",
            archive_path.display()
        )
    })?;
    if sha256(&archive) != source.sha256 {
        bail!(
            "materialized source archive sha256 does not match {}",
            archive_path.display()
        );
    }
    validate_archive_commit(&archive_path, &source.commit)?;
    Ok(())
}

fn validate_archive_commit(archive_path: &Path, expected_commit: &str) -> Result<()> {
    let archive = fs::File::open(archive_path).with_context(|| {
        format!(
            "open materialized source archive {}",
            archive_path.display()
        )
    })?;
    let output = Command::new("git")
        .arg("get-tar-commit-id")
        .stdin(Stdio::from(archive))
        .output()
        .with_context(|| {
            format!(
                "read Git commit from source archive {}",
                archive_path.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "materialized source archive {} is not a git archive",
            archive_path.display()
        );
    }
    let actual = String::from_utf8(output.stdout)
        .context("decode Git commit from materialized source archive")?;
    require_same(
        "materialized source archive commit",
        actual.trim(),
        expected_commit,
    )
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BenchmarkCase, CorpusMetadata, CorpusSelection, GroundTruthReview, PositionEncoding,
        ReferencePolicy,
    };
    use serde_json::json;
    use tempfile::tempdir;

    const TREE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn checked_in_real_project_protocol_is_valid() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = root.join("benchmarks/evaluation/real-project-v1/protocol.json");
        let (protocol, _) = load_checked::<EvaluationProtocol>(
            &path,
            EVALUATION_PROTOCOL_SCHEMA,
            "checked-in evaluation protocol",
        )
        .unwrap();

        validate_schema_version(protocol.schema_version, "checked-in evaluation protocol").unwrap();
        assert_eq!(protocol.freeze_id, "real-project-v1");
        validate_profiles(&protocol.target_profiles).unwrap();
        for profile in protocol.target_profiles {
            assert!(root.join(profile.profile).is_file());
        }
    }

    #[test]
    fn matching_evaluation_evidence_is_accepted() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let archive_input = root.join("archive-input");
        fs::create_dir_all(&archive_input).unwrap();
        git(&archive_input, &["init"]);
        git(
            &archive_input,
            &["config", "user.email", "usagebench@example.test"],
        );
        git(&archive_input, &["config", "user.name", "UsageBench test"]);
        git(&archive_input, &["config", "commit.gpgSign", "false"]);
        fs::write(archive_input.join("source.txt"), "archived source").unwrap();
        git(&archive_input, &["add", "source.txt"]);
        git(&archive_input, &["commit", "-m", "archive source"]);
        let commit = git_stdout(&archive_input, &["rev-parse", "HEAD"]);
        let tree = git_stdout(&archive_input, &["rev-parse", "HEAD^{tree}"]);
        let protocol_path = root.join("protocol.json");
        write_json(
            &protocol_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "targetProfiles": [
                    {"language": "go", "candidateId": "gopls", "profile": "adapters/lsp/gopls.json"},
                    {"language": "python", "candidateId": "pyright", "profile": "adapters/lsp/pyright.json"}
                ],
                "population": {"snapshot": "population.json", "eligibility": "documented", "exclusions": "documented"},
                "sampling": {"seedDerivation": "protocol commit", "repositoriesPerProfile": 4, "declarationsPerRepository": 3, "replacementRule": "documented"},
                "operations": ["references", "definition"],
                "claimScope": "the sampled repositories"
            }),
        );
        let selection_path = root.join("selection.json");
        write_json(
            &selection_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "protocol": artifact_link("protocol.json", &protocol_path),
                "documents": [{
                    "caseFile": "cases/example.yaml",
                    "language": "go",
                    "candidateId": "gopls",
                    "source": {"repo": "https://github.com/example/project.git", "commit": commit},
                    "caseIds": ["selected-call"]
                }]
            }),
        );
        let reviewer_a = root.join("review-a.json");
        let reviewer_b = root.join("review-b.json");
        let adjudication = root.join("adjudication.json");
        fs::write(&reviewer_a, "first independent derivation").unwrap();
        fs::write(&reviewer_b, "second independent derivation").unwrap();
        fs::write(&adjudication, "adjudicated").unwrap();
        let review_path = root.join("review.json");
        write_json(
            &review_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "selection": artifact_link("selection.json", &selection_path),
                "reviewers": [
                    review_artifact("alice", "review-a.json", &reviewer_a),
                    review_artifact("bob", "review-b.json", &reviewer_b)
                ],
                "adjudication": review_artifact("adjudication", "adjudication.json", &adjudication)
            }),
        );
        let source_lock_path = root.join("sources.json");
        let archive = root.join("sources/example-project.tar");
        fs::create_dir_all(archive.parent().unwrap()).unwrap();
        let status = Command::new("git")
            .arg("archive")
            .arg("--format=tar")
            .arg("--output")
            .arg(&archive)
            .arg("HEAD")
            .current_dir(&archive_input)
            .status()
            .unwrap();
        assert!(status.success());
        write_json(
            &source_lock_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "selection": artifact_link("selection.json", &selection_path),
                "sources": [{
                    "repo": "https://github.com/example/project.git",
                    "commit": commit,
                    "tree": tree,
                    "archive": "sources/example-project.tar",
                    "sha256": sha256(&fs::read(&archive).unwrap())
                }]
            }),
        );
        let case_file = root.join("cases/example.yaml");
        fs::create_dir_all(case_file.parent().unwrap()).unwrap();
        fs::write(&case_file, "placeholder").unwrap();
        let document = document(&commit);

        validate_document_evidence(&document, &case_file, root).unwrap();
        let extracted = materialized_source_root(&document, root, &root.join("work"))
            .unwrap()
            .unwrap();
        assert_eq!(
            fs::read_to_string(extracted.join("source.txt")).unwrap(),
            "archived source"
        );
    }

    #[test]
    fn selection_source_mismatch_is_rejected() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let protocol_path = root.join("protocol.json");
        write_json(
            &protocol_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "targetProfiles": [
                    {"language": "go", "candidateId": "gopls", "profile": "adapters/lsp/gopls.json"},
                    {"language": "python", "candidateId": "pyright", "profile": "adapters/lsp/pyright.json"}
                ],
                "population": {"snapshot": "population.json", "eligibility": "documented", "exclusions": "documented"},
                "sampling": {"seedDerivation": "protocol commit", "repositoriesPerProfile": 4, "declarationsPerRepository": 3, "replacementRule": "documented"},
                "operations": ["references"],
                "claimScope": "the sampled repositories"
            }),
        );
        let selection_path = root.join("selection.json");
        write_json(
            &selection_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "protocol": artifact_link("protocol.json", &protocol_path),
                "documents": [{
                    "caseFile": "cases/example.yaml",
                    "language": "go",
                    "candidateId": "gopls",
                    "source": {"repo": "https://github.com/example/project.git", "commit": "dddddddddddddddddddddddddddddddddddddddd"},
                    "caseIds": ["selected-call"]
                }]
            }),
        );
        let reviewer_a = root.join("review-a.json");
        let reviewer_b = root.join("review-b.json");
        let adjudication = root.join("adjudication.json");
        fs::write(&reviewer_a, "a").unwrap();
        fs::write(&reviewer_b, "b").unwrap();
        fs::write(&adjudication, "adjudicated").unwrap();
        let review_path = root.join("review.json");
        write_json(
            &review_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "selection": artifact_link("selection.json", &selection_path),
                "reviewers": [review_artifact("alice", "review-a.json", &reviewer_a), review_artifact("bob", "review-b.json", &reviewer_b)],
                "adjudication": review_artifact("adjudication", "adjudication.json", &adjudication)
            }),
        );
        let source_lock_path = root.join("sources.json");
        write_json(
            &source_lock_path,
            json!({
                "schemaVersion": 1,
                "freezeId": "real-project-v1",
                "selection": artifact_link("selection.json", &selection_path),
                "sources": [{
                    "repo": "https://github.com/example/project.git",
                    "commit": "dddddddddddddddddddddddddddddddddddddddd",
                    "tree": TREE,
                    "archive": "sources/example-project.tar",
                    "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                }]
            }),
        );
        let case_file = root.join("cases/example.yaml");
        fs::create_dir_all(case_file.parent().unwrap()).unwrap();
        fs::write(&case_file, "placeholder").unwrap();

        let error = validate_document_evidence(
            &document("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            &case_file,
            root,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("does not match the selection"));
    }

    fn document(commit: &str) -> BenchmarkDocument {
        BenchmarkDocument {
            schema_version: 2,
            position_encoding: PositionEncoding::Utf16,
            source: Source::Git {
                repo: Url::parse("https://github.com/example/project.git").unwrap(),
                commit: commit.to_string(),
            },
            language: "go".to_string(),
            corpus: CorpusMetadata {
                partition: CorpusPartition::Evaluation,
                selection: CorpusSelection::PreRegistered,
                freeze_id: Some("real-project-v1".to_string()),
                selection_manifest: Some("selection.json".to_string()),
                review_manifest: Some("review.json".to_string()),
                source_lock: Some("sources.json".to_string()),
            },
            ground_truth: GroundTruthReview {
                status: GroundTruthReviewStatus::IndependentlyReviewed,
                reviewers: vec!["alice".to_string(), "bob".to_string()],
            },
            reference_policy: ReferencePolicy::BindingsOptional,
            cases: vec![BenchmarkCase {
                id: "selected-call".to_string(),
                declaration: None,
                reference_probe: None,
                expected_usages: Vec::new(),
                expected_unproven_usages: Vec::new(),
                allowed_extra_usages: Vec::new(),
                allowed_unproven_usages: Vec::new(),
                usage_lookups: Vec::new(),
                type_lookups: Vec::new(),
                expected_failure: None,
                not_planned: None,
                unsupported: None,
                verification: None,
            }],
        }
    }

    fn artifact_link(file: &str, path: &Path) -> serde_json::Value {
        json!({"file": file, "sha256": sha256(&fs::read(path).unwrap())})
    }

    fn review_artifact(id: &str, file: &str, path: &Path) -> serde_json::Value {
        json!({"id": id, "file": file, "sha256": sha256(&fs::read(path).unwrap())})
    }

    fn write_json(path: &Path, value: serde_json::Value) {
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }

    fn git(path: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn git_stdout(path: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }
}
