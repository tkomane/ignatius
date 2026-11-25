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
