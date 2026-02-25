# PyGen

A Jedi-based tool to extract usages from Python source code.

## Requirements

* [uv](https://docs.astral.sh/uv/getting-started/installation/)

## Getting Started

1. Install dependencies:
   ```bash
   uv sync
   ```

2. Run:
   ```bash
   uv run main.py <input-path> <output-dir>
   ```

## Development Commands

### Build (Install Dependencies)
```bash
uv sync
```

### Test All
```bash
uv run pytest
```

### Test Some
Run specific tests using mustache variables for filtering.
This command uses `pytest` to filter tests by class or method name.

```bash
uv run pytest -v -k "{{#classes}}{{value}} or {{/classes}}{{#fqclasses}}{{value}} or {{/fqclasses}}ConstraintToPreventEmpty"
```
