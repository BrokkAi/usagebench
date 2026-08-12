//! Source-only, pre-review freeze for selecting the retrospective legacy cohort.

use crate::{
    evaluation::safe_repo_relative_path, BenchmarkCase, BenchmarkDocument, Location, Source,
};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const SCHEMA: &str = include_str!("../schema/legacy-promotion-cohort.schema.json");
const POLICY_FILE: &str = "docs/legacy-promotion-selection-policy.md";
const GENERATOR_FILE: &str = "src/promotion_cohort.rs";
const LEGACY_CASE_COUNT: usize = 158;
const LEGACY_LANGUAGE_COUNT: usize = 11;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactLink {
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceDocument {
    pub file: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryInventory {
    pub operation: String,
    pub status: String,
    pub uri: String,
    pub range: crate::Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocationCounts {
    pub required: usize,
    pub optional: usize,
    pub unproven: usize,
    pub excluded: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewProvenance {
    pub method: String,
    pub notes_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InventoryRow {
    pub language: String,
    pub document: String,
    pub case_id: String,
    pub source_uri: String,
    pub queries: Vec<QueryInventory>,
    pub symbol_kind: String,
    pub semantic_feature: String,
    pub source_complexity: String,
    pub location_counts: LocationCounts,
    pub fixture_determinism: String,
    pub project_load_evidence: String,
    pub exact_range_validity: String,
    pub duplication_group: String,
    pub first_review_provenance: ReviewProvenance,
    pub decision: String,
    pub selection_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LegacyPromotionCohort {
    pub schema_version: u32,
    pub cohort_id: String,
    pub selection_provenance: String,
    pub selection_basis: String,
    pub analyzer_outcome_use: String,
    pub policy: ArtifactLink,
    pub generator: ArtifactLink,
    pub source_documents: Vec<SourceDocument>,
    pub eligible_counts: BTreeMap<String, usize>,
    pub balanced_core_per_language: usize,
    pub inventory: Vec<InventoryRow>,
}

#[derive(Clone)]
struct Candidate {
    row: InventoryRow,
    operation_tags: BTreeSet<String>,
}

pub fn generate(cases_path: &Path, output: &Path) -> Result<LegacyPromotionCohort> {
    let root = crate::find_repo_root_for_path(cases_path)?;
    let mut files = Vec::new();
    collect_yaml(cases_path, &mut files)?;
    files.sort();

    let mut source_documents = Vec::new();
    let mut candidates = Vec::new();
    for file in files {
        crate::validate_path(&file)
            .with_context(|| format!("validate legacy source document {}", file.display()))?;
        let bytes = fs::read(&file).with_context(|| format!("read {}", file.display()))?;
        let document: BenchmarkDocument =
            serde_yaml::from_slice(&bytes).with_context(|| format!("parse {}", file.display()))?;
        // Published semantic-pack navigation cases postdate the frozen 158-case legacy corpus.
        if document.semantic_packs.is_some() {
            continue;
        }
        let relative = repo_relative(&root, &file)?;
        source_documents.push(SourceDocument {
            file: relative.clone(),
            sha256: sha256(&bytes),
        });
        let source_uri = match &document.source {
            Source::Fixture { .. } => "benchmark://source/".to_string(),
            Source::Git { .. } => {
                bail!("legacy cohort may only contain checked-in fixture sources")
            }
        };
        let project_load_evidence = project_load_evidence(&root, &document.source)?;
        for case in &document.cases {
            candidates.push(candidate(
                &document,
                case,
                &relative,
                &source_uri,
                &project_load_evidence,
            )?);
        }
    }
    if candidates.len() != LEGACY_CASE_COUNT {
        bail!(
            "legacy population drift: expected {LEGACY_CASE_COUNT} cases, found {}",
            candidates.len()
        );
    }

    let mut by_language: BTreeMap<String, Vec<Candidate>> = BTreeMap::new();
    for candidate in candidates {
        by_language
            .entry(candidate.row.language.clone())
            .or_default()
            .push(candidate);
    }
    if by_language.len() != LEGACY_LANGUAGE_COUNT {
        bail!("legacy population must contain exactly 11 languages");
    }
    let eligible_counts = by_language
        .iter()
        .map(|(language, rows)| {
            (
                language.clone(),
                rows.iter().filter(|row| is_eligible(&row.row)).count(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let balanced_core_per_language = 10.min(*eligible_counts.values().min().unwrap());

    let mut inventory = Vec::with_capacity(LEGACY_CASE_COUNT);
    for (language, rows) in by_language {
        let (eligible, mut controls): (Vec<_>, Vec<_>) =
            rows.into_iter().partition(|row| is_eligible(&row.row));
        let ordered = diversity_order(eligible);
        for (index, mut candidate) in ordered.into_iter().enumerate() {
            candidate.row.selection_order = index + 1;
            candidate.row.decision = if index < balanced_core_per_language {
                "balanced_core".into()
            } else {
                "overflow".into()
            };
            inventory.push(candidate.row);
        }
        controls.sort_by(|a, b| row_key(&a.row).cmp(&row_key(&b.row)));
        let offset = inventory
            .iter()
            .filter(|row| row.language == language)
            .count();
        for (index, mut candidate) in controls.into_iter().enumerate() {
            candidate.row.selection_order = offset + index + 1;
            inventory.push(candidate.row);
        }
    }
    inventory.sort_by(|a, b| {
        (&a.language, a.selection_order, &a.document, &a.case_id).cmp(&(
            &b.language,
            b.selection_order,
            &b.document,
            &b.case_id,
        ))
    });

    let policy_path = root.join(POLICY_FILE);
    let cohort = LegacyPromotionCohort {
        schema_version: 1,
        cohort_id: "legacy-promotion-v1-source-only-cohort".into(),
        selection_provenance: "retrospectively_selected".into(),
        selection_basis: "source_only".into(),
        analyzer_outcome_use: "forbidden".into(),
        policy: ArtifactLink {
            file: POLICY_FILE.into(),
            sha256: sha256(&fs::read(&policy_path).context("read cohort policy")?),
        },
        generator: ArtifactLink {
            file: GENERATOR_FILE.into(),
            sha256: sha256(&fs::read(root.join(GENERATOR_FILE)).context("read cohort generator")?),
        },
        source_documents,
        eligible_counts,
        balanced_core_per_language,
        inventory,
    };
    validate_value(&root, &cohort)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(
        output,
        format!("{}\n", serde_json::to_string_pretty(&cohort)?),
    )
    .with_context(|| format!("write {}", output.display()))?;
    Ok(cohort)
}

pub fn validate(path: &Path) -> Result<LegacyPromotionCohort> {
    let root = crate::find_repo_root_for_path(path)?;
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).context("parse cohort JSON")?;
    let schema: serde_json::Value = serde_json::from_str(SCHEMA)?;
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|error| anyhow!("compile cohort schema: {error}"))?;
    if let Err(errors) = compiled.validate(&value) {
        bail!(
            "cohort schema validation failed: {}",
            errors
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let cohort: LegacyPromotionCohort = serde_json::from_value(value)?;
    validate_value(&root, &cohort)?;
    let temporary = tempfile::tempdir().context("create cohort regeneration directory")?;
    let expected = generate(
        &root.join("benchmarks/cases"),
        &temporary.path().join("cohort.json"),
    )?;
    if cohort != expected {
        bail!(
            "cohort differs from deterministic source-only regeneration; create a new versioned cohort instead of editing frozen membership or order"
        );
    }
    Ok(cohort)
}

fn validate_value(root: &Path, cohort: &LegacyPromotionCohort) -> Result<()> {
    if cohort.selection_provenance != "retrospectively_selected"
        || cohort.selection_basis != "source_only"
        || cohort.analyzer_outcome_use != "forbidden"
    {
        bail!("legacy cohort must preserve retrospective, source-only provenance");
    }
    validate_link(root, &cohort.policy, "selection policy")?;
    validate_link(root, &cohort.generator, "selection generator")?;
    if cohort.eligible_counts.len() != LEGACY_LANGUAGE_COUNT {
        bail!("eligibleCounts must contain exactly 11 languages");
    }
    let expected_n = 10.min(*cohort.eligible_counts.values().min().unwrap());
    if cohort.balanced_core_per_language != expected_n {
        bail!("balanced-core denominator drift: expected N={expected_n}");
    }
    if cohort.inventory.len() != LEGACY_CASE_COUNT {
        bail!("inventory must contain exactly 158 rows");
    }
    let mut source_hashes = BTreeMap::new();
    for source in &cohort.source_documents {
        let path = safe_join(root, &source.file)?;
        let bytes = fs::read(path)?;
        if sha256(&bytes) != source.sha256 {
            bail!("historical source document hash changed: {}", source.file);
        }
        source_hashes.insert(source.file.as_str(), source.sha256.as_str());
    }
    let mut ids = BTreeSet::new();
    let mut eligible = BTreeMap::<&str, usize>::new();
    let mut core = BTreeMap::<&str, usize>::new();
    let mut orders = BTreeSet::new();
    for row in &cohort.inventory {
        if !source_hashes.contains_key(row.document.as_str()) {
            bail!(
                "inventory row references an unbound document: {}",
                row.document
            );
        }
        if !ids.insert(row.case_id.as_str()) {
            bail!("duplicate inventory case ID: {}", row.case_id);
        }
        if !orders.insert((row.language.as_str(), row.selection_order)) {
            bail!("duplicate selection order for language {}", row.language);
        }
        if matches!(row.decision.as_str(), "balanced_core" | "overflow") {
            *eligible.entry(&row.language).or_default() += 1;
        }
        if row.decision == "balanced_core" {
            *core.entry(&row.language).or_default() += 1;
        }
    }
    for (language, expected) in &cohort.eligible_counts {
        if eligible.get(language.as_str()).copied().unwrap_or(0) != *expected {
            bail!("eligible count drift for {language}");
        }
        if core.get(language.as_str()).copied().unwrap_or(0) != expected_n {
            bail!("balanced core must contain N={expected_n} cases for {language}");
        }
    }
    Ok(())
}

fn candidate(
    document: &BenchmarkDocument,
    case: &BenchmarkCase,
    relative: &str,
    source_uri: &str,
    project_load_evidence: &str,
) -> Result<Candidate> {
    let control = if case.unsupported.is_some() {
        Some(("unsupported", "control_unsupported"))
    } else if case.not_planned.is_some() {
        Some(("not_planned", "control_not_planned"))
    } else {
        None
    };
    let mut queries = Vec::new();
    if let Some(declaration) = &case.declaration {
        queries.push(query(
            "references",
            control.map_or("canonical", |v| v.0),
            &declaration.location,
        ));
    }
    for lookup in &case.usage_lookups {
        queries.push(query(
            lookup.operation.as_str(),
            control.map_or("canonical", |v| v.0),
            &lookup.usage.location,
        ));
        for compatible in &lookup.compatible_operations {
            queries.push(query(
                compatible.as_str(),
                control.map_or("compatible", |v| v.0),
                &lookup.usage.location,
            ));
        }
    }
    for lookup in &case.type_lookups {
        queries.push(query(
            "type_definition",
            control.map_or("canonical", |value| value.0),
            &lookup.expression.location,
        ));
    }
    if queries.is_empty() {
        bail!("legacy case {} has no source query", case.id);
    }
    let symbol_kind = case
        .declaration
        .as_ref()
        .or_else(|| {
            case.usage_lookups
                .first()
                .map(|lookup| &lookup.expected_declaration)
        })
        .or_else(|| {
            case.type_lookups
                .first()
                .map(|lookup| &lookup.expected_type)
        })
        .map(|location| serde_json::to_value(&location.kind))
        .transpose()?
        .and_then(|value| value.as_str().map(str::to_owned))
        .context("legacy case has no symbol kind")?;
    let semantic_feature = semantic_feature(&case.id);
    let source_complexity = source_complexity(relative);
    let operation_tags = queries
        .iter()
        .map(|query| format!("{}:{}", query.operation, query.status))
        .collect::<BTreeSet<_>>();
    let counts = LocationCounts {
        required: case.expected_usages.len() + case.usage_lookups.len() + case.type_lookups.len(),
        optional: case.allowed_extra_usages.len()
            + case.allowed_unproven_usages.len()
            + case
                .usage_lookups
                .iter()
                .map(|lookup| lookup.allowed_extra_targets.len())
                .sum::<usize>(),
        unproven: case.expected_unproven_usages.len(),
        // The benchmark model has no positive `excludedLocations` collection.
        excluded: 0,
    };
    let duplicate_material = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        document.language,
        symbol_kind,
        semantic_feature,
        source_complexity,
        operation_tags.iter().cloned().collect::<Vec<_>>().join(","),
        counts.required,
        counts.optional,
        counts.unproven
    );
    let verification = case
        .verification
        .as_ref()
        .context("legacy case lacks first-review metadata")?;
    let method = serde_json::to_value(&verification.method)?
        .as_str()
        .unwrap()
        .to_owned();
    Ok(Candidate {
        row: InventoryRow {
            language: document.language.clone(),
            document: relative.into(),
            case_id: case.id.clone(),
            source_uri: source_uri.into(),
            queries,
            symbol_kind,
            semantic_feature,
            source_complexity,
            location_counts: counts,
            fixture_determinism: "checked_in_fixture".into(),
            project_load_evidence: project_load_evidence.into(),
            exact_range_validity: "validated_against_fixture".into(),
            duplication_group: format!("dup-{}", &sha256(duplicate_material.as_bytes())[..16]),
            first_review_provenance: ReviewProvenance {
                method,
                notes_sha256: sha256(verification.notes.as_bytes()),
            },
            decision: control.map_or_else(|| "overflow".into(), |v| v.1.into()),
            selection_order: 1,
        },
        operation_tags,
    })
}

fn diversity_order(mut remaining: Vec<Candidate>) -> Vec<Candidate> {
    let mut ordered = Vec::with_capacity(remaining.len());
    let mut operations = BTreeSet::new();
    let mut families = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    while !remaining.is_empty() {
        remaining.sort_by(|a, b| {
            let score = |candidate: &Candidate| {
                let new_operations = candidate
                    .operation_tags
                    .iter()
                    .filter(|tag| !operations.contains(*tag))
                    .count();
                (
                    new_operations,
                    usize::from(!families.contains(&candidate.row.semantic_feature)),
                    usize::from(!kinds.contains(&candidate.row.symbol_kind)),
                    usize::from(!duplicates.contains(&candidate.row.duplication_group)),
                )
            };
            score(b)
                .cmp(&score(a))
                .then_with(|| row_key(&a.row).cmp(&row_key(&b.row)))
        });
        let selected = remaining.remove(0);
        operations.extend(selected.operation_tags.iter().cloned());
        families.insert(selected.row.semantic_feature.clone());
        kinds.insert(selected.row.symbol_kind.clone());
        duplicates.insert(selected.row.duplication_group.clone());
        ordered.push(selected);
    }
    ordered
}

fn query(operation: &str, status: &str, location: &Location) -> QueryInventory {
    QueryInventory {
        operation: operation.into(),
        status: status.into(),
        uri: location.uri.to_string(),
        range: location.range.clone(),
    }
}

fn semantic_feature(id: &str) -> String {
    let rules = [
        (
            ["macro", "generated", "source-generator"].as_slice(),
            "generated_or_macro",
        ),
        (
            ["import", "alias", "reexport", "barrel"].as_slice(),
            "imports_and_aliases",
        ),
        (
            ["override", "inherit", "interface", "trait", "mixin"].as_slice(),
            "inheritance_and_dispatch",
        ),
        (
            ["field", "property", "constant", "variable", "val-", "attr"].as_slice(),
            "state_and_properties",
        ),
        (
            ["constructor", "factory", "new-", "case-class"].as_slice(),
            "construction",
        ),
        (
            ["module", "package", "namespace"].as_slice(),
            "modules_and_namespaces",
        ),
        (
            ["dynamic", "getattr", "public-send", "computed"].as_slice(),
            "dynamic_indirection",
        ),
        (
            ["method", "call", "function", "extension"].as_slice(),
            "callables",
        ),
        (
            ["class", "type", "record", "struct", "enum"].as_slice(),
            "nominal_types",
        ),
    ];
    rules
        .iter()
        .find_map(|(needles, family)| {
            needles
                .iter()
                .any(|needle| id.contains(needle))
                .then_some(*family)
        })
        .unwrap_or("other_source_semantics")
        .into()
}

fn source_complexity(file: &str) -> String {
    if file.contains("source-generator") {
        "generated_project"
    } else if file.contains("lsp-parity") {
        "adapted_parity_fixture"
    } else if file.contains("precision") || file.contains("sibling-associated") {
        "focused_edge_fixture"
    } else {
        "minimal_baseline_fixture"
    }
    .into()
}

fn project_load_evidence(root: &Path, source: &Source) -> Result<String> {
    let Source::Fixture { path } = source else {
        return Ok("source_only_not_executed".into());
    };
    let mut stack = vec![root.join(path)];
    let project_files = [
        "Cargo.toml",
        "go.mod",
        "pom.xml",
        "build.gradle",
        "build.gradle.kts",
        "package.json",
        "tsconfig.json",
        "composer.json",
        "Gemfile",
        "build.sbt",
        "CMakeLists.txt",
    ];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                stack.push(entry.path());
            } else if project_files.contains(&entry.file_name().to_string_lossy().as_ref()) {
                return Ok("project_file_present_not_executed".into());
            }
        }
    }
    Ok("source_only_not_executed".into())
}

fn is_eligible(row: &InventoryRow) -> bool {
    !row.decision.starts_with("control_")
}

fn row_key(row: &InventoryRow) -> (&str, &str) {
    (&row.document, &row.case_id)
}

fn collect_yaml(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "yaml") {
            files.push(path);
        }
    }
    Ok(())
}

fn validate_link(root: &Path, link: &ArtifactLink, label: &str) -> Result<()> {
    let bytes = fs::read(safe_join(root, &link.file)?)
        .with_context(|| format!("read {label} {}", link.file))?;
    if sha256(&bytes) != link.sha256 {
        bail!("{label} hash mismatch for {}", link.file);
    }
    Ok(())
}

fn safe_join(root: &Path, file: &str) -> Result<PathBuf> {
    let path = root.join(safe_repo_relative_path(file, "cohort artifact")?);
    let canonical_root = root
        .canonicalize()
        .context("canonicalize repository root")?;
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("canonicalize cohort artifact {file}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!("cohort artifact resolves outside the repository: {file}");
    }
    Ok(canonical_path)
}

fn repo_relative(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .canonicalize()?
        .strip_prefix(root.canonicalize()?)?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_words_do_not_affect_semantic_family() {
        assert_eq!(semantic_feature("java-method-call"), "callables");
        assert_eq!(semantic_feature("java-method-call-failure"), "callables");
    }

    #[test]
    fn generated_cohort_is_complete_and_balanced() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("cohort.json");
        let cohort = generate(Path::new("benchmarks/cases"), &output).unwrap();
        assert_eq!(cohort.inventory.len(), 158);
        assert_eq!(cohort.eligible_counts.len(), 11);
        assert_eq!(cohort.balanced_core_per_language, 10);
        assert_eq!(
            cohort
                .inventory
                .iter()
                .filter(|row| row.decision == "balanced_core")
                .count(),
            110
        );
    }

    #[test]
    fn checked_in_cohort_matches_deterministic_regeneration() {
        validate(Path::new("benchmarks/promotion/legacy-v1/cohort.json")).unwrap();
    }

    #[test]
    fn rejects_a_post_freeze_membership_edit() {
        let source = fs::read("benchmarks/promotion/legacy-v1/cohort.json").unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&source).unwrap();
        value["inventory"][0]["decision"] = serde_json::json!("overflow");
        value["inventory"][10]["decision"] = serde_json::json!("balanced_core");
        let temporary = tempfile::Builder::new()
            .prefix("cohort-tamper-")
            .tempdir_in(".")
            .unwrap();
        let path = temporary.path().join("cohort.json");
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let error = validate(&path).unwrap_err();
        assert!(error
            .to_string()
            .contains("deterministic source-only regeneration"));
    }
}
