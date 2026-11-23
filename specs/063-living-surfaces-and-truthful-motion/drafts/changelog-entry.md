- New: Switching theme sweeps the new palette across the pane body from left
  to right in about a third of a second, behind a hard edge so every cell stays
  readable in one theme or the other. The header and footer change in the
  first frame, so `[PROD]`, `[READ ONLY]` and transaction markers are never
  split between two themes. Reduced motion, ASCII, colour off and 16-colour
  terminals switch at once (Feature 063).
- New: The Results status word of a complete, successful result settles from
  the success colour to its resting colour over about half a second, so an
  identical rerun is visibly new. Failed, cancelled and truncated outcomes keep
  their colours, and lower colour depths stay still.
- Improved: The running indicator is a heartbeat comet that travels back and
  forth and never fills, so it can no longer read as progress, and turns amber
  while cancellation is requested.
- Improved: The workbench recedes into quiet ink behind blocking overlays while
  the header and footer stay untouched; modal badges follow a three-tier
  weight, with full capsules kept for safety states; modals fit their content;
  plan rows use tree chevrons and compact metrics; the palette prompt follows
  the glyph tier, with a placeholder and plural-aware counts.
- Improved: The footer offers an open overlay's own keys and keeps the way out
  longest; header safety capsules survive every width and density; the Results
  toolbar states a local sort once with its `retained rows only` qualifier and
  drops whole facts when narrow; the boot reveal's identity line is now "a
  quiet instrument for PostgreSQL".
- Fixed: Header clicks resolve to the column header row in compact density and
  under warnings; the snippet library blocks clicks to the panes behind it.
- Security: CI audits every GitHub Actions workflow, local action and the
  Dependabot policy with offline, digest-pinned zizmor 1.32.0, with reviewed
  exceptions pinned by contract test. Workflow scripts receive expression
  values only through the environment, contract tests enforce checkout
  credential removal and container image digests, and Dependabot version
  updates wait seven days after an upstream release.
