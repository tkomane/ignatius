# Quickstart: Schema-aware completion

## TUI

1. Start a connection as usual: `ignatius connect <target>`.
2. Wait for the object tree/catalogue status to say it is loaded, or use the
   existing object reload action.
3. In the editor, type `SELECT ` and press `Ctrl+Space`.
4. Use Up/Down to choose a column or object. Enter inserts the quoted name;
   Esc leaves the SQL exactly as it was.
5. Type `FROM orders o`, then `o.` and press `Ctrl+Space` to see only columns of
   the aliased relation. Use `public.` to scope objects to that schema.
6. Press `Ctrl+Z` once after acceptance to undo the complete replacement.

Automatic popups are enabled by default. To keep typing quiet while retaining
explicit completion, add this to `config.toml`:

```toml
[ui]
completion = false
```

## Plain mode

Plain mode loads the same snapshot after connecting:

```text
orders => SELECT * FROM ord
orders => \complete
1  orders  table  public  readable
orders => \use 1
Completion inserted: orders
orders => ;
```

The completion command does not run the partial SQL statement. The exact
transcript may include a truthful loading/stale/unavailable note.
Use `\use <number>` or the exact candidate name. A bad choice leaves the SQL
buffer untouched and does not run anything.

## Focused verification

```bash
cargo test query::completion app::update::tests::completion
cargo test --test cli_contract plain_mode_completion
cargo test --test postgres_integration completion_catalog
```

Start the disposable database when the integration test needs it:

```bash
cargo --locked xtask db up
cargo --locked xtask verify
```

The final verification report must separate focused test results, full-suite
results, live PostgreSQL evidence, and any skipped platform or terminal checks.
