# Windows provider launch tasks (Feature 011)

- [x] T001 Add and review the account-free x64 Azure CLI MSI launch probe in
  the Windows platform job. Prove the exact bare-name outcome and the
  packaged interpreter result on the hosted runner. Record image,
  architecture, installed version and run evidence. [Plan](plan.md)

On 25 November 2026, commit `620e6e2c1384d4980b1eba151a84b935a358c5b8`
ran on Windows Server 2025 x64, image `windows-2025-vs2026` version
`20261116.315.1`, OS build `10.0.26100.0`. Azure CLI MSI `2.93.0` returned
`missing-program` for bare `az` on the MSI path and `version-matched` for its
packaged interpreter. [Windows job and receipt](https://github.com/tkomane/ignatius/actions/runs/45691294548/job/139477135355)

- [x] T002 Give the Windows x64 built-in Entra provider the verified MSI
  launch route. Prove native precedence, pre-start-only fallback, intact
  configured overrides, TLS and the shared deadline and credential handling.
  Verify the automatic route through the opt-in Windows version probe.

On 25 November 2026, commit `fc221a297d41b0edcfddf823f7036d3fe6f6c33b`
ran on Windows Server 2025 x64, image `windows-2025-vs2026` version
`20261116.315.1`, OS build `10.0.26100.0`. For Azure CLI MSI `2.93.0`, bare
`az` reported `missing-program`, the direct package control reported
`version-matched` and the production built-in resolver reported
`resolved-entra=version-matched`. The Windows junction and zero-timeout
configured-provider tests passed. [Windows job and receipt](https://github.com/tkomane/ignatius/actions/runs/45695897697/job/139493085072)

- [x] T003 Verify the built-in direct AWS CLI launch on hosted Windows x64
  against the installed MSI version using an account-free version probe.
  Record the runner identity, source and observed launch result.

The opt-in CI dispatch at `732a882` passed on Windows Server 2025 x64
(`windows-2025-vs2026`, image `20261116.315.1`, OS build `10.0.26100.0`).
The built-in direct launch of `aws.exe --version` reported AWS CLI v2
`2.40.3`, matching the registered MSI `2.40.3.0`. The fixed-argument probe
used a synthetic TLS-required target and a fresh account-free worker.
Live RDS or Aurora IAM authentication requires its own controlled test.
[Windows job and receipt](https://github.com/tkomane/ignatius/actions/runs/45705066295/job/139524815561)
