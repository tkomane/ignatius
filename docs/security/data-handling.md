# Data handling

What this program stores, where, for how long, and what it never stores.

## Never persisted

- **Result rows.** Nothing is written to disk unless the user explicitly exports.
  There is no cache and no crash-recovery copy of results.
- **Passwords.** No credential is written to the configuration file. Profiles
  (Feature 002) will store a reference to a credential, never the credential.
- **SQL text.** Not written anywhere by default. Query history does not exist yet;
  when it does (Feature 003) it will be opt-out-able, scoped, and erasable, and
  will still never contain result rows.

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

`ignatius config paths` prints all of these. Deleting the configuration directory
returns the program to a first-run state and loses nothing else.

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
