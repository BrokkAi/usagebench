# Per-case blinded navigation methodology v1

This profile extends the retained v3 source-review method to cases whose frozen
contract contains navigation queries but no declaration-centered References
contract. It does not reinterpret Type Definition as References and does not
change evidence produced under the v3 usage-review profile.

Each case is reviewed in one fresh provider-native session per provider. The
reviewer receives exactly:

- this versioned methodology, prompt, and response schema;
- one packet containing the requested operation and query location for every
  navigation query in the case; and
- the complete pinned fixture mounted as `source/`.

The packet excludes authored targets, first-review evidence, analyzer identity
and output, product capability/result state, other reviewers' responses, prior
adjudication, outcome-derived labels, and git history. Packet, fixture tree,
prompt, schema, raw response, and provider-native execution metadata are
content-addressed.

Allowed operations are `declaration`, `definition`, and `type_definition`.
Reviewers derive exact targets from source and language semantics. Primary
automatic consensus is mechanical equality for every query of:

- query ID, operation, and query location;
- decision and complete target set;
- high confidence; and
- no target-changing ambiguity.

The independently described resolved identity is advisory, not a consensus
field. Any disagreement, abstention, replacement proposal, non-high confidence,
or ambiguity requires accountable human adjudication. Exact consensus still
requires the human to adjudicate the case before the next milestone.

This evidence reviews a source contract. It neither requires nor assumes that
a candidate advertises the corresponding LSP capability. Later execution must
report unsupported capability separately from an incorrect target.
