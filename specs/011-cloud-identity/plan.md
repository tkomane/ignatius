# Implementation plan for Windows x64 Azure CLI launch evidence

Add a Windows-only probe using the existing CI workflow opt-in
`check_windows_azure_msi` input. Use `cloud::fetch` with the existing
TLS-required synthetic target and `version --output json`. Verify installation
layout and version from local MSI metadata. Classify the bare name and use the
fixed package interpreter to prove a shell-free configured route.

Build the test with the locked workspace toolchain before launching it in
a fresh process. Allow only necessary OS and test environment values. Give
Azure CLI temporary configuration, profile, extension and working directories.
Use a local version cache and disable telemetry, auto-upgrade and dynamic
extension installation. Route the reviewed HTTP client to a non-forwarding
loopback endpoint. Retain only the runner, source, route and version evidence.

Check workflow and script syntax locally. For the opted-in CI run require
both Windows test receipts and the installed version match. Verify the pull request
and the exact main revision with CI and CodeQL. Live sign-in and PostgreSQL
evidence remain independent of the launch probe.

## Windows x64 built-in Entra MSI launcher

Preserve exact registry provenance at provider selection and TLS enforcement
before process creation. Try one native spawn. A missing-program result for the
built-in Entra route permits discovery under the x64 Program Files known folder.
Match the installed `wbin` lexically on PATH, then verify the fixed package
locations and reject path redirection within them. Use the bundled `python.exe`
with `-IBm azure.cli`, the existing argument elements and child-only
`AZ_INSTALLER=MSI`. Both launch choices share the existing timeout, output
extraction and redaction paths. Native spawn and fixed local metadata reads
are synchronous OS calls. Their elapsed time counts toward the deadline and
the resolver rechecks it before starting the fallback.

Retain the Windows evidence from T001 as the failing baseline. Exercise
native success, spawn refusal, absent and invalid package layout, configured
overrides, and a started child that fails. Run the opt-in hosted version probe
through the runtime's fallback policy on the registered x64 MSI. Record both
the selected route and its installed version match on the exact source.
