# Legacy promotion v1 cohort audit

The machine-readable source of truth is
`benchmarks/promotion/legacy-v1/cohort.json`. It inventories all 158 historical
development cases and binds the exact 35 YAML documents, selection policy, and
generator with SHA-256 digests. This audit summarizes that frozen artifact; it
does not record agent review, adjudication, analyzer execution, or publication.

## Evidence boundary

The cohort is retrospectively selected from the named analyzer-informed legacy
corpus. Selection used checked-in source contracts only. Historical Bifrost and
language-server outcomes, `expectedFailure`, pass/fail state, disagreement, and
regression interest were excluded. Consequently, later independent review can
strengthen these exact contracts but cannot make their original selection
preregistered or establish language-wide or ecosystem-wide accuracy.

All cases use checked-in fixtures. Normal case validation checked fixture
existence and authored ranges before generation. The inventory distinguishes a
present project file from an executed project load: this freeze performed no
compiler, build-host, language-server, or analyzer run.

## Population and denominator

The 158-case boundary excludes only the two published semantic-pack navigation
cases added after the legacy corpus was established. Six source-contract
controls (`unsupported` or `notPlanned`) are outside correctness eligibility.
The lowest eligible count is 10, so the frozen rule yields `N = 10` and a
110-case balanced core.

| Language | Population | Eligible | Core | Overflow | Controls |
| --- | ---: | ---: | ---: | ---: | ---: |
| C++ | 16 | 15 | 10 | 5 | 1 |
| C# | 16 | 16 | 10 | 6 | 0 |
| Go | 12 | 11 | 10 | 1 | 1 |
| Java | 11 | 11 | 10 | 1 | 0 |
| JavaScript | 11 | 10 | 10 | 0 | 1 |
| PHP | 14 | 14 | 10 | 4 | 0 |
| Python | 15 | 13 | 10 | 3 | 2 |
| Ruby | 21 | 20 | 10 | 10 | 1 |
| Rust | 15 | 15 | 10 | 5 | 0 |
| Scala | 15 | 15 | 10 | 5 | 0 |
| TypeScript | 12 | 12 | 10 | 2 | 0 |
| **Total** | **158** | **152** | **110** | **42** | **6** |

## Coverage and replacement

Every core covers References where the case exposes a declaration, plus the
available canonical navigation operations. Across the core, the deterministic
diversity ordering represents calls, construction, imports/aliases, nominal
types, state/properties, inheritance/dispatch, and language-specific generated,
module, or dynamic-source features where present. The manifest exposes each
case's exact operations, symbol kind, semantic family, source complexity, and
near-duplication group so reviewers can inspect the balance directly.

Within each language, `selectionOrder` 1 through 10 is the core. Later eligible
rows are both overflow and the immutable replacement queue. A rejected core
case may be replaced only by the next overflow row for that language; N remains
10. The six controls follow the eligible ordering and keep separate statuses.

The controls are:

- C++ configured compile-command case (`unsupported`)
- Go build-tag case (`unsupported`)
- JavaScript computed-name case (`not_planned`)
- Python dynamic `getattr` and `__getattr__` cases (`not_planned`)
- Ruby `public_send` case (`not_planned`)

Any post-review membership, ordering, or denominator change requires a new
versioned cohort. The existing artifact must remain available and unchanged.
