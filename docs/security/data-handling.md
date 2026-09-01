# Data handling

What this program stores, where, for how long, and what it never stores.

## Never persisted

- **Result rows.** Nothing is written to disk unless the user explicitly exports.
  There is no cache and no crash-recovery copy of results.
- **Passwords.** No credential is written to the configuration file. A connection
  profile that names `password`, `pgpassword` or `sslpassword` is refused, by
  name, with the routes that do exist; the refusal never repeats the value back.
  A profile that references a credential held elsewhere is a later feature, and
  a reference is not a secret.
- **A password typed at the prompt.** When the server asks for one, what is
  typed lives in memory for as long as it takes to try the connection again and
  is then dropped. It is never written to the configuration file, never recorded
  in the history, never logged, and never passed as a process argument. The type
  holding it prints `<hidden>` rather than its contents, so an unexpected
  `{:?}` - in a panic, a test failure, a log line - cannot leak it. Nothing
  offers to remember it: an OS credential store is a later feature and will
  store a reference, not a secret.
- **Result rows in the history.** The statement history records the SQL that ran
  and how it ended. It never records what came back.
- **A cloud identity token.** When a connection authenticates through `entra`,
  `aws`, `gcp` or a provider defined in configuration, the credential is obtained
  by running that cloud's own tool and reading its standard output. It lives in
  `SecretString` for as long as the connections of one session take to open and
  is then dropped. It is never written to any file, never an environment
  variable, never a process argument, never recorded in the history and never
  logged. This client keeps no token cache of its own: the cloud's tool has one
  already, and a second would be a second thing to get wrong.

  What **is** written to configuration is the provider's name and, for a provider
  the user defines, the command to run. A name is not a secret and neither is a
  command line - but naming a program there means this client will execute it,
  which the threat model states as a capability the file did not previously have.

## Exports

An export is the one route by which result data is written to disk, and it only
happens when the user asks for it by naming a file. Nothing is cached, and no
result is ever written anywhere else.

An export never overwrites an existing file without `--force`, and never leaves
a truncated file at the destination: incomplete work stays under a `.partial`
name, which is reported rather than cleaned up silently, because the rows in it
may be what the user needed.

## Written to disk

| What | Where | Permissions | Lifetime |
| --- | --- | --- | --- |
| Configuration | `~/.config/ignatius/config.toml` or `%APPDATA%\ignatius\config.toml` | `0600` on Unix; profile ACL on Windows | Until deleted |
| Migration backups | Beside the configuration, timestamped | Same | Until deleted |
| Logs | `~/.local/share/ignatius/logs/ignatius.log` | `0600` on Unix | Rotated at 5 MB, one previous file kept |
| Saved queries | `~/.config/ignatius/queries/*.sql` | The directory's own | Until deleted. Written only when the user names one |
| Statement history | `~/.local/share/ignatius/history.jsonl` | `0600` on Unix, set at creation and kept when the file is trimmed | Last 1000 entries by default; `ignatius history clear` removes it |

`ignatius config paths` prints all of these. Deleting the configuration directory
returns the program to a first-run state and loses nothing else.

## Release evidence handling

Feature 008 release evidence is separate from user configuration, query data and
local support logs. The current non-publishing workflow may retain only bounded
candidate artefacts, release records, checksum sidecars and safe identity or
gate metadata needed to review one exact target. Examples include product
version, source revision, working-tree state, target, compiler, build identity,
archive basename, byte size, SHA-256 digest and explicit inventory, signature,
provenance and readiness states.

Release evidence must not contain passwords, tokens, credential-bearing URIs,
SQL text, result rows, exports, configuration files, statement history,
environment dumps, process arguments or unrestricted logs. A workflow must use
explicit artefact, record and evidence paths rather than uploading the
workspace or user-data directories. The provider-neutral inventory and
signature/provenance contracts are defined in `release-evidence/inventory.md`
and `release-evidence/README.md`; their presence is not evidence that those
checks or evidence objects exist.

The candidate staging root is constrained by the schema-1 allowlist in
`release-evidence/scope.schema.json`: it contains a unique, non-empty list of
staging-root-relative portable paths, and the checker requires exactly those
ordinary files. Absolute or traversal paths, empty or dot components,
backslashes, colons, symlinks, non-regular files and paths outside the root are
rejected. This is only a file-set boundary; it does not scan secrets, generate
an inventory, verify signing or provenance, invoke a hosted runner, upload an
artefact or publish user data.

A local rehearsal is labelled `rehearsal` or `blocked` and never `published`.
Any hosted or long-term retention path for a live release, plus signing,
provenance, SBOM storage or publication, remains decision-gated and requires
owner-approved lifecycle, provider or offline mechanism where applicable, and
authorization.

## Native credentials and tickets

ADR-0012 closed Feature 001a without a native adapter. Its retained conditional
data-handling rule is still fixed: if a future GSSAPI, Kerberos or SSPI
requirement reopens that work, credentials, tickets, tokens and native
connection strings are secrets even when a provider describes them as
metadata. Any such adapter must keep them behind the existing non-printing
secret boundary and must never place them in configuration, history, logs,
release evidence or support bundles.

Native-driver diagnostics are untrusted text. They must pass through the same
redaction and terminal-sanitisation paths as other server-supplied messages
before display or logging. A support bundle may identify the product, source or
build, target, native dependency and error category, but must contain no ticket,
token, password, SQL text or result value. These rules do not select a provider,
dependency route or authentication mechanism.

## The statement history

Interactive sessions record what was run. Scripted `ignatius query` runs do not:
a script's SQL is already in the script, and a CI job should not accumulate a
file of statements nobody asked it to keep.

Each entry holds the time, the redacted connection target, the database, the
environment classification, the statement, its outcome and how long it took. It
holds no result rows and no password: the target goes through the same single
redaction implementation as every other output path before it is written.

**A statement that mentions a credential is never written.** The check looks for
`password`, `passwd`, `secret`, `token`, `credential`, `api_key`, `apikey` and
`private_key` as substrings, so `password_hash` and `access_token` are caught
too. It over-refuses on purpose: a query about a `tokens_used` column is not
recorded either. A missing entry costs a retype; a password in a file that
outlives the session costs more.

That check is a heuristic and is not the only control. All of these are real:

| Control | What it does |
| --- | --- |
| `history.enabled = false` in `config.toml` | Nothing is ever recorded |
| `--no-history` | Pauses recording for one run |
| `Ctrl+K v` in the client | Pauses recording for the session, and the header says so |
| `ignatius history clear --yes` | Deletes the file |
| `ignatius history list` | Shows exactly what is kept |

A paused session is announced rather than hidden, in the header of the
full-screen client and in the opening lines of plain mode. The mode that keeps
less is the one worth stating.

## Logging

Off unless `IGNATIUS_LOG` is set. When on:

- SQL text and row values are never written. A statement appears as a job id and
  a character count, which identifies it in a support conversation without
  revealing it.
- Only Ignatius-owned events are admitted. Dependency tracing is excluded
  because a database driver may emit complete SQL at debug level. The variable
  accepts only `off`, `error`, `warn`, `info`, `debug` or `trace`; target
  directives fail closed.
- Every complete event passes through the one redaction implementation before
  any bytes are written.
- The file is size-bounded and rotated, so it cannot grow without limit.

## In memory

- Passwords are held in `secrecy::SecretString`, which does not print itself and
  zeroes on drop. Zeroing is best-effort: no userspace program can guarantee a
  value is unrecoverable from memory.
- Result rows are bounded by `query.max-buffered-rows`, default 10000. Rows past
  the cap are counted and dropped rather than retained.

## Exposure this program cannot fix

- **Process arguments.** A password in a connection string on the command line is
  visible to every other process on the machine and is written to shell history.
  Use a `.pgpass` file, which this client reads: it keeps the password out of the
  command line, out of the environment, and out of shell history. A file others
  can read is refused rather than used.
- **Terminal scrollback.** Results shown on screen are in the terminal's buffer
  and, depending on the emulator, in its saved sessions.
- **The clipboard.** Anything copied is readable by other applications. Timed
  clearing is under consideration but will not be described as a guarantee,
  because the operating system does not provide one.
