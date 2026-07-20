use super::RunReport;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportDifference {
    pub path: String,
    pub expected: String,
    pub actual: String,
}

pub fn compare_report_files(expected: &Path, actual: &Path) -> Result<Vec<ReportDifference>> {
    let expected = read_report_value(expected)?;
    let actual = read_report_value(actual)?;
    Ok(compare_values(expected, actual))
}

pub fn read_report(path: &Path) -> Result<RunReport> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("read benchmark report {}", path.display()))?,
    )
    .with_context(|| format!("parse benchmark report {}", path.display()))
}

fn read_report_value(path: &Path) -> Result<Value> {
    let bytes =
        fs::read(path).with_context(|| format!("read benchmark report {}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse benchmark report {}", path.display()))?;
    serde_json::from_value::<RunReport>(value.clone())
        .with_context(|| format!("validate benchmark report {}", path.display()))?;
    Ok(value)
}

pub fn compare_reports(expected: &RunReport, actual: &RunReport) -> Vec<ReportDifference> {
    compare_values(
        serde_json::to_value(expected).expect("RunReport serialization cannot fail"),
        serde_json::to_value(actual).expect("RunReport serialization cannot fail"),
    )
}

fn compare_values(expected: Value, actual: Value) -> Vec<ReportDifference> {
    let expected = semantic_value(expected);
    let actual = semantic_value(actual);
    let mut differences = Vec::new();
    collect_differences("$", &expected, &actual, &mut differences);
    differences
}

fn semantic_value(mut value: Value) -> Value {
    let source_roots = value
        .get("documents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|document| document.get("sourceRoot").and_then(Value::as_str))
        .filter(|root| !root.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    replace_source_roots(&mut value, &source_roots);

    let Value::Object(root) = &mut value else {
        unreachable!("RunReport serializes as an object");
    };
    root.remove("startedAtUnixSeconds");
    root.remove("finishedAtUnixSeconds");
    root.remove("bifrostRepo");
    root.remove("bifrostCommit");

    if let Some(Value::Object(runner)) = root.get_mut("runner") {
        if runner
            .get("source")
            .and_then(Value::as_str)
            .is_some_and(is_local_path)
        {
            runner.insert(
                "source".to_string(),
                Value::String("<runner-source>".to_string()),
            );
        }
        key_array(runner, "capabilities", "operation");
    }

    if let Some(Value::Object(environment)) = root.get_mut("environment") {
        if let Some(Value::Object(container)) = environment.get_mut("container") {
            container.insert(
                "imageDigest".to_string(),
                Value::String("<locally-built-image>".to_string()),
            );
        }
        if let Some(Value::Object(executable)) = environment.get_mut("analyzerExecutable") {
            executable.remove("resolvedPath");
        }
    }

    if let Some(Value::Array(documents)) = root.remove("documents") {
        root.insert("documents".to_string(), keyed_documents(documents));
    }

    sort_arrays(&mut value);
    value
}

fn keyed_documents(documents: Vec<Value>) -> Value {
    let mut grouped = BTreeMap::<String, Vec<Value>>::new();
    for mut document in documents {
        let Some(object) = document.as_object_mut() else {
            continue;
        };
        let key = object
            .get("caseFile")
            .and_then(Value::as_str)
            .unwrap_or("<unknown-document>")
            .to_string();
        object.insert(
            "sourceRoot".to_string(),
            Value::String("<source-root>".to_string()),
        );
        key_array(object, "cases", "id");
        grouped.entry(key).or_default().push(document);
    }
    grouped_value(grouped)
}

fn key_array(object: &mut Map<String, Value>, field: &str, key_field: &str) {
    let Some(Value::Array(items)) = object.remove(field) else {
        return;
    };
    let mut grouped = BTreeMap::<String, Vec<Value>>::new();
    for item in items {
        let key = item
            .get(key_field)
            .and_then(Value::as_str)
            .unwrap_or("<unknown>")
            .to_string();
        grouped.entry(key).or_default().push(item);
    }
    object.insert(field.to_string(), grouped_value(grouped));
}

fn grouped_value(grouped: BTreeMap<String, Vec<Value>>) -> Value {
    Value::Object(
        grouped
            .into_iter()
            .map(|(key, mut values)| {
                if values.len() == 1 {
                    (key, values.pop().expect("one grouped value"))
                } else {
                    values.sort_by_key(|value| serde_json::to_string(value).unwrap_or_default());
                    (key, Value::Array(values))
                }
            })
            .collect(),
    )
}

fn replace_source_roots(value: &mut Value, roots: &[String]) {
    match value {
        Value::String(text) => {
            for root in roots {
                *text = text.replace(root, "<source-root>");
            }
        }
        Value::Array(items) => {
            for item in items {
                replace_source_roots(item, roots);
            }
        }
        Value::Object(object) => {
            for item in object.values_mut() {
                replace_source_roots(item, roots);
            }
        }
        _ => {}
    }
}

fn sort_arrays(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items.iter_mut() {
                sort_arrays(item);
            }
            items.sort_by_key(|item| serde_json::to_string(item).unwrap_or_default());
        }
        Value::Object(object) => {
            for item in object.values_mut() {
                sort_arrays(item);
            }
        }
        _ => {}
    }
}

fn collect_differences(
    path: &str,
    expected: &Value,
    actual: &Value,
    differences: &mut Vec<ReportDifference>,
) {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            let keys = expected
                .keys()
                .chain(actual.keys())
                .collect::<std::collections::BTreeSet<_>>();
            for key in keys {
                let child_path = format!("{path}.{}", display_path_segment(key));
                match (expected.get(key), actual.get(key)) {
                    (Some(expected), Some(actual)) => {
                        collect_differences(&child_path, expected, actual, differences)
                    }
                    (Some(expected), None) => differences.push(ReportDifference {
                        path: child_path,
                        expected: compact(expected),
                        actual: "<missing>".to_string(),
                    }),
                    (None, Some(actual)) => differences.push(ReportDifference {
                        path: child_path,
                        expected: "<missing>".to_string(),
                        actual: compact(actual),
                    }),
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(expected), Value::Array(actual)) => {
            let length = expected.len().max(actual.len());
            for index in 0..length {
                let child_path = format!("{path}[{index}]");
                match (expected.get(index), actual.get(index)) {
                    (Some(expected), Some(actual)) => {
                        collect_differences(&child_path, expected, actual, differences)
                    }
                    (Some(expected), None) => differences.push(ReportDifference {
                        path: child_path,
                        expected: compact(expected),
                        actual: "<missing>".to_string(),
                    }),
                    (None, Some(actual)) => differences.push(ReportDifference {
                        path: child_path,
                        expected: "<missing>".to_string(),
                        actual: compact(actual),
                    }),
                    (None, None) => unreachable!(),
                }
            }
        }
        _ if expected != actual => differences.push(ReportDifference {
            path: path.to_string(),
            expected: compact(expected),
            actual: compact(actual),
        }),
        _ => {}
    }
}

fn display_path_segment(segment: &str) -> String {
    if segment
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        segment.to_string()
    } else {
        format!("[{}]", serde_json::to_string(segment).unwrap_or_default())
    }
}

fn compact(value: &Value) -> String {
    const LIMIT: usize = 240;
    let text = serde_json::to_string(value).unwrap_or_else(|_| "<unprintable>".to_string());
    if text.chars().count() <= LIMIT {
        return text;
    }
    format!("{}...", text.chars().take(LIMIT).collect::<String>())
}

fn is_local_path(value: &str) -> bool {
    value.starts_with('/') || value.starts_with("./") || value.starts_with("../")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CorpusPartition, CorpusSelection, GroundTruthReviewStatus, ReferencePolicy};
    use std::collections::BTreeMap;

    #[test]
    fn ignores_only_documented_volatile_fields() {
        let expected = report();
        let mut actual = expected.clone();
        actual.started_at_unix_seconds = 99;
        actual.finished_at_unix_seconds = 100;
        actual.documents[0].source_root = "/another/run/source".to_string();
        actual.environment.container.as_mut().unwrap().image_digest =
            format!("sha256:{}", "d".repeat(64));
        actual.environment.analyzer_executable.resolved_path =
            Some("/another/bin/bifrost".to_string());

        assert!(compare_reports(&expected, &actual).is_empty());
    }

    #[test]
    fn reports_case_level_semantic_differences() {
        let expected = report();
        let mut actual = expected.clone();
        actual.documents[0].cases[0].status = super::super::CaseStatus::Failed;

        let differences = compare_reports(&expected, &actual);

        assert_eq!(differences.len(), 1);
        assert!(differences[0].path.contains("sample-case"));
        assert!(differences[0].path.ends_with("status"));
        assert_eq!(differences[0].expected, "\"passed\"");
        assert_eq!(differences[0].actual, "\"failed\"");
    }

    #[test]
    fn compares_requested_analyzer_version() {
        let expected = report();
        let mut actual = expected.clone();
        actual.runner.requested_version = "another-version".to_string();

        let differences = compare_reports(&expected, &actual);

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].path, "$.runner.requestedVersion");
    }

    #[test]
    fn preserves_duplicate_case_ids_during_comparison() {
        let expected = report();
        let mut actual = expected.clone();
        let mut duplicate = actual.documents[0].cases[0].clone();
        duplicate.status = super::super::CaseStatus::Failed;
        actual.documents[0].cases.push(duplicate);

        let differences = compare_reports(&expected, &actual);

        assert!(differences
            .iter()
            .any(|difference| difference.path.contains("sample-case")));
    }

    #[test]
    fn file_comparison_preserves_unknown_semantic_fields() {
        let tempdir = tempfile::tempdir().unwrap();
        let expected_path = tempdir.path().join("expected.json");
        let actual_path = tempdir.path().join("actual.json");
        let expected = serde_json::to_value(report()).unwrap();
        let mut actual = expected.clone();
        actual
            .as_object_mut()
            .unwrap()
            .insert("futureSemanticField".to_string(), Value::Bool(true));
        fs::write(&expected_path, serde_json::to_vec(&expected).unwrap()).unwrap();
        fs::write(&actual_path, serde_json::to_vec(&actual).unwrap()).unwrap();

        let differences = compare_report_files(&expected_path, &actual_path).unwrap();

        assert_eq!(differences.len(), 1);
        assert_eq!(differences[0].path, "$.futureSemanticField");
    }

    fn report() -> RunReport {
        RunReport {
            usagebench_version: "0.1.0".to_string(),
            usagebench_revision: "a".repeat(40),
            usagebench_release: Some("v0.1.0".to_string()),
            runner: super::super::RunnerMetadata {
                name: "bifrost".to_string(),
                requested_version: "origin/master".to_string(),
                resolved_version: "b".repeat(40),
                source: "/checkout/bifrost".to_string(),
                adapter_version: "0.1.0".to_string(),
                capabilities: Vec::new(),
            },
            invocation: super::super::RunInvocation {
                include_unsupported: false,
                include_definition_lookups: true,
                profile: None,
                case_id: None,
            },
            environment: super::super::ExecutionEnvironment {
                operating_system: "linux".to_string(),
                architecture: "x86_64".to_string(),
                execution_mode: super::super::ExecutionMode::Container,
                platform_scope: super::super::PlatformScope::CanonicalReference,
                reference_environment: Some(super::super::ReferenceEnvironmentProvenance {
                    version: "1".to_string(),
                    definition_digest: format!("sha256:{}", "c".repeat(64)),
                    canonical_platform: "linux/amd64".to_string(),
                }),
                container: Some(super::super::ContainerProvenance {
                    image_reference: "usagebench-reference:v0.1.0-env1-bifrost".to_string(),
                    image_digest: format!("sha256:{}", "d".repeat(64)),
                }),
                analyzer_executable: super::super::ExecutableProvenance {
                    command: "bifrost".to_string(),
                    resolved_path: Some("/usr/local/bin/bifrost".to_string()),
                    sha256: Some("e".repeat(64)),
                },
                toolchains: BTreeMap::new(),
            },
            bifrost_repo: Some("/checkout/bifrost".to_string()),
            bifrost_commit: Some("origin/master".to_string()),
            bifrost_resolved_commit: Some("b".repeat(40)),
            started_at_unix_seconds: 1,
            finished_at_unix_seconds: 2,
            case_files: vec!["benchmarks/cases/rust-baseline.yaml".to_string()],
            totals: super::super::RunTotals {
                documents: 1,
                cases: 1,
                development_cases: 1,
                passed: 1,
                ..Default::default()
            },
            documents: vec![super::super::DocumentRunReport {
                case_file: "benchmarks/cases/rust-baseline.yaml".to_string(),
                language: "rust".to_string(),
                source_root: "/work/run-1/source".to_string(),
                corpus_partition: CorpusPartition::Development,
                corpus_selection: CorpusSelection::AnalyzerInformed,
                ground_truth_status: GroundTruthReviewStatus::LegacyUnattributed,
                reference_policy: ReferencePolicy::BindingsOptional,
                cases: vec![super::super::CaseRunReport {
                    id: "sample-case".to_string(),
                    status: super::super::CaseStatus::Passed,
                    expected_failure_reason: None,
                    not_planned_reason: None,
                    unsupported_reason: None,
                    declaration_to_usages: None,
                    usage_to_declaration: Vec::new(),
                    type_lookups: Vec::new(),
                    diagnostics: Vec::new(),
                }],
            }],
        }
    }
}
