# PyGen

A Jedi-based tool to extract usages from Python source code.

## Requirements

* Python 3.9+
* pip

## Getting Started

1. Install dependencies:
   ```bash
   python3 -m venv .venv
   source .venv/bin/activate  # On Windows use `.venv\Scripts\activate`
   pip install -r requirements.txt
   ```

2. Run:
   ```bash
   source .venv/bin/activate
   python3 main.py <input-path> <output-dir>
   ```

## Development Commands

### Build (Install Dependencies)
```bash
python3 -m venv .venv
source .venv/bin/activate  # On Windows use `.venv\Scripts\activate`
pip install -r requirements.txt
```

### Test All
```bash
# From the root of the repo:
python3 -m unittest discover -s pygen/tests -p 'test_*.py' -v
```

### Test Some
Run specific tests using mustache variables for filtering.
This command uses `pytest` to filter tests by class or method name.

```bash
cd pygen && pytest -v -k "{{#classes}}{{value}} or {{/classes}}{{#fqclasses}}{{value}} or {{/fqclasses}}ConstraintToPreventEmpty"
```
