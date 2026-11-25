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

## Windows x64 direct AWS CLI launch

Add the default-off `check_windows_aws_cli` dispatch to the existing Windows
job. Check the native AWS CLI v2 MSI executable and installation record. Keep
the built-in provider's program and raw extraction. Replace its argument tail
with `--version` and supply the synthetic TLS-required target. Launch the test
in a fresh allowlisted process environment with a private profile and working
directory, metadata disabled and the installed executable directory on PATH.

Check the exact workflow step and match the CLI version to the MSI version.
Record the source, runner image, architecture and direct launch result.
The regular PR and main CI and CodeQL checks remain the integration gates.

## Windows x64 Google Cloud CLI archive probe

Add the default-off `check_windows_gcloud_archive` dispatch to the existing
Windows job. Fetch the exact bundled-Python ZIP and verify Google's published
SHA-256 before extraction. Keep the archive and test profiles in fresh worker
directories. Supply both workers with bounded environments, timeouts and
isolated profiles. Disable credential use, telemetry, update checks and prompts.

Resolve the built-in `gcp` direct route for a synthetic TLS-required target.
Use only the fixed version command and read its version JSON if it starts.
In a separate control, use Google's packaged Windows launcher with that same
fixed command. Compare the version with the archive pin and record both
outcomes, the archive digest, runner image and OS build.
[Google's versioned archives and SHA-256 checksums](https://docs.cloud.google.com/sdk/docs/downloads-versioned-archives)

Add a separate control within the same opt-in job after the archive hash has
been verified. Start its bundled `python.exe` with the verified script
path, `-S` and the fixed `version --format=json` command. Reuse the worker's
bounded capture, isolated configuration and account-free environment. Require
the same pinned version and retain the observed stock and vendor-launcher
outcomes.

## Shared provider output limits

Drain stdout and stderr concurrently while waiting for the child. Retain
complete streams up to their respective byte limits and discard any stream
that overflows. Continue draining through the existing timeout so the
process can report its exit status. Handle an oversized successful stdout
as an authentication diagnostic before token decoding. Pass complete
failure stderr through the existing redactor and omit partial streams
from diagnostics. Apply this to direct launches and the Windows x64 MSI
fallback while preserving each route's launch and timeout ordering.

Verify byte boundaries and extraction decisions with small synthetic
in-memory streams. Re-run the provider unit tests and the disposable TLS
integration. Check the existing account-free Windows x64 Azure MSI launch
probe and standard PR and main CI and CodeQL. The limits cover retained
stream bytes.
