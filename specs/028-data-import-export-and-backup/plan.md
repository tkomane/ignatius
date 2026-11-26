# Plan - Import transaction ownership

Use the CLI argument contract plus a defensive check at the session boundary.
Check transaction state under the existing user SQL gate before beginning an
owned transaction. Ensure the COPY sink ends before COMMIT or ROLLBACK.
Use the PostgreSQL response to report rollback and preserve the COPY failure
when cleanup also fails. Leave target resolution, production confirmation
and the explicitly unwrapped route intact.

Prove the CLI refuses the flags without a readable input file. Exercise
transaction boundaries with synthetic rows on disposable PostgreSQL, and
verify the actual executable reports a rejected combination and zero new
rows. Run the full local verifier and check the exact pull request and main.
