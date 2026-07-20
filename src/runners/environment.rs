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
    #[serde(default)]
    toolchains: BTreeMap<String, String>,
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
}
