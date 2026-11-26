//! Merge reports from disjoint shards of one analyzer run into a single report.
//!
//! CI splits a long corpus run into parallel shards (for example one
//! `run-bifrost --language` invocation per language). Each shard writes an
//! ordinary [`RunReport`]. This module combines them into one report covering
//! the requested corpus, so existing summaries and completeness gates keep
//! working. Shards must share the same analyzer build, UsageBench revision,
//! invocation and environment; anything else would mix evidence.

use super::{compute_totals, report_case_path, requested_run_totals, RunReport, RunTimings};
use crate::BenchmarkDocument;
use anyhow::{bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

/// Merge shard reports into one report scoped to every case file under
/// `case_path`.
///
/// The merged report is complete only when every shard is complete and the
/// shards together cover every requested case file. Case files that no shard
/// requested (for example a shard that never wrote a report) stay in
/// `requestedCaseFiles` and `requestedTotals`, so the gap shows up as an
/// incomplete report rather than a silently smaller corpus.
pub fn merge_run_reports(case_path: &Path, shards: Vec<RunReport>) -> Result<RunReport> {
    let repo_root = crate::find_repo_root_for_path(case_path)?;
    // The corpus is validated before the shards run; listing it is enough
    // here and avoids re-materializing evaluation evidence.
    let case_files = crate::list_case_files(case_path)?;
    let requested_case_files = case_files
        .iter()
        .map(|path| report_case_path(path, &repo_root))
        .collect::<Vec<_>>();
    let order = requested_case_files
        .iter()
        .enumerate()
        .map(|(index, path)| (path.clone(), index))
        .collect::<BTreeMap<_, _>>();

    let mut shards = shards.into_iter();
    let Some(first) = shards.next() else {
        bail!("merge-reports needs at least one shard report");
    };
    let mut merged = first.clone();
    merged.requested_case_files = requested_case_files.clone();

    let mut all = vec![first];
    for shard in shards {
        ensure_same_run(&merged, &shard)?;
        all.push(shard);
    }

    let mut shard_requested = BTreeSet::new();
    let mut documents = Vec::new();
    let mut requested_totals = super::RequestedRunTotals::default();
    let mut timings = RunTimings {
        build_cache_hit: true,
        provenance_cache_hit: true,
        ..RunTimings::default()
    };
    let mut semantic_pack_runs = Vec::new();
    let mut completed = true;
    let mut started_at = u64::MAX;
    let mut finished_at = 0;
    for (index, shard) in all.into_iter().enumerate() {
        for case_file in &shard.requested_case_files {
            if !order.contains_key(case_file) {
                bail!(
                    "shard {index} requested {case_file}, which is not under the merged case path"
                );
            }
            if !shard_requested.insert(case_file.clone()) {
                bail!("shard {index} requested {case_file}, which another shard already covers");
            }
        }
        for document in &shard.documents {
            if !shard.requested_case_files.contains(&document.case_file) {
                bail!(
                    "shard {index} reports {} outside its requested case files",
                    document.case_file
                );
            }
        }
        completed &= shard.completed && shard.ensure_complete().is_ok();
        add_requested_totals(&mut requested_totals, &shard.requested_totals);
        add_timings(&mut timings, &shard.timings);
        started_at = started_at.min(shard.started_at_unix_seconds);
        finished_at = finished_at.max(shard.finished_at_unix_seconds);
        semantic_pack_runs.extend(shard.semantic_pack_runs);
        documents.extend(shard.documents);
    }

    let missing = case_files
        .iter()
        .zip(&requested_case_files)
        .filter(|(_, relative)| !shard_requested.contains(*relative))
        .map(|(path, _)| {
            let yaml = fs::read_to_string(path)
                .with_context(|| format!("read benchmark cases {}", path.display()))?;
            serde_yaml::from_str::<BenchmarkDocument>(&yaml)
                .with_context(|| format!("deserialize benchmark cases {}", path.display()))
        })
        .collect::<Result<Vec<_>>>()?;
    if !missing.is_empty() {
        completed = false;
        add_requested_totals(
            &mut requested_totals,
            &requested_run_totals(
                &missing,
                merged.invocation.include_unsupported,
                merged.invocation.case_id.as_deref(),
            ),
        );
    }

    documents.sort_by_key(|document| order[&document.case_file]);
    timings.refresh_total();
    merged.case_files = documents
        .iter()
        .map(|document| document.case_file.clone())
        .collect();
    merged.totals = compute_totals(&documents);
    merged.documents = documents;
    merged.requested_totals = requested_totals;
    merged.timings = timings;
    merged.semantic_pack_runs = semantic_pack_runs;
    merged.started_at_unix_seconds = started_at;
    merged.finished_at_unix_seconds = finished_at;
    merged.completed = completed && merged.ensure_complete().is_ok();
    Ok(merged)
}

/// Read shard report files and write the merged report to `output`.
pub fn merge_report_files(case_path: &Path, shards: &[&Path], output: &Path) -> Result<RunReport> {
    let reports = shards
        .iter()
        .map(|path| super::report_compare::read_report(path))
        .collect::<Result<Vec<_>>>()?;
    let merged = merge_run_reports(case_path, reports)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(&merged)?;
    bytes.push(b'\n');
    fs::write(output, bytes)
        .with_context(|| format!("write merged report {}", output.display()))?;
    Ok(merged)
}

fn ensure_same_run(expected: &RunReport, actual: &RunReport) -> Result<()> {
    let checks: [(&str, bool); 8] = [
        (
            "usagebenchVersion",
            expected.usagebench_version == actual.usagebench_version,
        ),
        (
            "usagebenchRevision",
            expected.usagebench_revision == actual.usagebench_revision,
        ),
        (
            "usagebenchRelease",
            expected.usagebench_release == actual.usagebench_release,
        ),
        ("runner", expected.runner == actual.runner),
        ("invocation", expected.invocation == actual.invocation),
        ("environment", expected.environment == actual.environment),
        ("bifrostRepo", expected.bifrost_repo == actual.bifrost_repo),
        (
            "bifrostResolvedCommit",
            expected.bifrost_commit == actual.bifrost_commit
                && expected.bifrost_resolved_commit == actual.bifrost_resolved_commit,
        ),
    ];
    if let Some((field, _)) = checks.iter().find(|(_, same)| !same) {
        bail!("shard reports disagree on {field}; they are not shards of one run");
    }
    Ok(())
}

fn add_requested_totals(totals: &mut super::RequestedRunTotals, shard: &super::RequestedRunTotals) {
    totals.documents += shard.documents;
    totals.authored_cases += shard.authored_cases;
    totals.planned_cases += shard.planned_cases;
    totals.development_planned_cases += shard.development_planned_cases;
    totals.evaluation_planned_cases += shard.evaluation_planned_cases;
}

/// Phase timings add up across shards, so the merged values are total
/// machine time spent in each phase, not wall-clock time.
fn add_timings(totals: &mut RunTimings, shard: &RunTimings) {
    totals.checkout_setup_millis = totals
        .checkout_setup_millis
        .saturating_add(shard.checkout_setup_millis);
    totals.build_millis = totals.build_millis.saturating_add(shard.build_millis);
    totals.provenance_hashing_millis = totals
        .provenance_hashing_millis
        .saturating_add(shard.provenance_hashing_millis);
    totals.workspace_readiness_millis = totals
        .workspace_readiness_millis
        .saturating_add(shard.workspace_readiness_millis);
    totals.analyzer_query_millis = totals
        .analyzer_query_millis
        .saturating_add(shard.analyzer_query_millis);
    totals.build_cache_hit &= shard.build_cache_hit;
    totals.provenance_cache_hit &= shard.provenance_cache_hit;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::{
        runner_failure_case, CaseStatus, DocumentRunReport, ExecutableProvenance,
        ExecutionEnvironment, ExecutionMode, PlatformScope, RunInvocation, RunnerMetadata,
    };
    use std::path::PathBuf;

    const LANGUAGES: [&str; 2] = ["alpha", "beta"];

    fn corpus() -> (tempfile::TempDir, PathBuf) {
        let tempdir = tempfile::tempdir().unwrap();
        let root = tempdir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        fs::create_dir(root.join("schema")).unwrap();
        let fixture = root.join("fixtures/fixture/src");
        fs::create_dir_all(&fixture).unwrap();
        fs::write(fixture.join("sample.txt"), "let value = 1\n").unwrap();
        let cases = root.join("benchmarks/cases");
        fs::create_dir_all(&cases).unwrap();
        for language in LANGUAGES {
            fs::write(
                cases.join(format!("{language}.yaml")),
                format!(
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
language: {language}
cases:
  - id: {language}-value
    declaration:
      location:
        uri: benchmark://source/src/sample.txt
        range:
          start: {{ line: 0, character: 4 }}
          end: {{ line: 0, character: 9 }}
      kind: variable
      displayName: value
    expectedUsages: []
    usageLookups: []
"#
                ),
            )
            .unwrap();
        }
        (tempdir, cases)
    }

    fn shard(cases: &Path, language: &str, started_at: u64) -> RunReport {
        let path = cases.join(format!("{language}.yaml"));
        let document: BenchmarkDocument =
            serde_yaml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let case_file = format!("benchmarks/cases/{language}.yaml");
        let mut case = runner_failure_case(&document.cases[0], false, "unused", "unused");
        case.status = CaseStatus::Passed;
        case.required_destination_status = None;
        let documents = vec![DocumentRunReport {
            case_file: case_file.clone(),
            language: language.to_string(),
            source_root: "/work/source".to_string(),
            corpus_partition: document.corpus.partition,
            corpus_selection: document.corpus.selection,
            ground_truth_status: document.ground_truth.status,
            reference_policy: document.reference_policy,
            cases: vec![case],
        }];
        RunReport {
            usagebench_version: "0.3.1".to_string(),
            usagebench_revision: "a".repeat(40),
            usagebench_release: None,
            runner: RunnerMetadata {
                name: "bifrost".to_string(),
                requested_version: "abc123".to_string(),
                resolved_version: "abc123".to_string(),
                source: ".bifrost".to_string(),
                adapter_version: "0.1.0".to_string(),
                capabilities: Vec::new(),
            },
            invocation: RunInvocation {
                include_unsupported: false,
                include_definition_lookups: false,
                scan_usages_max_duration_secs: Some(300),
                profile: None,
                profile_sha256: None,
                case_id: None,
            },
            timings: RunTimings {
                analyzer_query_millis: 10,
                provenance_cache_hit: false,
                ..RunTimings::default()
            },
            completed: true,
            requested_case_files: vec![case_file.clone()],
            requested_totals: requested_run_totals([&document], false, None),
            semantic_pack_runs: Vec::new(),
            environment: ExecutionEnvironment {
                operating_system: "linux".to_string(),
                architecture: "x86_64".to_string(),
                execution_mode: ExecutionMode::Container,
                platform_scope: PlatformScope::CanonicalReference,
                reference_environment: None,
                container: None,
                analyzer_executable: ExecutableProvenance {
                    command: "/bin/bifrost".to_string(),
                    resolved_path: Some("/bin/bifrost".to_string()),
                    sha256: Some("c".repeat(64)),
                },
                toolchains: BTreeMap::new(),
            },
            bifrost_repo: Some(".bifrost".to_string()),
            bifrost_commit: Some("abc123".to_string()),
            bifrost_resolved_commit: Some("abc123".to_string()),
            started_at_unix_seconds: started_at,
            finished_at_unix_seconds: started_at + 5,
            case_files: vec![case_file],
            totals: compute_totals(&documents),
            documents,
        }
    }

    #[test]
    fn merges_disjoint_shards_into_one_complete_report() {
        let (_tempdir, cases) = corpus();

        // Shards arrive out of corpus order; the merge restores it.
        let merged = merge_run_reports(
            &cases,
            vec![shard(&cases, "beta", 20), shard(&cases, "alpha", 10)],
        )
        .unwrap();

        assert!(merged.completed);
        assert_eq!(
            merged.case_files,
            vec![
                "benchmarks/cases/alpha.yaml".to_string(),
                "benchmarks/cases/beta.yaml".to_string()
            ]
        );
        assert_eq!(merged.requested_case_files, merged.case_files);
        assert_eq!(merged.requested_totals.documents, 2);
        assert_eq!(merged.requested_totals.planned_cases, 2);
        assert_eq!(merged.totals.documents, 2);
        assert_eq!(merged.totals.passed, 2);
        assert_eq!(merged.timings.analyzer_query_millis, 20);
        assert!(!merged.timings.provenance_cache_hit);
        assert_eq!(merged.started_at_unix_seconds, 10);
        assert_eq!(merged.finished_at_unix_seconds, 25);
        merged.ensure_complete().unwrap();
    }

    #[test]
    fn missing_shard_keeps_its_documents_in_the_requested_scope() {
        let (_tempdir, cases) = corpus();

        let merged = merge_run_reports(&cases, vec![shard(&cases, "alpha", 10)]).unwrap();

        assert!(!merged.completed);
        assert_eq!(merged.requested_case_files.len(), 2);
        assert_eq!(merged.case_files, vec!["benchmarks/cases/alpha.yaml"]);
        assert_eq!(merged.requested_totals.documents, 2);
        assert_eq!(merged.requested_totals.planned_cases, 2);
        assert_eq!(merged.totals.documents, 1);
    }

    #[test]
    fn incomplete_shard_checkpoint_makes_the_merge_incomplete() {
        let (_tempdir, cases) = corpus();
        let mut partial = shard(&cases, "beta", 20);
        partial.completed = false;
        partial.case_files.clear();
        partial.documents.clear();
        partial.totals = compute_totals(&[]);

        let merged = merge_run_reports(&cases, vec![shard(&cases, "alpha", 10), partial]).unwrap();

        assert!(!merged.completed);
        assert_eq!(merged.requested_totals.documents, 2);
        assert_eq!(merged.totals.documents, 1);
    }

    #[test]
    fn rejects_overlapping_shards() {
        let (_tempdir, cases) = corpus();

        let error = merge_run_reports(
            &cases,
            vec![shard(&cases, "alpha", 10), shard(&cases, "alpha", 20)],
        )
        .unwrap_err();

        assert!(error.to_string().contains("another shard already covers"));
    }

    #[test]
    fn rejects_shards_from_different_analyzer_builds() {
        let (_tempdir, cases) = corpus();
        let mut other = shard(&cases, "beta", 20);
        other.bifrost_resolved_commit = Some("def456".to_string());

        let error = merge_run_reports(&cases, vec![shard(&cases, "alpha", 10), other]).unwrap_err();

        assert!(error.to_string().contains("bifrostResolvedCommit"));
    }
}
