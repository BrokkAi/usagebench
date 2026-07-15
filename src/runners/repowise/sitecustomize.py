"""UsageBench trace hook for the pinned Repowise v0.31.0 adapter.

Python imports ``sitecustomize`` before executing the Repowise console script.
The hook records each call resolver input and output before GraphBuilder folds
individual calls into caller-to-callee edges. It does not alter the resolver's
return value or Repowise's persisted graph.
"""

import json
import os
import threading


TRACE_PATH = os.environ.get("USAGEBENCH_REPOWISE_CALL_TRACE")
if TRACE_PATH:
    from repowise.core.ingestion.call_resolver import CallResolver

    _original_resolve_one = CallResolver._resolve_one
    _trace_lock = threading.Lock()

    def _write_record(record):
        with _trace_lock:
            with open(TRACE_PATH, "a", encoding="utf-8") as trace:
                trace.write(json.dumps(record, sort_keys=True) + "\n")

    _write_record({"record_type": "trace_ready", "schema_version": 1})

    def _traced_resolve_one(self, file_path, call):
        resolved = _original_resolve_one(self, file_path, call)
        _write_record(
            {
                "record_type": "call_site",
                "source_file": file_path,
                "target_name": call.target_name,
                "receiver_name": call.receiver_name,
                "caller_id": call.caller_symbol_id,
                "line": call.line,
                "argument_count": call.argument_count,
                "callee_id": resolved.callee_id if resolved else None,
                "confidence": resolved.confidence if resolved else None,
            }
        )
        return resolved

    CallResolver._resolve_one = _traced_resolve_one
