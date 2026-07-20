use anyhow::{bail, Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const REFERENCE_ENVIRONMENT_VARIABLE: &str = "USAGEBENCH_REFERENCE_ENVIRONMENT";
const REFERENCE_ENVIRONMENT_MARKER: &str = "/usr/local/share/usagebench/reference-environment.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Native,
    Container,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlatformScope {
    HostSpecific,
    CanonicalReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableProvenance {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceEnvironmentProvenance {
    pub version: String,
    pub definition_digest: String,
    pub canonical_platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerProvenance {
    pub image_reference: String,
    pub image_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEnvironment {
    pub operating_system: String,
    pub architecture: String,
    pub execution_mode: ExecutionMode,
    pub platform_scope: PlatformScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_environment: Option<ReferenceEnvironmentProvenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerProvenance>,
    pub analyzer_executable: ExecutableProvenance,
    pub toolchains: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceEnvironmentDescriptor {
    version: String,
    definition_digest: String,
    canonical_platform: String,
    image_reference: String,
    image_digest: String,
    usagebench_release: String,
    usagebench_revision: String,
    runner_id: String,
    #[serde(default)]
    toolchains: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EmbeddedReferenceEnvironment {
    version: String,
    definition_digest: String,
    canonical_platform: String,
    usagebench_release: String,
    usagebench_revision: String,
    runner_id: String,
}

pub(crate) fn executable_provenance(command: &Command) -> Result<ExecutableProvenance> {
    executable_provenance_for(command.get_program(), command.get_current_dir())
}

pub(crate) fn unresolved_executable(command: &OsStr) -> ExecutableProvenance {
    ExecutableProvenance {
        command: command.to_string_lossy().into_owned(),
        resolved_path: None,
        sha256: None,
    }
}

pub(crate) fn capture_execution_environment(
    analyzer_executable: ExecutableProvenance,
    observed_toolchains: &[&str],
    expected_runner: &str,
    usagebench_revision: &str,
    usagebench_release: Option<&str>,
) -> Result<ExecutionEnvironment> {
    let descriptor = env::var(REFERENCE_ENVIRONMENT_VARIABLE)
        .ok()
        .map(|raw| {
            serde_json::from_str::<ReferenceEnvironmentDescriptor>(&raw)
                .context("parse USAGEBENCH_REFERENCE_ENVIRONMENT")
        })
        .transpose()?;

    let mut toolchains = observed_toolchains
        .iter()
        .filter_map(|tool| toolchain_version(tool).map(|version| ((*tool).to_string(), version)))
        .collect::<BTreeMap<_, _>>();

    let (execution_mode, platform_scope, reference_environment, container) =
        if let Some(descriptor) = descriptor {
            validate_descriptor(&descriptor)?;
            validate_embedded_environment(
                &descriptor,
                expected_runner,
                usagebench_revision,
                usagebench_release,
            )?;
            toolchains.extend(descriptor.toolchains.clone());
            (
                ExecutionMode::Container,
                PlatformScope::CanonicalReference,
                Some(ReferenceEnvironmentProvenance {
                    version: descriptor.version,
                    definition_digest: descriptor.definition_digest,
                    canonical_platform: descriptor.canonical_platform,
                }),
                Some(ContainerProvenance {
                    image_reference: descriptor.image_reference,
                    image_digest: descriptor.image_digest,
                }),
            )
        } else {
            (
                ExecutionMode::Native,
                PlatformScope::HostSpecific,
                None,
                None,
            )
        };

    Ok(ExecutionEnvironment {
        operating_system: env::consts::OS.to_string(),
        architecture: env::consts::ARCH.to_string(),
        execution_mode,
        platform_scope,
        reference_environment,
        container,
        analyzer_executable,
        toolchains,
    })
}

fn executable_provenance_for(
    program: &OsStr,
    current_dir: Option<&Path>,
) -> Result<ExecutableProvenance> {
    let command = program.to_string_lossy().into_owned();
    let Some(path) = resolve_executable(program, current_dir) else {
        return Ok(unresolved_executable(program));
    };
    let sha256 = sha256_file(&path)
        .with_context(|| format!("checksum analyzer executable {}", path.display()))?;

    Ok(ExecutableProvenance {
        command,
        resolved_path: Some(path.to_string_lossy().into_owned()),
        sha256: Some(sha256),
    })
}

fn resolve_executable(program: &OsStr, current_dir: Option<&Path>) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    if path.components().count() > 1 {
        let candidate = current_dir.unwrap_or_else(|| Path::new(".")).join(path);
        return candidate.is_file().then_some(candidate);
    }

    env::var_os("PATH").and_then(|path_value| {
        env::split_paths(&path_value)
            .map(|directory| directory.join(path))
            .find(|candidate| candidate.is_file())
    })
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn toolchain_version(tool: &str) -> Option<String> {
    let output = Command::new(tool)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!version.is_empty()).then_some(version)
}

fn validate_descriptor(descriptor: &ReferenceEnvironmentDescriptor) -> Result<()> {
    if descriptor.version.is_empty() {
        bail!("reference environment version must not be empty");
    }
    if descriptor.canonical_platform != "linux/amd64" {
        bail!(
            "unsupported canonical reference platform `{}`",
            descriptor.canonical_platform
        );
    }
    if descriptor.image_reference.is_empty() {
        bail!("reference image reference must not be empty");
    }
    if descriptor.runner_id.is_empty()
        || descriptor.usagebench_release.is_empty()
        || descriptor.usagebench_revision.is_empty()
    {
        bail!("reference environment identity fields must not be empty");
    }
    for (field, digest) in [
        ("definitionDigest", descriptor.definition_digest.as_str()),
        ("imageDigest", descriptor.image_digest.as_str()),
    ] {
        if !is_sha256_digest(digest) {
            bail!("reference environment {field} must be a sha256 digest");
        }
    }
    Ok(())
}

fn validate_embedded_environment(
    descriptor: &ReferenceEnvironmentDescriptor,
    expected_runner: &str,
    usagebench_revision: &str,
    usagebench_release: Option<&str>,
) -> Result<()> {
    if env::consts::OS != "linux" || env::consts::ARCH != "x86_64" {
        bail!("canonical reference metadata requires a linux/amd64 harness");
    }
    let marker: EmbeddedReferenceEnvironment = serde_json::from_slice(
        &fs::read(REFERENCE_ENVIRONMENT_MARKER)
            .with_context(|| format!("read {REFERENCE_ENVIRONMENT_MARKER}"))?,
    )
    .with_context(|| format!("parse {REFERENCE_ENVIRONMENT_MARKER}"))?;
    validate_embedded_identity(
        descriptor,
        &marker,
        expected_runner,
        usagebench_revision,
        usagebench_release,
    )
}

fn validate_embedded_identity(
    descriptor: &ReferenceEnvironmentDescriptor,
    marker: &EmbeddedReferenceEnvironment,
    expected_runner: &str,
    usagebench_revision: &str,
    usagebench_release: Option<&str>,
) -> Result<()> {
    let supplied = EmbeddedReferenceEnvironment {
        version: descriptor.version.clone(),
        definition_digest: descriptor.definition_digest.clone(),
        canonical_platform: descriptor.canonical_platform.clone(),
        usagebench_release: descriptor.usagebench_release.clone(),
        usagebench_revision: descriptor.usagebench_revision.clone(),
        runner_id: descriptor.runner_id.clone(),
    };
    if marker != &supplied {
        bail!("runtime reference descriptor does not match the embedded image identity");
    }
    if marker.runner_id != expected_runner {
        bail!(
            "reference image runner `{}` does not match `{expected_runner}`",
            marker.runner_id
        );
    }
    if marker.usagebench_revision != usagebench_revision {
        bail!(
            "reference image revision `{}` does not match corpus `{usagebench_revision}`",
            marker.usagebench_revision
        );
    }
    if usagebench_release != Some(marker.usagebench_release.as_str()) {
        bail!("reference image release does not match corpus release provenance");
    }
    Ok(())
}

fn is_sha256_digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_sha256_digests() {
        assert!(is_sha256_digest(&format!("sha256:{}", "a".repeat(64))));
        assert!(!is_sha256_digest(&format!("sha256:{}", "g".repeat(64))));
        assert!(!is_sha256_digest(&"a".repeat(64)));
    }

    #[test]
    fn embedded_identity_binds_runner_and_corpus() {
        let descriptor = ReferenceEnvironmentDescriptor {
            version: "1".to_string(),
            definition_digest: format!("sha256:{}", "a".repeat(64)),
            canonical_platform: "linux/amd64".to_string(),
            image_reference: "usagebench-reference:v1.0.0-env1-gopls".to_string(),
            image_digest: format!("sha256:{}", "b".repeat(64)),
            usagebench_release: "v1.0.0".to_string(),
            usagebench_revision: "c".repeat(40),
            runner_id: "gopls".to_string(),
            toolchains: BTreeMap::new(),
        };
        let marker = EmbeddedReferenceEnvironment {
            version: descriptor.version.clone(),
            definition_digest: descriptor.definition_digest.clone(),
            canonical_platform: descriptor.canonical_platform.clone(),
            usagebench_release: descriptor.usagebench_release.clone(),
            usagebench_revision: descriptor.usagebench_revision.clone(),
            runner_id: descriptor.runner_id.clone(),
        };

        validate_embedded_identity(
            &descriptor,
            &marker,
            "gopls",
            &"c".repeat(40),
            Some("v1.0.0"),
        )
        .unwrap();
        assert!(validate_embedded_identity(
            &descriptor,
            &marker,
            "bifrost",
            &"c".repeat(40),
            Some("v1.0.0"),
        )
        .is_err());
        assert!(validate_embedded_identity(
            &descriptor,
            &marker,
            "gopls",
            &"d".repeat(40),
            Some("v1.0.0"),
        )
        .is_err());
    }
}
