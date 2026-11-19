# Compare environments with fleet

Choose the baseline and at most three candidates from your existing
`[profiles]` configuration. Each selected profile supplies `host`,
`dbname` and `user`. Set `port` for destinations outside port 5432.
Use a password-file entry for each target or the profile's cloud identity
provider. Set the TLS mode on each profile. `PGSSLROOTCERT` can supply a
shared trust root. Fleet uses these target-scoped identities in place of
ambient `PGPASSWORD` or client certificates.

```sh
ignatius fleet --baseline @development --candidate @staging \
  --candidate @production-readonly --schema public --format json
```

Omit `--schema` to use the Feature 057 non-system schema scope. The filter
applies to snapshots. Extension inventories are per database and allowlisted
settings are the effective values for each session.

The command opens the selected sessions read-only and reports the observed
server version number, recovery posture and TLS state. The `sslmode` field
records the requested verification policy. A catalog comparison can have
one of three results.

| Result | Meaning |
| --- | --- |
| `match` | Both inputs for that dimension were complete and equal. |
| `drift` | Both inputs were complete and differed. |
| `unavailable` | At least one side could not be compared. Check the reason for that side. |

The schema comparison shows both fingerprints and severity counts. Use
`ignatius snapshot capture` and `ignatius snapshot check` for object-level
details. Extensions and settings show before and after values for each
observed difference.

Output defaults to a table. `--format plain` prints compact status lines.
JSON format version 1 reports ordered target health and comparisons with
separate `complete` and `drift` flags. Exit 0 means complete equality and
exit 1 means drift or incomplete evidence. Invalid arguments return exit 2.
Invalid command-wide configuration returns exit 3.
