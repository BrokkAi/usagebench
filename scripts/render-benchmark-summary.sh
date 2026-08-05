#!/usr/bin/env bash
set -euo pipefail

mode="summary"
if [ "$#" -eq 2 ] && [ "$1" = "--failing-cases" ]; then
  mode="failing-cases"
  report_path="$2"
elif [ "$#" -eq 2 ] && [ "$1" = "--counts" ]; then
  mode="counts"
  report_path="$2"
elif [ "$#" -eq 1 ]; then
  report_path="$1"
else
  echo "usage: $0 [--counts|--failing-cases] REPORT_JSON" >&2
  exit 2
fi

render_failing_cases() {
  jq -r '
      [
        .documents[]? as $document
        | $document.cases[]?
        | select(.status == "failed" or .status == "error")
        | "- \(.status): \(.id) (\($document.caseFile))"
          + (
            if .declarationToUsages then
              " references=\(.declarationToUsages.status):\(.declarationToUsages.rawStatuses | join(",")) missing=\(.declarationToUsages.missing | length), extra=\(.declarationToUsages.unexpected | length)"
            else
              ""
            end
          )
          + (
            ((.usageToDeclaration // [])
              | map(select(.status == "failed" or .status == "error" or .status == "unsupported"))
              | map("\(.operation // "profile_default")=\(.status):\(.rawStatus),targets=\(.actualDeclarations | length)")
              | join("; ")) as $navigation_issues
            | if ($navigation_issues | length) > 0 then
                " navigation=[\($navigation_issues)]"
              else
                ""
              end
          )
      ] as $items
      | if ($items | length) > 0 then
          ($items[0:10][]),
          (if ($items | length) > 10 then "- ... \(($items | length) - 10) more failing case(s)" else empty end)
        else
          empty
        end
    ' "$report_path"
}

if [ "$mode" = "failing-cases" ]; then
  render_failing_cases
  exit 0
fi

counts_json="$(
  jq -c '
    (if has("completed") then .completed else true end) as $complete
    | (.totals.documents // (.documents | length)) as $processed_documents
    | (.requestedTotals.documents // (.requestedCaseFiles | length) // $processed_documents) as $requested_documents
    | (.totals.cases // 0) as $processed_cases
    | (.requestedTotals.authoredCases // (if $complete then ([.documents[]?.cases[]?] | length) else null end)) as $authored_cases
    | (.requestedTotals.plannedCases // (if $complete then $processed_cases else null end)) as $requested_cases
    | (.requestedTotals.developmentPlannedCases // (if $complete then (.totals.developmentCases // 0) else null end)) as $requested_development
    | (.requestedTotals.evaluationPlannedCases // (if $complete then (.totals.evaluationCases // 0) else null end)) as $requested_evaluation
    | {
        report_complete: $complete,
        processed_documents_count: $processed_documents,
        requested_documents_count: $requested_documents,
        processed_cases_count: $processed_cases,
        requested_authored_cases_count: $authored_cases,
        requested_planned_cases_count: $requested_cases,
        requested_development_cases_count: $requested_development,
        requested_evaluation_cases_count: $requested_evaluation
      }
  ' "$report_path"
)"
if [ "$mode" = "counts" ]; then
  echo "$counts_json"
  exit 0
fi

jq -r --argjson counts "$counts_json" '
  (if has("completed") then .completed else true end) as $complete
  | def count_or_unknown($value): if $value == null then "unknown" else ($value | tostring) end;
    if $complete then "## Totals" else "## INCOMPLETE - Processed Totals" end,
    "",
    "- Documents processed/requested: \($counts.processed_documents_count)/\(count_or_unknown($counts.requested_documents_count))",
    "- Authored cases requested: \(count_or_unknown($counts.requested_authored_cases_count))",
    "- Planned cases processed/requested: \($counts.processed_cases_count)/\(count_or_unknown($counts.requested_planned_cases_count))",
    "- Development cases processed/requested: \(.totals.developmentCases // 0)/\(count_or_unknown($counts.requested_development_cases_count))",
    "- Evaluation cases processed/requested: \(.totals.evaluationCases // 0)/\(count_or_unknown($counts.requested_evaluation_cases_count))",
    "- Passed: \(.totals.passed // 0)",
    "- Near misses: \(.totals.nearMisses // 0)",
    "- Position-unverified cases: \(.totals.positionUnverified // 0)",
    "- Improved: \(.totals.improved // 0)",
    "- Failed: \(.totals.failed // 0)",
    "- Expected failures: \(.totals.expectedFailures // 0)",
    "- Not planned: \(.totals.notPlanned // 0)",
    "- Unsupported cases: \(.totals.unsupported // 0)",
    "- Skipped: \(.totals.skipped // 0)",
    "- Errors: \(.totals.errors // 0)"
' "$report_path"

unsupported_navigation_lookups="$(
  jq '[.documents[]?.cases[]?.usageToDeclaration[]? | select(.status == "unsupported")] | length' "$report_path"
)"
echo "- Unsupported navigation lookups: $unsupported_navigation_lookups"

failing_cases="$(
  render_failing_cases
)"
if [ -n "$failing_cases" ]; then
  echo
  echo "## Failing Cases"
  echo
  echo "$failing_cases"
fi
