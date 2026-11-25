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
