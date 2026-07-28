use crate::runners::{
    report_compare::{compare_report_files_with_scope, ComparisonScope, ReportDifference},
    ExecutionMode, PlatformScope, RunReport,
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use url::Url;

pub const REPRODUCTION_EVIDENCE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproductionClass {
    Canonical,
    NativeTwoHost,
}

impl std::fmt::Display for ReproductionClass {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Canonical => "canonical",
            Self::NativeTwoHost => "native two-host",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceFile {
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceHost {
    pub id: String,
    pub runner_name: String,
    pub provider: String,
    pub operating_system: String,
    pub architecture: String,
    pub provenance: Url,
    pub requested_version: String,
    pub profile_sha256: String,
    pub executable_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonStatus {
    Equivalent,
    Different,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeComparison {
    pub scope: ComparisonScopeName,
    pub status: ComparisonStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<EvidenceFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonScopeName {
    NativeResults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "class", rename_all = "snake_case")]
pub enum ReproductionProof {
    Canonical {
        #[serde(rename = "referenceRunner")]
        reference_runner: String,
        #[serde(rename = "environmentVersion")]
        environment_version: String,
        #[serde(rename = "definitionDigest")]
        definition_digest: String,
    },
    NativeTwoHost {
        #[serde(rename = "primaryHost")]
        primary_host: EvidenceHost,
        #[serde(rename = "corroboratingHost")]
        corroborating_host: EvidenceHost,
        #[serde(rename = "corroboratingReport")]
        corroborating_report: EvidenceFile,
        comparison: NativeComparison,
    },
}

impl ReproductionProof {
    pub fn class(&self) -> ReproductionClass {
        match self {
            Self::Canonical { .. } => ReproductionClass::Canonical,
            Self::NativeTwoHost { .. } => ReproductionClass::NativeTwoHost,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReproductionEvidence {
    pub schema_version: u32,
    pub candidate_id: String,
    pub primary_report: EvidenceFile,
    #[serde(flatten)]
    pub proof: ReproductionProof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateEvidenceLink {
    pub candidate_id: String,
    pub class: ReproductionClass,
    pub file: String,
    pub sha256: String,
}

#[derive(Debug)]
pub struct ValidatedEvidence {
    pub evidence: ReproductionEvidence,
    pub link: CandidateEvidenceLink,
    pub accepted: bool,
}

#[derive(Debug, Clone)]
pub struct CreateNativeEvidenceOptions {
    pub candidate_id: String,
    pub primary_report: PathBuf,
    pub primary_host_id: String,
    pub primary_runner_name: String,
    pub primary_host_provider: String,
    pub primary_host_provenance: String,
    pub primary_requested_version: String,
    pub primary_profile_sha256: String,
    pub corroborating_report: PathBuf,
    pub corroborating_host_id: String,
    pub corroborating_runner_name: String,
    pub corroborating_host_provider: String,
    pub corroborating_host_provenance: String,
    pub corroborating_requested_version: String,
    pub corroborating_profile_sha256: String,
    pub output: PathBuf,
    pub diff_output: PathBuf,
}

pub fn create_native_evidence(
    options: CreateNativeEvidenceOptions,
) -> Result<ReproductionEvidence> {
    let output_directory = options
        .output
        .parent()
        .context("native evidence output has no parent directory")?;
    for report in [&options.primary_report, &options.corroborating_report] {
        if report.parent() != Some(output_directory) {
            bail!("native evidence reports and manifest must share one directory");
        }
    }
    if options.diff_output.parent() != Some(output_directory) {
        bail!("native evidence diff and manifest must share one directory");
    }

    let primary_bytes = fs::read(&options.primary_report)
        .with_context(|| format!("read primary report {}", options.primary_report.display()))?;
    let corroborating_bytes = fs::read(&options.corroborating_report).with_context(|| {
        format!(
            "read corroborating report {}",
            options.corroborating_report.display()
        )
    })?;
    let primary: RunReport = serde_json::from_slice(&primary_bytes)
        .with_context(|| format!("parse primary report {}", options.primary_report.display()))?;
    let corroborating: RunReport =
        serde_json::from_slice(&corroborating_bytes).with_context(|| {
            format!(
                "parse corroborating report {}",
                options.corroborating_report.display()
            )
        })?;
    validate_same_run_identity(&primary, &corroborating)?;

    let primary_host = EvidenceHost {
        id: options.primary_host_id,
        runner_name: options.primary_runner_name,
        provider: options.primary_host_provider,
        operating_system: primary.environment.operating_system.clone(),
        architecture: primary.environment.architecture.clone(),
        provenance: Url::parse(&options.primary_host_provenance)
            .context("parse primary host provenance URL")?,
        requested_version: options.primary_requested_version,
        profile_sha256: options.primary_profile_sha256,
        executable_sha256: primary
            .environment
            .analyzer_executable
            .sha256
            .clone()
            .context("primary native report lacks an executable checksum")?,
    };
    let corroborating_host = EvidenceHost {
        id: options.corroborating_host_id,
        runner_name: options.corroborating_runner_name,
        provider: options.corroborating_host_provider,
        operating_system: corroborating.environment.operating_system.clone(),
        architecture: corroborating.environment.architecture.clone(),
        provenance: Url::parse(&options.corroborating_host_provenance)
            .context("parse corroborating host provenance URL")?,
        requested_version: options.corroborating_requested_version,
        profile_sha256: options.corroborating_profile_sha256,
        executable_sha256: corroborating
            .environment
            .analyzer_executable
            .sha256
            .clone()
            .context("corroborating native report lacks an executable checksum")?,
    };
    validate_native_host(&primary_host, &primary)?;
    validate_native_host(&corroborating_host, &corroborating)?;
    if primary_host.id == corroborating_host.id
        || primary_host.runner_name == corroborating_host.runner_name
        || primary_host.provenance == corroborating_host.provenance
    {
        bail!("native reproduction requires two distinct host identities");
    }
    validate_matching_native_platform(&primary_host, &corroborating_host)?;

    let differences = compare_report_files_with_scope(
        &options.primary_report,
        &options.corroborating_report,
        ComparisonScope::NativeResults,
    )?;
    let comparison = if differences.is_empty() {
        NativeComparison {
            scope: ComparisonScopeName::NativeResults,
            status: ComparisonStatus::Equivalent,
            diff: None,
        }
    } else {
        let diff_bytes = serde_json::to_vec_pretty(&differences)?;
        fs::write(&options.diff_output, &diff_bytes).with_context(|| {
            format!(
                "write native semantic diff {}",
                options.diff_output.display()
            )
        })?;
        NativeComparison {
            scope: ComparisonScopeName::NativeResults,
            status: ComparisonStatus::Different,
            diff: Some(EvidenceFile {
                file: simple_file_name(&options.diff_output)?,
                sha256: hex_digest(&diff_bytes),
            }),
        }
    };
    let evidence = ReproductionEvidence {
        schema_version: REPRODUCTION_EVIDENCE_SCHEMA_VERSION,
        candidate_id: options.candidate_id,
        primary_report: EvidenceFile {
            file: simple_file_name(&options.primary_report)?,
            sha256: hex_digest(&primary_bytes),
        },
        proof: ReproductionProof::NativeTwoHost {
            primary_host,
            corroborating_host,
            corroborating_report: EvidenceFile {
                file: simple_file_name(&options.corroborating_report)?,
                sha256: hex_digest(&corroborating_bytes),
            },
            comparison,
        },
    };
    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(&options.output, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("write native evidence {}", options.output.display()))?;
    Ok(evidence)
}

pub fn validate_evidence(
    evidence_path: &Path,
    expected_candidate_id: &str,
    expected_class: ReproductionClass,
    expected_reference_runner: Option<&str>,
    expected_requested_version: &str,
    expected_profile_sha256: Option<&str>,
    primary_report_path: &Path,
    primary_report: &RunReport,
) -> Result<ValidatedEvidence> {
    let evidence_bytes = fs::read(evidence_path)
        .with_context(|| format!("read reproduction evidence {}", evidence_path.display()))?;
    let evidence = parse_evidence(&evidence_bytes, evidence_path)?;
    if evidence.schema_version != REPRODUCTION_EVIDENCE_SCHEMA_VERSION {
        bail!(
            "unsupported reproduction evidence schema version {}",
            evidence.schema_version
        );
    }
    if evidence.candidate_id != expected_candidate_id {
        bail!("reproduction evidence candidate does not match {expected_candidate_id}");
    }
    if evidence.proof.class() != expected_class {
        bail!("reproduction evidence class does not match candidate {expected_candidate_id}");
    }
    validate_file_reference(&evidence.primary_report)?;
    let primary_name = simple_file_name(primary_report_path)?;
    if evidence.primary_report.file != primary_name {
        bail!("reproduction evidence primary report does not match candidate report");
    }
    let primary_bytes = fs::read(primary_report_path)
        .with_context(|| format!("read primary report {}", primary_report_path.display()))?;
    require_digest(&evidence.primary_report, &primary_bytes)?;

    let evidence_directory = evidence_path
        .parent()
        .context("reproduction evidence path has no parent directory")?;
    let accepted = match &evidence.proof {
        ReproductionProof::Canonical {
            reference_runner,
            environment_version,
            definition_digest,
        } => {
            if expected_reference_runner != Some(reference_runner.as_str()) {
                bail!("canonical evidence reference runner does not match candidate");
            }
            if primary_report.environment.execution_mode != ExecutionMode::Container
                || primary_report.environment.platform_scope != PlatformScope::CanonicalReference
            {
                bail!("canonical evidence requires a canonical container report");
            }
            let reference = primary_report
                .environment
                .reference_environment
                .as_ref()
                .context("canonical report lacks reference environment provenance")?;
            if reference.version != *environment_version
                || reference.definition_digest != *definition_digest
            {
                bail!("canonical evidence does not match report environment provenance");
            }
            if primary_report.environment.operating_system != "linux"
                || primary_report.environment.architecture != "x86_64"
                || reference.canonical_platform != "linux/amd64"
                || !prefixed_sha256(&reference.definition_digest)
            {
                bail!("canonical evidence requires the pinned linux/amd64 reference platform");
            }
            let container = primary_report
                .environment
                .container
                .as_ref()
                .context("canonical report lacks container provenance")?;
            if container.image_reference.is_empty() || !prefixed_sha256(&container.image_digest) {
                bail!("canonical report lacks pinned container image provenance");
            }
            if primary_report
                .environment
                .analyzer_executable
                .command
                .is_empty()
                || !primary_report
                    .environment
                    .analyzer_executable
                    .sha256
                    .as_deref()
                    .is_some_and(raw_sha256)
            {
                bail!("canonical report lacks an analyzer executable checksum");
            }
            if primary_report.environment.toolchains.is_empty()
                || primary_report
                    .environment
                    .toolchains
                    .iter()
                    .any(|(name, version)| name.is_empty() || version.is_empty())
            {
                bail!("canonical report lacks pinned toolchain provenance");
            }
            true
        }
        ReproductionProof::NativeTwoHost {
            primary_host,
            corroborating_host,
            corroborating_report,
            comparison,
        } => {
            validate_native_host(primary_host, primary_report)?;
            if primary_host.id == corroborating_host.id
                || primary_host.runner_name == corroborating_host.runner_name
                || primary_host.provenance == corroborating_host.provenance
            {
                bail!("native reproduction requires two distinct host identities");
            }
            let expected_profile_sha256 = expected_profile_sha256
                .context("native candidate lacks a pinned profile checksum")?;
            for host in [primary_host, corroborating_host] {
                if host.requested_version != expected_requested_version
                    || host.profile_sha256 != expected_profile_sha256
                {
                    bail!("native host tool attestation does not match candidate release");
                }
            }
            validate_file_reference(corroborating_report)?;
            let corroborating_path = evidence_directory.join(&corroborating_report.file);
            let corroborating_bytes = fs::read(&corroborating_path).with_context(|| {
                format!("read corroborating report {}", corroborating_path.display())
            })?;
            require_digest(corroborating_report, &corroborating_bytes)?;
            let corroborating: RunReport = serde_json::from_slice(&corroborating_bytes)
                .with_context(|| {
                    format!(
                        "parse corroborating report {}",
                        corroborating_path.display()
                    )
                })?;
            validate_native_host(corroborating_host, &corroborating)?;
            validate_matching_native_platform(primary_host, corroborating_host)?;
            validate_same_run_identity(primary_report, &corroborating)?;
            let differences = compare_report_files_with_scope(
                primary_report_path,
                &corroborating_path,
                ComparisonScope::NativeResults,
            )?;
            validate_comparison(evidence_directory, comparison, &differences)?;
            differences.is_empty()
        }
    };

    Ok(ValidatedEvidence {
        link: CandidateEvidenceLink {
            candidate_id: expected_candidate_id.to_string(),
            class: expected_class,
            file: simple_file_name(evidence_path)?,
            sha256: hex_digest(&evidence_bytes),
        },
        evidence,
        accepted,
    })
}

fn validate_matching_native_platform(
    primary: &EvidenceHost,
    corroborating: &EvidenceHost,
) -> Result<()> {
    if primary.operating_system != corroborating.operating_system
        || primary.architecture != corroborating.architecture
        || primary.executable_sha256 != corroborating.executable_sha256
    {
        bail!(
            "native reproduction requires the same platform and analyzer executable on both hosts"
        );
    }
    Ok(())
}

fn validate_native_host(host: &EvidenceHost, report: &RunReport) -> Result<()> {
    if host.id.trim().is_empty()
        || host.runner_name.trim().is_empty()
        || host.provider.trim().is_empty()
        || !matches!(host.provenance.scheme(), "https" | "http")
        || host.provenance.host_str().is_none()
    {
        bail!("native evidence host requires id, provider, and provenance");
    }
    if report.environment.execution_mode != ExecutionMode::Native
        || report.environment.platform_scope != PlatformScope::HostSpecific
    {
        bail!("native two-host evidence requires native host-specific reports");
    }
    if host.operating_system != report.environment.operating_system
        || host.architecture != report.environment.architecture
    {
        bail!("native evidence host does not match report environment");
    }
    if !raw_sha256(&host.profile_sha256)
        || !raw_sha256(&host.executable_sha256)
        || report.invocation.profile_sha256.as_deref() != Some(host.profile_sha256.as_str())
        || report.environment.analyzer_executable.sha256.as_deref()
            != Some(host.executable_sha256.as_str())
    {
        bail!("native evidence tool attestation does not match report executable");
    }
    if report.totals.errors > 0
        || report.totals.documents == 0
        || report.totals.cases == 0
        || report.case_files.is_empty()
        || report.documents.is_empty()
    {
        bail!("native reproduction requires a nonempty run without runner errors");
    }
    Ok(())
}

fn validate_same_run_identity(primary: &RunReport, corroborating: &RunReport) -> Result<()> {
    if primary.usagebench_revision != corroborating.usagebench_revision
        || primary.usagebench_release != corroborating.usagebench_release
        || primary.runner.name != corroborating.runner.name
        || primary.runner.requested_version != corroborating.runner.requested_version
        || primary.runner.source != corroborating.runner.source
        || primary.runner.adapter_version != corroborating.runner.adapter_version
        || primary.invocation != corroborating.invocation
        || primary.case_files != corroborating.case_files
    {
        bail!("native evidence reports do not share the same run identity");
    }
    Ok(())
}

fn validate_comparison(
    directory: &Path,
    comparison: &NativeComparison,
    differences: &[ReportDifference],
) -> Result<()> {
    match (comparison.status, differences.is_empty(), &comparison.diff) {
        (ComparisonStatus::Equivalent, true, None) => Ok(()),
        (ComparisonStatus::Different, false, Some(diff)) => {
            validate_file_reference(diff)?;
            let path = directory.join(&diff.file);
            let bytes = fs::read(&path)
                .with_context(|| format!("read native semantic diff {}", path.display()))?;
            require_digest(diff, &bytes)?;
            let recorded: Vec<ReportDifference> = serde_json::from_slice(&bytes)
                .with_context(|| format!("parse native semantic diff {}", path.display()))?;
            if recorded != differences {
                bail!("native semantic diff does not match compared reports");
            }
            Ok(())
        }
        _ => bail!("native comparison status or diff does not match compared reports"),
    }
}

fn validate_file_reference(file: &EvidenceFile) -> Result<()> {
    if !raw_sha256(&file.sha256) {
        bail!("evidence file {} has an invalid SHA-256", file.file);
    }
    let _ = simple_path(&file.file)?;
    Ok(())
}

pub fn parse_evidence(bytes: &[u8], source: &Path) -> Result<ReproductionEvidence> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .with_context(|| format!("parse reproduction evidence {}", source.display()))?;
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/reproduction-evidence.schema.json"))
            .context("parse embedded reproduction evidence schema")?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow::anyhow!("compile reproduction evidence schema: {error}"))?;
    if let Err(errors) = compiled.validate(&value) {
        let messages = errors
            .take(8)
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        bail!(
            "reproduction evidence {} violates its schema: {}",
            source.display(),
            messages
        );
    }
    serde_json::from_value(value)
        .with_context(|| format!("decode reproduction evidence {}", source.display()))
}

fn raw_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn prefixed_sha256(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(raw_sha256)
}

fn require_digest(file: &EvidenceFile, bytes: &[u8]) -> Result<()> {
    let actual = hex_digest(bytes);
    if actual != file.sha256 {
        bail!(
            "checksum mismatch for {}: evidence {}, actual {}",
            file.file,
            file.sha256,
            actual
        );
    }
    Ok(())
}

pub fn simple_path(file: &str) -> Result<PathBuf> {
    let path = Path::new(file);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        bail!("evidence file must be a simple file name: {file}");
    }
    Ok(path.to_path_buf())
}

fn simple_file_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .with_context(|| format!("path {} has no file name", path.display()))
}

pub fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PROFILE_SHA: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    #[test]
    fn accepts_equivalent_reports_from_two_native_hosts() {
        let tempdir = tempfile::tempdir().unwrap();
        let primary_path = tempdir.path().join("pyright-primary.json");
        let corroborating_path = tempdir.path().join("pyright-corroborating.json");
        let primary = native_report("macos", "aarch64");
        let mut corroborating = native_report("macos", "aarch64");
        corroborating.finished_at_unix_seconds = 3;
        let primary_bytes = serde_json::to_vec_pretty(&primary).unwrap();
        let corroborating_bytes = serde_json::to_vec_pretty(&corroborating).unwrap();
        fs::write(&primary_path, &primary_bytes).unwrap();
        fs::write(&corroborating_path, &corroborating_bytes).unwrap();
        let evidence_path = tempdir.path().join("pyright-evidence.json");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 1,
                "candidateId": "pyright",
                "primaryReport": {"file": "pyright-primary.json", "sha256": hex_digest(&primary_bytes)},
                "class": "native_two_host",
                "primaryHost": host("mac-a", "macos", "aarch64"),
                "corroboratingHost": host("mac-b", "macos", "aarch64"),
                "corroboratingReport": {"file": "pyright-corroborating.json", "sha256": hex_digest(&corroborating_bytes)},
                "comparison": {"scope": "native_results", "status": "equivalent"}
            }))
            .unwrap(),
        )
        .unwrap();

        let schema: serde_json::Value = serde_json::from_slice(
            &fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("schema/reproduction-evidence.schema.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let evidence_value: serde_json::Value =
            serde_json::from_slice(&fs::read(&evidence_path).unwrap()).unwrap();
        let compiled = jsonschema::JSONSchema::compile(&schema).unwrap();
        assert!(compiled.validate(&evidence_value).is_ok());

        let validated = validate_evidence(
            &evidence_path,
            "pyright",
            ReproductionClass::NativeTwoHost,
            None,
            "1.1.411",
            Some(PROFILE_SHA),
            &primary_path,
            &primary,
        )
        .unwrap();

        assert!(validated.accepted);
    }

    #[test]
    fn rejects_native_evidence_that_reuses_a_host_identity() {
        let tempdir = tempfile::tempdir().unwrap();
        let primary_path = tempdir.path().join("pyright-primary.json");
        let corroborating_path = tempdir.path().join("pyright-corroborating.json");
        let report = native_report("macos", "aarch64");
        let bytes = serde_json::to_vec_pretty(&report).unwrap();
        fs::write(&primary_path, &bytes).unwrap();
        fs::write(&corroborating_path, &bytes).unwrap();
        let evidence_path = tempdir.path().join("pyright-evidence.json");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 1,
                "candidateId": "pyright",
                "primaryReport": {"file": "pyright-primary.json", "sha256": hex_digest(&bytes)},
                "class": "native_two_host",
                "primaryHost": host("same", "macos", "aarch64"),
                "corroboratingHost": host("same", "macos", "aarch64"),
                "corroboratingReport": {"file": "pyright-corroborating.json", "sha256": hex_digest(&bytes)},
                "comparison": {"scope": "native_results", "status": "equivalent"}
            }))
            .unwrap(),
        )
        .unwrap();

        let error = validate_evidence(
            &evidence_path,
            "pyright",
            ReproductionClass::NativeTwoHost,
            None,
            "1.1.411",
            Some(PROFILE_SHA),
            &primary_path,
            &report,
        )
        .unwrap_err();
        assert!(error.to_string().contains("two distinct host identities"));
    }

    #[test]
    fn accepts_byte_identical_reports_with_trusted_distinct_hosts() {
        let tempdir = tempfile::tempdir().unwrap();
        let primary_path = tempdir.path().join("pyright-primary.json");
        let corroborating_path = tempdir.path().join("pyright-corroborating.json");
        let report = native_report("macos", "aarch64");
        let bytes = serde_json::to_vec_pretty(&report).unwrap();
        fs::write(&primary_path, &bytes).unwrap();
        fs::write(&corroborating_path, &bytes).unwrap();
        let evidence_path = tempdir.path().join("pyright-evidence.json");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 1,
                "candidateId": "pyright",
                "primaryReport": {"file": "pyright-primary.json", "sha256": hex_digest(&bytes)},
                "class": "native_two_host",
                "primaryHost": host("mac-a", "macos", "aarch64"),
                "corroboratingHost": host("mac-b", "macos", "aarch64"),
                "corroboratingReport": {"file": "pyright-corroborating.json", "sha256": hex_digest(&bytes)},
                "comparison": {"scope": "native_results", "status": "equivalent"}
            }))
            .unwrap(),
        )
        .unwrap();

        let validated = validate_evidence(
            &evidence_path,
            "pyright",
            ReproductionClass::NativeTwoHost,
            None,
            "1.1.411",
            Some(PROFILE_SHA),
            &primary_path,
            &report,
        )
        .unwrap();
        assert!(validated.accepted);
    }

    #[test]
    fn rejects_schema_unknown_fields_at_runtime() {
        let bytes = serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "candidateId": "bifrost",
            "primaryReport": {"file": "bifrost.json", "sha256": "a".repeat(64)},
            "class": "canonical",
            "referenceRunner": "bifrost",
            "environmentVersion": "1",
            "definitionDigest": format!("sha256:{}", "b".repeat(64)),
            "unverifiedAttestation": true
        }))
        .unwrap();

        let error = parse_evidence(&bytes, Path::new("evidence.json")).unwrap_err();
        assert!(error.to_string().contains("violates its schema"));
    }

    #[test]
    fn rejects_native_reports_with_runner_errors() {
        let mut report = native_report("macos", "aarch64");
        report.totals.errors = 1;
        let host: EvidenceHost = serde_json::from_value(host("mac-a", "macos", "aarch64")).unwrap();

        let error = validate_native_host(&host, &report).unwrap_err();
        assert!(error
            .to_string()
            .contains("nonempty run without runner errors"));
    }

    #[test]
    fn rejects_canonical_reports_without_container_provenance() {
        let tempdir = tempfile::tempdir().unwrap();
        let report_path = tempdir.path().join("bifrost.json");
        let mut report = native_report("linux", "x86_64");
        report.environment.execution_mode = ExecutionMode::Container;
        report.environment.platform_scope = PlatformScope::CanonicalReference;
        report.environment.reference_environment =
            Some(crate::runners::ReferenceEnvironmentProvenance {
                version: "1".to_string(),
                definition_digest: format!("sha256:{}", "c".repeat(64)),
                canonical_platform: "linux/amd64".to_string(),
            });
        report.environment.analyzer_executable.sha256 = Some("d".repeat(64));
        report
            .environment
            .toolchains
            .insert("rustc".to_string(), "rustc 1.97.0".to_string());
        let report_bytes = serde_json::to_vec_pretty(&report).unwrap();
        fs::write(&report_path, &report_bytes).unwrap();
        let evidence_path = tempdir.path().join("bifrost-evidence.json");
        fs::write(
            &evidence_path,
            serde_json::to_vec_pretty(&json!({
                "schemaVersion": 1,
                "candidateId": "bifrost",
                "primaryReport": {"file": "bifrost.json", "sha256": hex_digest(&report_bytes)},
                "class": "canonical",
                "referenceRunner": "bifrost",
                "environmentVersion": "1",
                "definitionDigest": format!("sha256:{}", "c".repeat(64))
            }))
            .unwrap(),
        )
        .unwrap();

        let error = validate_evidence(
            &evidence_path,
            "bifrost",
            ReproductionClass::Canonical,
            Some("bifrost"),
            "1.1.411",
            None,
            &report_path,
            &report,
        )
        .unwrap_err();
        assert!(error.to_string().contains("lacks container provenance"));
    }

    fn host(id: &str, operating_system: &str, architecture: &str) -> serde_json::Value {
        json!({
            "id": id,
            "runnerName": format!("runner-{id}"),
            "provider": "test",
            "operatingSystem": operating_system,
            "architecture": architecture,
            "provenance": format!("https://example.test/runs/{id}"),
            "requestedVersion": "1.1.411",
            "profileSha256": PROFILE_SHA,
            "executableSha256": if operating_system == "macos" {"a".repeat(64)} else {"b".repeat(64)}
        })
    }

    fn native_report(operating_system: &str, architecture: &str) -> RunReport {
        serde_json::from_value(json!({
            "usagebenchVersion": "0.1.0",
            "usagebenchRevision": "0123456789abcdef0123456789abcdef01234567",
            "usagebenchRelease": "v0.1.0",
            "runner": {
                "name": "pyright",
                "requestedVersion": "1.1.411",
                "resolvedVersion": "not reported",
                "source": "https://github.com/microsoft/pyright/releases/tag/1.1.411",
                "adapterVersion": "0.1.0",
                "capabilities": []
            },
            "invocation": {
                "includeUnsupported": false,
                "includeDefinitionLookups": true,
                "profile": "pyright",
                "profileSha256": PROFILE_SHA
            },
            "environment": {
                "operatingSystem": operating_system,
                "architecture": architecture,
                "executionMode": "native",
                "platformScope": "host_specific",
                "analyzerExecutable": {
                    "command": "npx",
                    "sha256": if operating_system == "macos" {"a".repeat(64)} else {"b".repeat(64)}
                },
                "toolchains": {}
            },
            "startedAtUnixSeconds": 1,
            "finishedAtUnixSeconds": 2,
            "caseFiles": ["benchmarks/cases/python-lsp-parity.yaml"],
            "totals": {
                "documents": 1,
                "cases": 1,
                "developmentCases": 1,
                "evaluationCases": 0,
                "passed": 0,
                "nearMisses": 0,
                "positionUnverified": 0,
                "improved": 0,
                "failed": 0,
                "expectedFailures": 0,
                "notPlanned": 0,
                "unsupported": 0,
                "skipped": 0,
                "errors": 0,
                "requiredDestinations": {
                    "scoreableCases": 0,
                    "found": 0,
                    "missing": 0,
                    "notPlanned": 0,
                    "unsupported": 0,
                    "skipped": 0,
                    "errors": 0,
                    "unreported": 0
                }
            },
            "documents": [{
                "caseFile": "benchmarks/cases/python-lsp-parity.yaml",
                "language": "python",
                "sourceRoot": "/tmp/source",
                "corpusPartition": "development",
                "corpusSelection": "analyzer_informed",
                "groundTruthStatus": "legacy_unattributed",
                "referencePolicy": "bindings_optional",
                "cases": []
            }]
        }))
        .unwrap()
    }
}
