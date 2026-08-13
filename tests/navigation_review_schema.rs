use jsonschema::JSONSchema;
use serde_json::json;

fn validator() -> JSONSchema {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../benchmarks/review-protocol/navigation-response-v1.schema.json"
    ))
    .unwrap();
    JSONSchema::compile(&schema).unwrap()
}

fn response(operation: &str) -> serde_json::Value {
    json!({
        "schemaVersion": 1,
        "reviewer": {
            "provider": "openai",
            "model": "gpt-5.6-sol",
            "executionId": "session-1",
            "executedAt": "2026-08-13T00:00:00Z"
        },
        "records": [{
            "caseId": "navigation-case",
            "queries": [{
                "queryId": "query-1",
                "operation": operation,
                "query": {
                    "uri": "benchmark://source/src/main.rs",
                    "range": {
                        "start": {"line": 3, "character": 8},
                        "end": {"line": 3, "character": 13}
                    }
                },
                "decision": "accept",
                "confidence": "high",
                "resolvedIdentity": "example::Widget",
                "targets": [{
                    "uri": "benchmark://source/src/lib.rs",
                    "range": {
                        "start": {"line": 1, "character": 11},
                        "end": {"line": 1, "character": 17}
                    }
                }],
                "ambiguities": [],
                "rationale": "The expression has the explicitly declared Widget type."
            }],
            "inspectedPaths": ["source/src/main.rs", "source/src/lib.rs"],
            "rationale": "The complete fixture establishes one exact target."
        }]
    })
}

#[test]
fn accepts_each_supported_navigation_operation() {
    let validator = validator();
    for operation in ["declaration", "definition", "type_definition"] {
        assert!(
            validator.validate(&response(operation)).is_ok(),
            "{operation}"
        );
    }
}

#[test]
fn rejects_references_as_a_navigation_operation() {
    let validator = validator();
    assert!(validator.validate(&response("references")).is_err());
}

#[test]
fn rejects_missing_query_target_evidence() {
    let validator = validator();
    let mut value = response("type_definition");
    value["records"][0]["queries"][0]
        .as_object_mut()
        .unwrap()
        .remove("targets");
    assert!(validator.validate(&value).is_err());
}
