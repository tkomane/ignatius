- Security: The benchmark overlay never touches data by surprise. `m` only
  switches rollback mode. Rollback-off runs of statements that are not reads
  are refused on production and in read-only sessions and need confirmation
  elsewhere; on production even rolled-back writes wait for confirmation
  because they take locks while they run. Benchmarks refuse to run inside an
  open transaction and run A then B in order on the session they started on.
  While a benchmark runs, workbench statements, plan-diff ANALYZE and profile
  switches are refused with a message naming the way out, and Cancel stops the
  benchmark and says so, so it never names the wrong statement.
  `ignatius bench` on a production target refuses statements that are not
  reads unless `--allow-write` is given, before it connects.
