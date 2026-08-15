# Personas and jobs

## Primary: the infrastructure engineer with production access

Works across cloud and on-premises systems. Fluent in the terminal, not primarily
a SQL developer. Has credentials for databases where a careless statement is an
incident.

What they actually need is not a better editor. It is to know, without looking
anything up, which database they are pointed at, whether the connection is
protected, and whether the statement they just cancelled actually stopped.

Their jobs:

- Check something in a database they have not touched in months, quickly, without
  re-learning the tool.
- Run a diagnostic query against production and be certain, at a glance, that it
  is production.
- Stop a query that is holding a lock, and know whether it stopped.
- Put the same query into a script tomorrow, with an exit code to branch on.
- Explain to a colleague what went wrong, using output they can paste safely.

What makes them abandon a tool: a status that turned out to be a lie, a silent
fallback, an interface that needs colour they cannot see, or a client that cannot
be scripted.

## Secondary: the application developer

Lives in a repository and a local database. Wants iteration speed: type SQL, see
rows, adjust, repeat, without leaving the terminal.

Their jobs:

- Iterate on a query against a local or development database.
- Read a wide result without it wrapping into unreadable text.
- Understand a PostgreSQL error well enough to fix it without a search engine.
- Keep useful queries as ordinary `.sql` files in the repository.

What makes them abandon a tool: slow startup, a fight with the editor, or losing
their buffer.

## Tertiary: the data analyst who lives in a terminal

Comfortable in SQL, less so in infrastructure. Wants results out of the terminal
and into a file or a pipeline.

Their jobs:

- Explore a schema they do not know.
- Extract a result to CSV or JSON, and know whether it was complete.
- Distinguish a NULL from an empty string without guessing.

## Who this is not for

- Someone who wants one client for PostgreSQL, MySQL and MongoDB. That
  contradicts the second principle of the constitution.
- Someone who wants a GUI. DBeaver, TablePlus and DataGrip are good.
- A team wanting shared saved queries and accounts. Saved queries are `.sql`
  files; use Git.
