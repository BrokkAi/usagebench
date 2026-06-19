use anyhow::{anyhow, bail, Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub position_encoding: PositionEncoding,
    pub source: Source,
    pub language: String,
    pub cases: Vec<BenchmarkCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PositionEncoding {
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "utf-16")]
    Utf16,
    #[serde(rename = "utf-32")]
    Utf32,
}

impl Default for PositionEncoding {
    fn default() -> Self {
        Self::Utf16
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    #[serde(rename = "git")]
    Git { repo: Url, commit: String },
    #[serde(rename = "fixture")]
    Fixture { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkCase {
    pub id: String,
    pub declaration: SymbolLocation,
    #[serde(default)]
    pub expected_usages: Vec<SymbolLocation>,
    #[serde(default)]
    pub allowed_extra_usages: Vec<SymbolLocation>,
    #[serde(default)]
    pub usage_lookups: Vec<UsageLookup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsupported: Option<UnsupportedReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<Verification>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SymbolLocation {
    pub location: Location,
    pub kind: SymbolKind,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disambiguation: Option<Disambiguation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub uri: Url,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Class,
    Constructor,
    Method,
    Function,
    Field,
    Variable,
    Constant,
    Module,
    Package,
    Interface,
    Type,
    Property,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Disambiguation {
    FirstMatchingSymbol,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageLookup {
    pub usage: SymbolLocation,
    pub expected_declaration: SymbolLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedReason {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    pub method: VerificationMethod,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMethod {
    ManualInspection,
    Lsp,
    AnalyzerComparison,
}

pub fn generated_schema_json() -> Result<String> {
    let schema = schemars::schema_for!(BenchmarkDocument);
    serde_json::to_string_pretty(&schema).context("serialize generated benchmark schema")
}

pub fn validate_path(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let path = path.as_ref();
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/benchmark-case.schema.json"))
            .context("parse bundled benchmark schema")?;
    let compiled_schema = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow!("compile bundled benchmark schema: {error}"))?;

    let mut files = Vec::new();
    collect_case_files(path, &mut files)?;
    files.sort();

    if files.is_empty() {
        bail!(
            "no benchmark case YAML files found under {}",
            path.display()
        );
    }

    for file in &files {
        validate_file(file, &compiled_schema)?;
    }

    Ok(files)
}

fn collect_case_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        if is_yaml_file(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }

    if !path.is_dir() {
        bail!("{} is neither a file nor a directory", path.display());
    }

    for entry in fs::read_dir(path).with_context(|| format!("read {}", path.display()))? {
        let entry = entry.with_context(|| format!("read entry under {}", path.display()))?;
        let child = entry.path();
        if child.is_dir() {
            collect_case_files(&child, files)?;
        } else if is_yaml_file(&child) {
            files.push(child);
        }
    }

    Ok(())
}

fn is_yaml_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yaml" | "yml")
    )
}

fn validate_file(file: &Path, compiled_schema: &jsonschema::JSONSchema) -> Result<()> {
    let yaml = fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?;
    let document: serde_yaml::Value =
        serde_yaml::from_str(&yaml).with_context(|| format!("parse YAML {}", file.display()))?;
    let json = serde_json::to_value(document)
        .with_context(|| format!("convert YAML to JSON {}", file.display()))?;

    if let Err(errors) = compiled_schema.validate(&json) {
        let messages = errors
            .map(|error| format!("{}: {}", error.instance_path, error))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(anyhow!(
            "{} failed schema validation:\n{}",
            file.display(),
            messages
        ));
    }

    serde_yaml::from_str::<BenchmarkDocument>(&yaml)
        .with_context(|| format!("deserialize benchmark document {}", file.display()))?
        .validate()
        .with_context(|| format!("validate benchmark semantics {}", file.display()))?;
    Ok(())
}

impl BenchmarkDocument {
    pub fn validate(&self) -> Result<()> {
        for case in &self.cases {
            case.validate()?;
        }

        Ok(())
    }
}

impl BenchmarkCase {
    fn validate(&self) -> Result<()> {
        self.declaration
            .validate()
            .with_context(|| format!("case {} declaration", self.id))?;

        for usage in &self.expected_usages {
            usage
                .validate()
                .with_context(|| format!("case {} expectedUsages", self.id))?;
        }

        for usage in &self.allowed_extra_usages {
            usage
                .validate()
                .with_context(|| format!("case {} allowedExtraUsages", self.id))?;
        }

        for lookup in &self.usage_lookups {
            lookup
                .usage
                .validate()
                .with_context(|| format!("case {} usageLookups usage", self.id))?;
            lookup
                .expected_declaration
                .validate()
                .with_context(|| format!("case {} usageLookups expectedDeclaration", self.id))?;
        }

        Ok(())
    }
}

impl SymbolLocation {
    fn validate(&self) -> Result<()> {
        if !self.location.uri.scheme().eq_ignore_ascii_case("benchmark")
            || self.location.uri.host_str() != Some("source")
        {
            bail!(
                "location uri {} must use the benchmark://source/... form",
                self.location.uri
            );
        }

        self.location.range.validate()?;

        if self.location.range.is_zero_width() {
            if self.disambiguation != Some(Disambiguation::FirstMatchingSymbol) {
                bail!("zero-width line-only ranges require disambiguation: first_matching_symbol");
            }
        }

        Ok(())
    }
}

impl Range {
    fn validate(&self) -> Result<()> {
        if (self.start.line, self.start.character) > (self.end.line, self.end.character) {
            bail!("range start must be before or equal to range end");
        }

        Ok(())
    }

    fn is_zero_width(&self) -> bool {
        self.start == self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_position_encoding_is_utf16() {
        let yaml = r#"
schemaVersion: 1
source:
  kind: fixture
  path: fixtures/java/basic
language: java
cases: []
"#;

        let document = serde_yaml::from_str::<BenchmarkDocument>(yaml).unwrap();

        assert_eq!(document.position_encoding, PositionEncoding::Utf16);
    }

    #[test]
    fn validates_example_cases() {
        let files = validate_path("benchmarks/cases").unwrap();

        assert!(!files.is_empty());
    }
}
