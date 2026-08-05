use serde_json::json;
use std::{fs, process::Command};

fn render(report: serde_json::Value) -> String {
    let tempdir = tempfile::tempdir().unwrap();
    let report_path = tempdir.path().join("report.json");
    fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();

    let output = Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/render-benchmark-summary.sh"
        ))
        .arg(report_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn render_counts(report: serde_json::Value) -> serde_json::Value {
    let tempdir = tempfile::tempdir().unwrap();
    let report_path = tempdir.path().join("report.json");
    fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
    let output = Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/render-benchmark-summary.sh"
        ))
        .arg("--counts")
        .arg(report_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn incomplete_summary_distinguishes_processed_and_requested_scope() {
    let summary = render(json!({
        "completed": false,
        "requestedCaseFiles": (0..49).map(|index| format!("case-{index}.yaml")).collect::<Vec<_>>(),
        "requestedTotals": {
            "documents": 49,
            "authoredCases": 196,
            "plannedCases": 190,
            "developmentPlannedCases": 154,
            "evaluationPlannedCases": 36
        },
        "totals": {
            "documents": 25,
            "cases": 89,
            "developmentCases": 89,
            "evaluationCases": 0,
            "passed": 88,
            "errors": 1
        },
        "documents": []
    }));

    assert!(summary.contains("## INCOMPLETE - Processed Totals"));
    assert!(summary.contains("Documents processed/requested: 25/49"));
    assert!(summary.contains("Authored cases requested: 196"));
    assert!(summary.contains("Planned cases processed/requested: 89/190"));
    assert!(!summary.contains("- Planned cases: 89"));
}

#[test]
fn complete_summary_uses_the_same_explicit_denominators() {
    let summary = render(json!({
        "completed": true,
        "requestedCaseFiles": ["one.yaml", "two.yaml"],
        "requestedTotals": {
            "documents": 2,
            "authoredCases": 3,
            "plannedCases": 2,
            "developmentPlannedCases": 1,
            "evaluationPlannedCases": 1
        },
        "totals": {
            "documents": 2,
            "cases": 2,
            "developmentCases": 1,
            "evaluationCases": 1,
            "passed": 2
        },
        "documents": []
    }));

    assert!(summary.starts_with("## Totals\n"));
    assert!(summary.contains("Documents processed/requested: 2/2"));
    assert!(summary.contains("Planned cases processed/requested: 2/2"));
}

#[test]
fn legacy_incomplete_summary_never_promotes_prefix_cases_to_corpus_size() {
    let report = json!({
        "completed": false,
        "requestedCaseFiles": ["one.yaml", "two.yaml"],
        "totals": { "documents": 1, "cases": 7 },
        "documents": []
    });
    let summary = render(report.clone());
    let counts = render_counts(report);

    assert!(summary.contains("## INCOMPLETE - Processed Totals"));
    assert!(summary.contains("Documents processed/requested: 1/2"));
    assert!(summary.contains("Authored cases requested: unknown"));
    assert!(summary.contains("Planned cases processed/requested: 7/unknown"));
    assert_eq!(counts["report_complete"], false);
    assert_eq!(counts["processed_cases_count"], 7);
    assert!(counts["requested_authored_cases_count"].is_null());
    assert!(counts["requested_planned_cases_count"].is_null());
}

#[test]
fn workflow_preserves_completion_context_in_github_and_slack_outputs() {
    let workflow = include_str!("../.github/workflows/benchmark.yml");

    assert!(!workflow.contains(".completed // true"));
    assert!(workflow.contains("scripts/render-benchmark-summary.sh"));
    assert!(workflow.contains("report_complete: $report_complete"));
    assert!(workflow.contains("processed_documents_count: $processed_documents_count"));
    assert!(workflow.contains("requested_documents_count: $requested_documents_count"));
    assert!(workflow.contains("processed_cases_count: $processed_cases_count"));
    assert!(workflow.contains("requested_planned_cases_count: $requested_planned_cases_count"));
    assert!(workflow.contains("usage benchmark report is INCOMPLETE"));
    assert!(workflow.contains("usage benchmark run did not write a report"));
}
