# Release-record fixtures

These records are synthetic validation inputs. They are not releases, do not
refer to real artefacts, and contain no credentials or user data.

Each JSON file is a one-record catalogue that uses
`../catalog.schema.json`:

- `valid-candidate.json` has a clean source identity, a complete checksum and
  covered target row. It remains a `candidate` because signing and provenance
  are deliberately not configured.
- `blocked-missing-checksum.json` is structurally valid but explicitly blocked
  because its artefact has no checksum.
- `mismatched-source.json` is structurally valid but explicitly blocked because
  the artefact and evidence source revisions do not match the release record.

The schema checks shape and allowed vocabulary. A later semantic validator must
check cross-entity equality, unique versions and the reasons represented by the
blocked fixtures.
