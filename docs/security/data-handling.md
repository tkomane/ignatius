# Data handling

What this program stores, where, for how long, and what it never stores.

## Never persisted

- **Result rows.** Nothing is written to disk unless the user explicitly exports.
  There is no cache and no crash-recovery copy of results.
- **Passwords.** No credential is written to the configuration file. Profiles
  (Feature 002) will store a reference to a credential, never the credential.
- **Result rows in the history.** The statement history records the SQL that ran
  and how it ended. It never records what came back.

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
| Statement history | `~/.local/share/ignatius/history.jsonl` | `0600` on Unix, set at creation and kept when the file is trimmed | Last 1000 entries by default; `ignatius history clear` removes it |

`ignatius config paths` prints all of these. Deleting the configuration directory
returns the program to a first-run state and loses nothing else.

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
- Every message passes through redaction.
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
