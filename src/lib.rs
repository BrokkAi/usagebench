use anyhow::{anyhow, bail, Context, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use url::Url;

pub mod runners;

/// Compatibility re-export for integrations that imported the original module.
pub use runners::bifrost as bifrost_runner;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub position_encoding: PositionEncoding,
    pub source: Source,
    pub language: String,
    pub corpus: CorpusMetadata,
    pub ground_truth: GroundTruthReview,
    pub reference_policy: ReferencePolicy,
    pub cases: Vec<BenchmarkCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CorpusMetadata {
    pub partition: CorpusPartition,
    pub selection: CorpusSelection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freeze_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CorpusPartition {
    Development,
    Evaluation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CorpusSelection {
    AnalyzerInformed,
    PreRegistered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroundTruthReview {
    pub status: GroundTruthReviewStatus,
    #[serde(default)]
    pub reviewers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroundTruthReviewStatus {
    LegacyUnattributed,
    AuthorReviewed,
    IndependentlyReviewed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferencePolicy {
    ExternalUsages,
    BindingsOptional,
    BindingsRequired,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PositionEncoding {
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "utf-16")]
    #[default]
    Utf16,
    #[serde(rename = "utf-32")]
    Utf32,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration: Option<SymbolLocation>,
    #[serde(default)]
    pub expected_usages: Vec<SymbolLocation>,
    #[serde(default)]
    pub expected_unproven_usages: Vec<SymbolLocation>,
    #[serde(default)]
    pub allowed_extra_usages: Vec<SymbolLocation>,
    #[serde(default)]
    pub allowed_unproven_usages: Vec<SymbolLocation>,
    #[serde(default)]
    pub usage_lookups: Vec<UsageLookup>,
    #[serde(default)]
    pub type_lookups: Vec<TypeLookup>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_failure: Option<ExpectedFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_planned: Option<NotPlannedReason>,
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
    #[serde(default)]
    pub operation: NavigationOperation,
    /// Accept either no result or an exact self-target, but reject navigation
    /// to any other token. `expectedDeclaration` must equal `usage`.
    #[serde(default)]
    pub expect_no_movement: bool,
    pub usage: SymbolLocation,
    pub expected_declaration: SymbolLocation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NavigationOperation {
    /// Preserve the legacy profile-selected endpoint while development cases
    /// are reviewed. Frozen evaluation cases must choose an explicit operation.
    #[default]
    ProfileDefault,
    Declaration,
    Definition,
}

impl NavigationOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProfileDefault => "profile_default",
            Self::Declaration => "declaration",
            Self::Definition => "definition",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TypeLookup {
    pub expression: SymbolLocation,
    pub expected_type: SymbolLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedFailure {
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotPlannedReason {
    pub reason: String,
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
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schema/benchmark-case.schema.json"))
            .context("parse checked-in benchmark schema")?;
    serde_json::to_string_pretty(&schema).context("serialize generated benchmark schema")
}

pub fn validate_path(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let path = path.as_ref();
    let repo_root = find_repo_root_for_path(path)?;
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
        validate_file(file, &compiled_schema, &repo_root)?;
    }

    Ok(files)
}

fn find_repo_root_for_path(path: &Path) -> Result<PathBuf> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .context("read current directory")?
            .join(path)
    };
    let search_start = if absolute_path.is_file() {
        absolute_path.parent().unwrap_or(&absolute_path)
    } else {
        absolute_path.as_path()
    };

    for ancestor in search_start.ancestors() {
        if ancestor.join("Cargo.toml").is_file() && ancestor.join("schema").is_dir() {
            return Ok(ancestor.to_path_buf());
        }
    }

    bail!("could not find usagebench repo root for {}", path.display());
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

fn validate_file(
    file: &Path,
    compiled_schema: &jsonschema::JSONSchema,
    repo_root: &Path,
) -> Result<()> {
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
        .validate_with_base(repo_root)
        .with_context(|| format!("validate benchmark semantics {}", file.display()))?;
    Ok(())
}

impl BenchmarkDocument {
    pub fn validate(&self) -> Result<()> {
        self.validate_with_base(Path::new("."))
    }

    fn validate_with_base(&self, base_dir: &Path) -> Result<()> {
        if self.schema_version != 2 {
            bail!(
                "schemaVersion must be 2 for corpus, ground-truth, and reference-policy metadata"
            );
        }
        self.validate_methodology_metadata()?;
        let fixture_root = match &self.source {
            Source::Fixture { path } => {
                validate_fixture_source_path(path)?;
                let fixture_root = base_dir.join(path);
                if !fixture_root.is_dir() {
                    bail!(
                        "fixture source path {} does not exist or is not a directory",
                        fixture_root.display()
                    );
                }
                let canonical_base = base_dir
                    .canonicalize()
                    .with_context(|| format!("canonicalize {}", base_dir.display()))?;
                let canonical_fixture = fixture_root
                    .canonicalize()
                    .with_context(|| format!("canonicalize {}", fixture_root.display()))?;
                let allowed_root = canonical_base.join("fixtures");
                if !canonical_fixture.starts_with(&allowed_root) {
                    bail!(
                        "fixture source path {} must stay under {}",
                        fixture_root.display(),
                        allowed_root.display()
                    );
                }
                Some(canonical_fixture)
            }
            Source::Git { .. } => None,
        };

        for case in &self.cases {
            case.validate(fixture_root.as_deref(), self.position_encoding)?;
        }

        Ok(())
    }

    fn validate_methodology_metadata(&self) -> Result<()> {
        let unique_reviewers = self
            .ground_truth
            .reviewers
            .iter()
            .map(|reviewer| reviewer.trim())
            .filter(|reviewer| !reviewer.is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        if unique_reviewers.len() != self.ground_truth.reviewers.len() {
            bail!("groundTruth reviewers must be non-empty and unique");
        }
        if self.ground_truth.status == GroundTruthReviewStatus::IndependentlyReviewed
            && unique_reviewers.len() < 2
        {
            bail!("independently_reviewed ground truth requires at least two reviewers");
        }
        if self.corpus.partition == CorpusPartition::Evaluation {
            if self.corpus.selection != CorpusSelection::PreRegistered {
                bail!("evaluation corpus documents must use pre_registered selection");
            }
            if self.ground_truth.status != GroundTruthReviewStatus::IndependentlyReviewed {
                bail!("evaluation corpus documents must be independently_reviewed");
            }
            if self
                .corpus
                .freeze_id
                .as_deref()
                .is_none_or(|freeze_id| freeze_id.trim().is_empty())
            {
                bail!("evaluation corpus documents require a non-empty freezeId");
            }
            if self.cases.iter().any(|case| {
                case.usage_lookups
                    .iter()
                    .any(|lookup| lookup.operation == NavigationOperation::ProfileDefault)
            }) {
                bail!("evaluation corpus usageLookups require an explicit navigation operation");
            }
        }
        Ok(())
    }
}

impl BenchmarkCase {
    fn validate(&self, fixture_root: Option<&Path>, encoding: PositionEncoding) -> Result<()> {
        let scoring_markers = [
            self.expected_failure.is_some(),
            self.not_planned.is_some(),
            self.unsupported.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        if scoring_markers > 1 {
            bail!(
                "case {} must use at most one of expectedFailure, notPlanned, or unsupported",
                self.id
            );
        }

        if let Some(declaration) = &self.declaration {
            declaration
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} declaration", self.id))?;
        }

        for usage in &self.expected_usages {
            usage
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} expectedUsages", self.id))?;
        }

        for usage in &self.expected_unproven_usages {
            usage
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} expectedUnprovenUsages", self.id))?;
        }

        for usage in &self.allowed_extra_usages {
            usage
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} allowedExtraUsages", self.id))?;
        }

        for usage in &self.allowed_unproven_usages {
            usage
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} allowedUnprovenUsages", self.id))?;
        }

        for lookup in &self.usage_lookups {
            lookup
                .usage
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} usageLookups usage", self.id))?;
            lookup
                .expected_declaration
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} usageLookups expectedDeclaration", self.id))?;
            if lookup.expect_no_movement
                && lookup.expected_declaration.location != lookup.usage.location
            {
                bail!(
                    "case {} no-movement lookup expectedDeclaration must equal its usage location",
                    self.id
                );
            }
        }

        for lookup in &self.type_lookups {
            lookup
                .expression
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} typeLookups expression", self.id))?;
            lookup
                .expected_type
                .validate(fixture_root, encoding)
                .with_context(|| format!("case {} typeLookups expectedType", self.id))?;
        }

        Ok(())
    }
}

impl SymbolLocation {
    fn validate(&self, fixture_root: Option<&Path>, encoding: PositionEncoding) -> Result<()> {
        if !self.location.uri.scheme().eq_ignore_ascii_case("benchmark")
            || self.location.uri.host_str() != Some("source")
        {
            bail!(
                "location uri {} must use the benchmark://source/... form",
                self.location.uri
            );
        }

        self.location.range.validate()?;

        if self.location.range.is_zero_width()
            && self.disambiguation != Some(Disambiguation::FirstMatchingSymbol)
        {
            bail!("zero-width line-only ranges require disambiguation: first_matching_symbol");
        }

        if let Some(fixture_root) = fixture_root {
            self.location
                .validate_fixture_range(fixture_root, encoding)?;
            if !self.location.range.is_zero_width() {
                let selected_text = self.location.fixture_range_text(fixture_root, encoding)?;
                if selected_text != self.display_name {
                    bail!(
                        "range for {} does not select displayName {:?}",
                        self.location.uri,
                        self.display_name
                    );
                }
            }
        }

        Ok(())
    }
}

impl Location {
    fn validate_fixture_range(
        &self,
        fixture_root: &Path,
        encoding: PositionEncoding,
    ) -> Result<()> {
        let source_path = self.fixture_source_path(fixture_root)?;
        let source = fs::read_to_string(&source_path)
            .with_context(|| format!("read {}", source_path.display()))?;
        self.range
            .validate_with_source_text(&source, encoding)
            .with_context(|| format!("range for {} in {}", self.uri, source_path.display()))?;

        Ok(())
    }

    fn fixture_range_text(
        &self,
        fixture_root: &Path,
        encoding: PositionEncoding,
    ) -> Result<String> {
        let source_path = self.fixture_source_path(fixture_root)?;
        let source = fs::read_to_string(&source_path)
            .with_context(|| format!("read {}", source_path.display()))?;
        self.range
            .text_from_source(&source, encoding)
            .with_context(|| format!("range for {} in {}", self.uri, source_path.display()))
    }

    fn fixture_source_path(&self, fixture_root: &Path) -> Result<PathBuf> {
        let relative_path = benchmark_source_path(&self.uri)?;
        let source_path = fixture_root.join(&relative_path);

        if !source_path.is_file() {
            bail!(
                "location uri {} maps to missing fixture file {}",
                self.uri,
                source_path.display()
            );
        }

        Ok(source_path)
    }
}

impl Range {
    fn validate(&self) -> Result<()> {
        if (self.start.line, self.start.character) > (self.end.line, self.end.character) {
            bail!("range start must be before or equal to range end");
        }

        Ok(())
    }

    fn validate_with_source_text(&self, source: &str, encoding: PositionEncoding) -> Result<()> {
        validate_position_with_source_text(&self.start, source, encoding).context("range start")?;
        validate_position_with_source_text(&self.end, source, encoding).context("range end")?;
        Ok(())
    }

    fn text_from_source(&self, source: &str, encoding: PositionEncoding) -> Result<String> {
        if self.start.line != self.end.line {
            bail!("symbol ranges in fixture cases must stay on a single line");
        }

        let line = source
            .split('\n')
            .nth(self.start.line as usize)
            .ok_or_else(|| anyhow!("line {} is outside the file", self.start.line))?;
        let line = line.strip_suffix('\r').unwrap_or(line);
        let start = byte_index_for_position(line, self.start.character, encoding)
            .context("range start character")?;
        let end = byte_index_for_position(line, self.end.character, encoding)
            .context("range end character")?;

        Ok(line[start..end].to_string())
    }

    fn is_zero_width(&self) -> bool {
        self.start == self.end
    }
}

fn validate_fixture_source_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_absolute() {
        bail!("fixture source path must be relative");
    }
    let mut components = path.components();
    if !matches!(components.next(), Some(Component::Normal(first)) if first == "fixtures") {
        bail!("fixture source path must start with fixtures/");
    }
    if components.any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!("fixture source path must not contain parent or root components");
    }

    Ok(())
}

fn benchmark_source_path(uri: &Url) -> Result<PathBuf> {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        bail!("location uri {uri} must include a source-relative path");
    }

    let path = Path::new(path);
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        bail!("location uri {uri} must not contain parent directory segments");
    }

    Ok(path.to_path_buf())
}

fn validate_position_with_source_text(
    position: &Position,
    source: &str,
    encoding: PositionEncoding,
) -> Result<()> {
    let Some(line) = source.split('\n').nth(position.line as usize) else {
        bail!("line {} is outside the file", position.line);
    };
    let line = line.strip_suffix('\r').unwrap_or(line);
    let line_len = line_len_for_encoding(line, encoding);

    if position.character > line_len {
        bail!(
            "character {} is outside line {} with length {}",
            position.character,
            position.line,
            line_len
        );
    }

    Ok(())
}

fn line_len_for_encoding(line: &str, encoding: PositionEncoding) -> u32 {
    match encoding {
        PositionEncoding::Utf8 => line.len() as u32,
        PositionEncoding::Utf16 => line.encode_utf16().count() as u32,
        PositionEncoding::Utf32 => line.chars().count() as u32,
    }
}

fn byte_index_for_position(
    line: &str,
    character: u32,
    encoding: PositionEncoding,
) -> Result<usize> {
    match encoding {
        PositionEncoding::Utf8 => byte_index_for_utf8_position(line, character),
        PositionEncoding::Utf16 => byte_index_for_utf16_position(line, character),
        PositionEncoding::Utf32 => byte_index_for_utf32_position(line, character),
    }
}

fn byte_index_for_utf8_position(line: &str, character: u32) -> Result<usize> {
    let byte_index = character as usize;
    if byte_index > line.len() {
        bail!(
            "character {character} is outside line with length {}",
            line.len()
        );
    }
    if !line.is_char_boundary(byte_index) {
        bail!("character {character} does not align to a UTF-8 character boundary");
    }

    Ok(byte_index)
}

fn byte_index_for_utf16_position(line: &str, character: u32) -> Result<usize> {
    let mut offset = 0;
    for (byte_index, ch) in line.char_indices() {
        if offset == character {
            return Ok(byte_index);
        }
        offset += ch.len_utf16() as u32;
        if offset > character {
            bail!("character {character} splits a UTF-16 surrogate pair");
        }
    }

    if offset == character {
        Ok(line.len())
    } else {
        bail!("character {character} is outside line with length {offset}")
    }
}

fn byte_index_for_utf32_position(line: &str, character: u32) -> Result<usize> {
    if character == line.chars().count() as u32 {
        return Ok(line.len());
    }
    line.char_indices()
        .nth(character as usize)
        .map(|(byte_index, _)| byte_index)
        .ok_or_else(|| {
            anyhow!(
                "character {character} is outside line with length {}",
                line.chars().count()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn default_position_encoding_is_utf16() {
        let yaml = r#"
schemaVersion: 2
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
referencePolicy: bindings_optional
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
    fn schema_command_uses_checked_schema_v2_contract() {
        let schema: serde_json::Value =
            serde_json::from_str(&generated_schema_json().unwrap()).unwrap();

        assert_eq!(schema["properties"]["schemaVersion"]["const"], 2);
        assert_eq!(
            schema["properties"]["referencePolicy"]["enum"],
            serde_json::json!(["external_usages", "bindings_optional", "bindings_required"])
        );
    }

    #[test]
    fn validates_example_cases() {
        let files = validate_path("benchmarks/cases").unwrap();

        assert!(!files.is_empty());
    }

    #[test]
    fn validation_rejects_multiple_scoring_markers() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::create_dir_all(tempdir.path().join("fixtures/fixture")).unwrap();
        let document = serde_yaml::from_str::<BenchmarkDocument>(
            r#"
schemaVersion: 2
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
referencePolicy: bindings_optional
source:
  kind: fixture
  path: fixtures/fixture
language: text
cases:
  - id: ambiguous-marker
    expectedFailure:
      reason: planned gap
    notPlanned:
      reason: out of scope
"#,
        )
        .unwrap();

        let error = document.validate_with_base(tempdir.path()).unwrap_err();

        assert!(format!("{error:#}").contains("must use at most one"));
    }

    #[test]
    fn fixture_validation_rejects_missing_source_files() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::create_dir_all(tempdir.path().join("fixtures/fixture")).unwrap();
        let document = serde_yaml::from_str::<BenchmarkDocument>(
            r#"
schemaVersion: 2
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
referencePolicy: bindings_optional
source:
  kind: fixture
  path: fixtures/fixture
language: text
cases:
  - id: missing-file
    declaration:
      location:
        uri: benchmark://source/src/missing.txt
        range:
          start: { line: 0, character: 0 }
          end: { line: 0, character: 5 }
      kind: variable
      displayName: value
    expectedUsages: []
    usageLookups: []
"#,
        )
        .unwrap();

        let error = document.validate_with_base(tempdir.path()).unwrap_err();

        assert!(format!("{error:#}").contains("missing fixture file"));
    }

    #[test]
    fn fixture_validation_rejects_display_name_mismatches() {
        let tempdir = tempfile::tempdir().unwrap();
        let fixture = tempdir.path().join("fixtures/fixture/src");
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("sample.txt"), "let value = 1\n").unwrap();
        let document = serde_yaml::from_str::<BenchmarkDocument>(
            r#"
schemaVersion: 2
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
referencePolicy: bindings_optional
source:
  kind: fixture
  path: fixtures/fixture
language: text
cases:
  - id: mismatch
    declaration:
      location:
        uri: benchmark://source/src/sample.txt
        range:
          start: { line: 0, character: 4 }
          end: { line: 0, character: 9 }
      kind: variable
      displayName: other
    expectedUsages: []
    usageLookups: []
"#,
        )
        .unwrap();

        let error = document.validate_with_base(tempdir.path()).unwrap_err();

        assert!(format!("{error:#}").contains("does not select displayName"));
    }

    #[test]
    fn fixture_validation_rejects_absolute_source_paths() {
        let tempdir = tempfile::tempdir().unwrap();
        let document = serde_yaml::from_str::<BenchmarkDocument>(
            r#"
schemaVersion: 2
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
referencePolicy: bindings_optional
source:
  kind: fixture
  path: /tmp
language: text
cases: []
"#,
        )
        .unwrap();

        let error = document.validate_with_base(tempdir.path()).unwrap_err();

        assert!(format!("{error:#}").contains("fixture source path must be relative"));
    }

    #[test]
    fn validates_utf8_fixture_positions() {
        let tempdir = tempfile::tempdir().unwrap();
        let fixture = tempdir.path().join("fixtures/fixture/src");
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("sample.txt"), "let café = 1\n").unwrap();
        let document = serde_yaml::from_str::<BenchmarkDocument>(
            r#"
schemaVersion: 2
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
referencePolicy: bindings_optional
positionEncoding: utf-8
source:
  kind: fixture
  path: fixtures/fixture
language: text
cases:
  - id: utf8
    declaration:
      location:
        uri: benchmark://source/src/sample.txt
        range:
          start: { line: 0, character: 4 }
          end: { line: 0, character: 9 }
      kind: variable
      displayName: café
    expectedUsages: []
    usageLookups: []
"#,
        )
        .unwrap();

        document.validate_with_base(tempdir.path()).unwrap();
    }

    #[test]
    fn validate_path_resolves_fixtures_from_repo_root() {
        let tempdir = tempfile::tempdir().unwrap();
        let repo_root = tempdir.path();
        fs::write(
            repo_root.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\n",
        )
        .unwrap();
        fs::create_dir(repo_root.join("schema")).unwrap();
        let fixture = repo_root.join("fixtures/fixture/src");
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("sample.txt"), "let value = 1\n").unwrap();
        let cases = repo_root.join("benchmarks/cases");
        fs::create_dir_all(&cases).unwrap();
        fs::write(
            cases.join("sample.yaml"),
            r#"
schemaVersion: 2
corpus:
  partition: development
  selection: analyzer_informed
groundTruth:
  status: legacy_unattributed
  reviewers: []
referencePolicy: bindings_optional
source:
  kind: fixture
  path: fixtures/fixture
language: text
cases:
  - id: value
    declaration:
      location:
        uri: benchmark://source/src/sample.txt
        range:
          start: { line: 0, character: 4 }
          end: { line: 0, character: 9 }
      kind: variable
      displayName: value
    expectedUsages: []
    usageLookups: []
"#,
        )
        .unwrap();

        let files = validate_path(&cases).unwrap();

        assert_eq!(files, vec![cases.join("sample.yaml")]);
    }

    #[test]
    fn evaluation_corpus_requires_preregistered_selection() {
        let document = methodology_document(
            "evaluation",
            "analyzer_informed",
            "independently_reviewed",
            "  freezeId: eval-v1\n",
            "  reviewers: [reviewer-a, reviewer-b]",
        );

        let error = document.validate_methodology_metadata().unwrap_err();

        assert!(format!("{error:#}").contains("pre_registered"));
    }

    #[test]
    fn evaluation_corpus_requires_two_independent_reviewers() {
        let document = methodology_document(
            "evaluation",
            "pre_registered",
            "independently_reviewed",
            "  freezeId: eval-v1\n",
            "  reviewers: [reviewer-a]",
        );

        let error = document.validate_methodology_metadata().unwrap_err();

        assert!(format!("{error:#}").contains("at least two reviewers"));
    }

    #[test]
    fn evaluation_corpus_requires_freeze_id() {
        let document = methodology_document(
            "evaluation",
            "pre_registered",
            "independently_reviewed",
            "",
            "  reviewers: [reviewer-a, reviewer-b]",
        );

        let error = document.validate_methodology_metadata().unwrap_err();

        assert!(format!("{error:#}").contains("freezeId"));
    }

    #[test]
    fn accepts_frozen_independently_reviewed_evaluation_corpus() {
        let document = methodology_document(
            "evaluation",
            "pre_registered",
            "independently_reviewed",
            "  freezeId: eval-v1\n",
            "  reviewers: [reviewer-a, reviewer-b]",
        );

        document.validate_methodology_metadata().unwrap();
    }

    #[test]
    fn evaluation_corpus_requires_explicit_navigation_operations() {
        let mut document = methodology_document(
            "evaluation",
            "pre_registered",
            "independently_reviewed",
            "  freezeId: eval-v1\n",
            "  reviewers: [reviewer-a, reviewer-b]",
        );
        document.cases.push(
            serde_yaml::from_str(
                r#"id: implicit-navigation
usageLookups:
  - usage:
      location:
        uri: benchmark://source/src/lib.rs
        range:
          start: { line: 1, character: 4 }
          end: { line: 1, character: 8 }
      kind: module
      displayName: item
    expectedDeclaration:
      location:
        uri: benchmark://source/src/lib.rs
        range:
          start: { line: 0, character: 4 }
          end: { line: 0, character: 8 }
      kind: module
      displayName: item
"#,
            )
            .unwrap(),
        );

        let error = document.validate_methodology_metadata().unwrap_err();

        assert!(format!("{error:#}").contains("explicit navigation operation"));
    }

    fn methodology_document(
        partition: &str,
        selection: &str,
        review_status: &str,
        freeze_id: &str,
        reviewers: &str,
    ) -> BenchmarkDocument {
        serde_yaml::from_str(&format!(
            r#"schemaVersion: 2
corpus:
  partition: {partition}
  selection: {selection}
{freeze_id}groundTruth:
  status: {review_status}
{reviewers}
referencePolicy: bindings_optional
source:
  kind: fixture
  path: fixtures/fixture
language: text
cases: []
"#
        ))
        .unwrap()
    }
}
