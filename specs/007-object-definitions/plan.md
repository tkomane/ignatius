# Shared object-tree connection disclosure

The header displays the history status and shared metadata explanation as
independent parts. Both use the existing width fitting and information style.
Connection state, authentication, catalogue reads and safety capsules retain
their established paths.

Verify the rendered header with history disabled, paused and temporarily
suppressed. In an owned terminal session, a PostgreSQL login limited to one
connection must show the fallback. Create a fresh schema after the refusal
and reload the tree. Check that the original backend also answers a follow-up
query and that quitting restores the terminal.

## Dedicated backend identification

Keep the query connection's configured application name. Use the custom
prefix for the object connection when it fits with ` (objects)` within
PostgreSQL's standard 63-byte limit. Use the default application name as
the prefix when the custom name may be truncated or expanded by escaping.
[PostgreSQL 18 application name](https://www.postgresql.org/docs/18/runtime-config-logging.html#GUC-APPLICATION-NAME)

Read both names by backend PID in `pg_stat_activity`. Use a 63-character
ASCII query name, then verify catalogue reads and server write refusal on
the production object connection target.
