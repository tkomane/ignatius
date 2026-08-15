# Landscape

**Research date: 2026-08-15.** Activity figures were read from the GitHub REST
API and PyPI on that date; product facts for the commercial tools were read from
their own sites. Anything not verified on that date is marked as unverified.

The purpose of this document is to decide whether a new PostgreSQL client is
worth building, and if so, which parts of it are actually differentiated. It is
not a competitor takedown. Every tool below is maintained, several are excellent,
and two of them are what most people should keep using.

## What is actually out there

| Tool | Version seen | Last activity | Licence | Shape |
| --- | --- | --- | --- | --- |
| `psql` | 18.6 (2026-08-13) | Active, ships with PostgreSQL | PostgreSQL | Line-oriented CLI |
| pgcli | 4.5.0 (2026-06-03) | Pushed 2026-08-03 | BSD-3-Clause | Enhanced REPL |
| Rainfrog | v0.4.3 (2026-08-08) | Pushed 2026-08-08 | MIT | Rust TUI |
| Harlequin | v2.9.0 (2026-08-15) | Pushed 2026-08-15 | MIT | Python TUI, multi-adapter |
| Lazysql | v0.5.5 (2026-06-29) | Pushed 2026-07-26 | MIT | Go TUI, multi-database |
| usql | v0.21.4 (2026-03-26) | Pushed 2026-06-19 | MIT | Universal CLI |
| DBeaver CE | 26.1.4 (2026-08-02) | Pushed 2026-08-15 | Apache-2.0 | Java desktop GUI |
| Beekeeper Studio | v6.0.1 (2026-08-14) | Pushed 2026-08-15 | See note | Electron desktop GUI |
| TablePlus | Not verified | Active product | Commercial | Native desktop GUI |
| DataGrip | Not verified | Active product | Commercial | JetBrains IDE |

Notes on the table:

- Beekeeper Studio's repository licence is reported by GitHub as `NOASSERTION`,
  which usually means a modified or dual licence. Anyone depending on its terms
  should read the repository's own licence file rather than trusting a summary.
- TablePlus and DataGrip do not publish a version on their landing pages, and
  neither advertises a terminal, TUI or headless mode. Their pricing tiers were
  not verified in detail on the research date and are not relied on below.
- "Last activity" is repository push time. It shows the project is alive; it does
  not measure release cadence or quality.

Primary sources:

- PostgreSQL versioning policy and current release: <https://www.postgresql.org/support/versioning/>
- `psql` reference: <https://www.postgresql.org/docs/current/app-psql.html>
- pgcli: <https://github.com/dbcli/pgcli>, <https://pypi.org/project/pgcli/>
- Rainfrog: <https://github.com/achristmascarl/rainfrog>
- Harlequin: <https://github.com/tconbeer/harlequin>
- Lazysql: <https://github.com/jorgerojas26/lazysql>
- usql: <https://github.com/xo/usql>
- DBeaver: <https://github.com/dbeaver/dbeaver>
- Beekeeper Studio: <https://github.com/beekeeper-studio/beekeeper-studio>
- TablePlus: <https://tableplus.com/>
- DataGrip: <https://www.jetbrains.com/datagrip/>

## How they differ, by the dimensions that matter here

**Onboarding.** `psql` assumes you already know libpq. pgcli is friendlier and
autocompletes well. The TUIs vary: Harlequin and Rainfrog both open into
something usable. The GUIs are the easiest to start with and the hardest to
script. None of the terminal tools stages connection feedback (address, TCP, TLS,
authentication, database) when a connection fails; you get one error.

**PostgreSQL depth.** `psql` is definitionally complete: it is the reference
implementation of client behaviour. usql, Harlequin and Lazysql are deliberately
multi-database, which caps how far PostgreSQL-specific semantics can be pushed
before the abstraction leaks. pgcli and Rainfrog are PostgreSQL-shaped, with
pgcli the closest to `psql` in fidelity.

**Result exploration.** The GUIs win outright on grid ergonomics. The TUIs are
much better than a scrolling REPL. `psql` has the pager and nothing else.

**Connection security.** This is where the terminal tools are thinnest. libpq
gives `psql` the full `sslmode` matrix including `verify-ca` and `verify-full`,
plus service files and `.pgpass`. Tools that reimplement the client protocol have
to rebuild that surface, and what any given tool actually verifies is often not
visible in the interface. Very few tools show the negotiated protection back to
the user, as distinct from what was requested.

**Production safeguards.** Broadly absent from the terminal tools. Some GUIs have
colour-coded environments, which is useful but is colour-only, and none of them
treats "which database am I about to run this against" as a first-class,
text-labelled, always-visible fact.

**Automation.** `psql` and usql are excellent here and set the bar: stable
formats, stable exit codes, pipe-friendly. The TUIs mostly are not scriptable,
and the GUIs are not scriptable at all.

**Accessibility.** Weak across the whole category. Full-screen TUIs are hard for
screen readers, and few tools offer a genuine plain line-oriented mode as an
alternative to their own interface.

**Footprint.** The TUIs and CLIs are small. DBeaver carries a JVM, Beekeeper
carries Electron.

## The hypothesis, and whether it survives

> A PostgreSQL-specific terminal workbench can outperform generic database
> clients by combining a discoverable full-screen query workflow, first-class
> PostgreSQL semantics, production-aware safeguards, portable automation, and a
> refined terminal-native visual system in one small local application.

Partly. Two parts of it do not survive contact with the evidence:

- **"Discoverable full-screen query workflow" is not a wedge on its own.**
  Harlequin and Rainfrog already do this, are actively maintained, and are good.
  Building a third one and calling that the differentiator would be a waste.
- **"First-class PostgreSQL semantics" is not a wedge against `psql`.** `psql`
  is the reference. The realistic claim is PostgreSQL fidelity in a full-screen
  interface, not fidelity beyond `psql`.

Three parts do survive, and they are related:

1. **Truthful connection state.** Showing what protection was actually negotiated
   rather than what was requested, and refusing to silently proceed when a
   security-relevant parameter is not supported. This is nearly absent in the
   category and it is exactly what a client used for production-adjacent work
   needs.
2. **Production awareness that does not rely on colour.** Explicit, user-declared
   environment classification, never inferred from a host name, shown as a word.
3. **One tool that is both a workbench and a scriptable CLI**, with documented
   exit codes and machine formats, so the thing you explore with is the thing you
   automate with.

The honest summary of the opportunity: the gap is not "nobody built a PostgreSQL
TUI". It is that the tools which are pleasant to explore with are not the tools
you would trust next to production, and the tool you trust next to production
(`psql`) is not pleasant to explore with.

## Build, defer, reject

| Capability | Decision | Reason |
| --- | --- | --- |
| Negotiated-vs-requested TLS state, read back from the server | Build | The differentiator, and cheap once the adapter exists |
| Security-relevant parameters refused rather than ignored | Build | Silent weakening is the failure mode that matters |
| Explicit environment classification, shown as a word | Build | Differentiator; colour-only marking is what everyone else does |
| Layered errors with SQLSTATE, cause and next action | Build | Table stakes done properly; cheap and constantly used |
| Bounded result memory with visible truncation | Build | Correctness, and it prevents a whole class of hangs |
| Terminal-safe rendering of hostile values | Build | Nobody advertises it; it is a real injection route |
| Stable exit codes and machine formats | Build | Table stakes for the automation half of the claim |
| Full-screen editor plus result grid | Build | Table stakes, not a differentiator. Do it well, claim nothing |
| Connection profiles with credential-store references | Build | Table stakes for daily use |
| Autocompletion of schema objects | Defer | Expensive to do well; pgcli already sets a high bar |
| Visual EXPLAIN | Defer | High value, but only after the core loop is trustworthy |
| SSH tunnels, cloud token authentication | Defer | Real demand, but each is its own threat-model change |
| `verify-ca` | Defer | Deliberately unimplemented in 0.1 and refused loudly |
| Multi-database support | Reject | Directly contradicts principle II |
| Schema migrations, admin suite, visual schema editing | Reject | Different products |
| AI-generated SQL | Reject | Contradicts principle III |
| Charts and dashboards | Reject | Different product |
| Plugin marketplace | Reject | No user need yet, large permanent cost |

## Naming gate

**Run on 2026-08-15. Name confirmed: Ignatius.**

| Check | Result |
| --- | --- |
| crates.io | `ignatius` unregistered |
| Homebrew core | No formula named `ignatius` |
| GitHub, database tooling | Zero repositories matching `ignatius postgres`. The 737 repositories with `ignatius` in the name are personal projects and unrelated products, the largest at 4 stars |
| Domains | `ignatius.io` is registered and in use. `ignatius.sh` did not resolve; `ignatius.dev` had no address record. Registration status was not confirmed with a registrar |
| Trademark databases | **Not checked.** No search was run against any trademark register |

Against the rejection criteria: it does not collide with an existing PostgreSQL
project, it is easy to type and to say, and it names nothing temporary about the
product. It is a person's name, which carries no functional claim to outgrow.

Residual work before publishing: confirm a domain, and run a trademark search in
the relevant jurisdictions. Neither blocks development, and neither is something
this project can answer from a terminal.

Identity still resolves through `src/branding.rs`, now so that the binary name,
display name, configuration directory, keyring service and `application_name`
cannot drift apart.
