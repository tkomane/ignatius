#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

EXPECTED_VERSION = '0.1.0'
CHECKS: list[dict] = []
REVISIONS = (
    ('4cdfa5c273faae12826e7bcd96c6c37af29ef16c', 'ci/44605153852-1', 9),
    ('bde6711802fe4c74c1330e4fe300029853d3a76e', 'ci/44655874167-1', 12),
)
ARCHIVES = {
    'aarch64-apple-darwin': ('tar.gz', (
        'c42a38f87ec87ca1f090dbb0ef9dab5b2be8282aedbdc439e128e045f464ec00',
        '0c67c03604f48451138ce2167e7d6db8f50b4f56f16c49339f7ab76ff814cbb8')),
    'x86_64-unknown-linux-gnu': ('tar.gz', (
        '5e23548a4c95c76cee50a06bdb575464494b46da370e1a6172c56079df9331b9',
        'cd1d9575c5b8101759ab36fc70da098ed52a36deb0726d22ab239adb06703509')),
    'x86_64-pc-windows-msvc': ('zip', (
        '5d1b4047cee4a24cca401628d3726ef79d128e3b685e8d46f785385246c18af5',
        '3e6f28b5350d0c85ddaa2db58a54a9bca9ee43ba6a1f1813cdcef04d7da3e830')),
}

def load_smoke(repo: Path):
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location('release_runtime_smoke', repo / 'scripts/release-runtime-smoke.py')
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def ensure(ok: bool, description: str) -> None:
    if not ok:
        raise RuntimeError(description)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(binary: Path, args: list[str], env: dict[str, str], cwd: Path, check: str,
        expected: int = 0) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [str(binary), *args],
        cwd=cwd,
        env=env,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        encoding='utf-8',
        timeout=20,
        check=False,
    )
    ensure(len(result.stdout) + len(result.stderr) <= 65536, f'{check} exceeded the output bound')
    CHECKS.append({'check': check, 'exit_code': result.returncode, 'pass': result.returncode == expected})
    ensure(result.returncode == expected, f'{check} exited {result.returncode}, expected {expected}')
    return result


def verified_archive(inputs: tuple, label: str, root: Path, env: dict[str, str], target: str,
                     verifier: Path, smoke) -> tuple[Path, dict]:
    bundle, source, build, expected_archive_hash, files = inputs
    archive_name = f'ignatius-0.1.0-{target}.{ARCHIVES[target][0]}'
    executable = 'ignatius.exe' if target == 'x86_64-pc-windows-msvc' else 'ignatius'
    record = bundle / 'record/release-record.json'
    manifest = bundle / 'evidence/release-manifest.json'
    checksums = bundle / 'evidence/SHA256SUMS'
    artefacts = bundle / 'artefacts'
    state = json.loads(record.read_text())['records']
    items = json.loads(manifest.read_text())['artefacts']
    ensure(len(state) == 1 and state[0]['source_revision'] == source and state[0]['build_identity'] == build,
           f'{label} candidate identity')
    ensure(state[0]['product_version'] == EXPECTED_VERSION and state[0]['status'] == 'blocked',
           f'{label} retained record must be a blocked 0.1.0 rehearsal')
    ensure(len(items) == 3 and len({entry['target'] for entry in items}) == 3,
           f'{label} aggregate has an exact three-target matrix')
    expected_scope = ['record/release-record.json', 'evidence/release-manifest.json', 'evidence/SHA256SUMS']
    for item in items:
        expected_scope += [f'artefacts/{item["name"]}', f'runtime/{item["target"]}.json']
        if files == 12:
            expected_scope += [f'evidence/{item["target"]}.cdx.json']
    ensure(len(expected_scope) == files, f'{label} allowlist count')
    allowlist = root / f'{label}-allowlist.json'
    allowlist.write_text(json.dumps({'schema_version': 1, 'files': sorted(expected_scope)}) + '\n')
    run(verifier, ['release', 'validate', str(record)], env, root, f'{label} record validation')
    run(verifier, ['release', 'evidence-scope', '--root', str(bundle), '--allowlist', str(allowlist)],
        env, root, f'{label} exact {files}-file readback')
    run(
        verifier,
        ['release', 'manifest', 'verify', '--record', str(record), '--artefact-dir', str(artefacts),
         '--manifest', str(manifest), '--checksums', str(checksums)],
        env, root, f'{label} manifest and three archive digests',
    )
    member = [m for m in items if m['target'] == target]
    ensure(len(member) == 1 and member[0]['name'] == archive_name, f'{label} exact native archive selection')
    archive = artefacts / archive_name
    ensure(sha256(archive) == expected_archive_hash == member[0]['checksum'], f'{label} frozen archive digest')
    binary = smoke.extract_archive(archive, executable, root / 'install' / source)
    binary_hash = sha256(binary)
    ensure(native_header(binary, target), f'{label} binary has the exact native executable architecture')
    CHECKS.append({'check': f'{label} binary architecture', 'pass': True})
    runtime = json.loads((bundle / f'runtime/{target}.json').read_text())
    ensure(runtime['source_revision'] == source and runtime['build_identity'] == build
           and runtime['archive_name'] == archive_name and runtime['target'] == target
           and runtime['status'] == 'pass', f'{label} hosted runtime receipt binding')
    if runtime.get('binary_sha256') is not None:
        ensure(runtime['binary_sha256'] == binary_hash, f'{label} exact executable hash vs hosted runtime')
    return binary, {
        'version': EXPECTED_VERSION,
        'source_revision': source,
        'build_identity': build,
        'target': target,
        'archive': archive_name,
        'archive_sha256': expected_archive_hash,
        'archive_size_bytes': archive.stat().st_size,
        'binary_sha256': binary_hash,
        'hosted_binary_hash_matched': (
            runtime['binary_sha256'] == binary_hash if 'binary_sha256' in runtime else None),
        'manifest_sha256': sha256(manifest),
        'checksums_sha256': sha256(checksums),
        'verified_aggregate_paths': files,
        'clean_blocked_rehearsal': True,
    }


def native_header(binary: Path, target: str) -> bool:
    with binary.open('rb') as f:
        prefix = f.read(64)
        if len(prefix) != 64:
            return False
        if target == 'aarch64-apple-darwin':
            return (prefix[:4] == bytes.fromhex('cffaedfe')
                    and int.from_bytes(prefix[4:8], 'little') == 0x0100000C
                    and int.from_bytes(prefix[12:16], 'little') == 2)
        if target == 'x86_64-unknown-linux-gnu':
            return (prefix[:6] == bytes.fromhex('7f454c460201')
                    and int.from_bytes(prefix[16:18], 'little') in {2, 3}
                    and int.from_bytes(prefix[18:20], 'little') == 62)
        if target != 'x86_64-pc-windows-msvc' or prefix[:2] != b'MZ':
            return False
        pe_offset = int.from_bytes(prefix[60:64], 'little')
        if not 64 <= pe_offset <= min(binary.stat().st_size - 26, 1024 * 1024):
            return False
        f.seek(pe_offset)
        pe = f.read(26)
        return (pe[:6] == bytes.fromhex('504500006486')
                and int.from_bytes(pe[22:24], 'little') & 2 != 0
                and int.from_bytes(pe[24:26], 'little') == 0x20B)

def executable_identity(binary: Path, expected: dict, env: dict[str, str], root: Path, label: str) -> dict:
    result = run(binary, ['version', '--verbose'], env, root, f'{label} verbose identity')
    lines = result.stdout.splitlines()
    ensure(lines[0] == f'Ignatius {EXPECTED_VERSION}', f'{label} product version')
    identity = dict(tuple(part.strip() for part in line.split(':', 1)) for line in lines[1:])
    for key, wanted in (
        ('source revision', expected['source_revision'] + ' (clean)'),
        ('build identity', expected['build_identity']),
        ('target', expected['target']),
    ):
        ensure(identity.get(key) == wanted, f'{label} embedded {key}')
    ensure(sha256(binary) == expected['binary_sha256'], f'{label} actual selected executable digest')
    return {'source_revision': expected['source_revision'], 'build_identity': identity['build identity'],
            'target': identity['target'], 'compiler': identity['compiler']}


def activate(path: Path, binary: Path) -> None:
    prepared = path.with_name(path.name + '.next')
    ensure(not prepared.exists() and not path.is_symlink(), 'owned scratch install switch')
    shutil.copy2(binary, prepared)
    digest = sha256(binary)
    ensure(sha256(prepared) == digest, 'prepared replacement matches the verified source')
    os.replace(prepared, path)
    ensure(sha256(path) == digest, 'active install path contains the selected exact executable')


def expect_doctor(binary: Path, label: str, env: dict[str, str], root: Path, smoke) -> dict:
    result = run(binary, ['doctor', '--json'], env, root, f'{label} doctor JSON')
    report = json.loads(result.stdout)
    smoke.verify_doctor(report, EXPECTED_VERSION)
    smoke.verify_privacy(f'{label} doctor JSON', result)
    return report['summary']


def experiment(root: Path, args: argparse.Namespace) -> dict:
    target = args.target
    machine = platform.machine().lower()
    compatible = {
        'aarch64-apple-darwin': platform.system() == 'Darwin' and machine == 'arm64',
        'x86_64-unknown-linux-gnu': platform.system() == 'Linux' and machine == 'x86_64',
        'x86_64-pc-windows-msvc': platform.system() == 'Windows' and machine in {'amd64', 'x86_64'},
    }
    ensure(compatible[target], 'native target host is required')
    smoke = load_smoke(Path(__file__).resolve().parents[2])
    _, digests = ARCHIVES[target]
    selected = [(bundle, revision, build, digest, scope)
                for bundle, (revision, build, scope), digest in
                zip((args.previous, args.candidate), REVISIONS, digests)]
    config_dir = root / 'config'
    data_dir = root / 'data'
    environment = {
        'PATH': os.defpath,
        'IGNATIUS_CONFIG_DIR': str(config_dir),
        'IGNATIUS_DATA_DIR': str(data_dir),
        'IGNATIUS_LOG': 'off',
        'PGPASSFILE': str(root / 'absent-passfile'),
    }
    if os.name == 'nt':
        environment['SystemRoot'] = os.environ['SystemRoot']
        environment['WINDIR'] = environment['SystemRoot']
        environment['PATH'] = str(Path(environment['SystemRoot']) / 'System32')
    previous, previous_identity = verified_archive(selected[0], 'previous', root, environment, target, args.xtask, smoke)
    candidate, candidate_identity = verified_archive(selected[1], 'candidate', root, environment, target, args.xtask, smoke)
    ensure(previous_identity['binary_sha256'] != candidate_identity['binary_sha256'], 'distinct executable bytes')

    launcher = root / 'bin' / candidate.name
    launcher.parent.mkdir()
    activate(launcher, previous)
    initial = executable_identity(launcher, previous_identity, environment, root, 'initial')
    paths = run(launcher, ['config', 'paths'], environment, root, 'initial isolated state paths')
    ensure(str(config_dir) in paths.stdout and str(data_dir) in paths.stdout, 'only the designated config/data paths')
    run(launcher, ['config', 'init'], environment, root, 'initial genuine config creation')
    run(launcher, ['config', 'validate'], environment, root, 'initial configuration validation')
    config = config_dir / 'config.toml'
    queries = config_dir / 'queries'
    queries.mkdir(exist_ok=True)
    (queries / 'rehearsal.sql').write_text('SELECT 42;\n')
    data_dir.mkdir(exist_ok=True)
    history = data_dir / 'history.jsonl'
    history.write_bytes(
        b'{"at":"2026-11-19T00:00:00+00:00","target":"test@127.0.0.1:5432/test",'
        b'"database":"test","environment":"development","sql":"SELECT 42","outcome":"succeeded",'
        b'"elapsed_ms":1}\n'
    )
    history.chmod(0o600)
    before = (smoke.tree_digest(config_dir), smoke.tree_digest(data_dir))
    file_digests = {'config.toml': sha256(config), 'history.jsonl': sha256(history),
                    'queries/rehearsal.sql': sha256(queries / 'rehearsal.sql')}
    backup = root / 'recoverable-state'
    shutil.copytree(config_dir, backup / 'config')
    shutil.copytree(data_dir, backup / 'data')
    ensure((smoke.tree_digest(backup / 'config'), smoke.tree_digest(backup / 'data')) == before,
           'protected full scratch backups match original state')

    proposed = executable_identity(candidate, candidate_identity, environment, root, 'candidate preflight')
    run(candidate, ['config', 'validate'], environment, root, 'candidate preflight retained-config validation')
    proposed_diagnostics = expect_doctor(candidate, 'candidate preflight', environment, root, smoke)
    ensure((smoke.tree_digest(config_dir), smoke.tree_digest(data_dir)) == before, 'candidate preflight did not alter state')
    activate(launcher, candidate)
    upgraded = executable_identity(launcher, candidate_identity, environment, root, 'after launcher replacement')
    ensure(upgraded == proposed, 'activated identity matches preflight')
    run(launcher, ['config', 'validate'], environment, root, 'activated candidate retained-config validation')
    ensure((smoke.tree_digest(config_dir), smoke.tree_digest(data_dir)) == before, 'activation preserved full state trees')

    # Inject one synthetic unsupported schema only after successful activation.
    rejected = root / 'schema-999.toml'
    rejected.write_text('schema_version = 999\n')
    rejected.chmod(0o600)
    os.replace(rejected, config)
    run(launcher, ['config', 'validate'], environment, root, 'active candidate schema-999 refusal', expected=3)
    run(launcher, ['doctor'], environment, root, 'active candidate failed-start doctor refusal', expected=3)
    ensure(sha256(history) == file_digests['history.jsonl'] and smoke.tree_digest(data_dir) == before[1],
           'controlled failure preserved exact history and data bytes')

    activate(launcher, previous)
    failed = root / 'recovery' / 'refused-config.toml'
    failed.parent.mkdir()
    os.replace(config, failed)
    restored = config_dir / 'config.toml.restore'
    shutil.copy2(backup / 'config/config.toml', restored)
    os.replace(restored, config)
    reverted = executable_identity(launcher, previous_identity, environment, root, 'after rollback')
    ensure(reverted == initial, 'rollback returned exact original identity')
    run(launcher, ['config', 'validate'], environment, root, 'rolled-back retained-config validation')
    rolled_back_diagnostics = expect_doctor(launcher, 'rollback', environment, root, smoke)
    ensure((smoke.tree_digest(config_dir), smoke.tree_digest(data_dir)) == before,
           'rollback preserved both complete state trees')
    ensure({ 'config.toml': sha256(config), 'history.jsonl': sha256(history),
            'queries/rehearsal.sql': sha256(queries / 'rehearsal.sql') } == file_digests,
           'rollback returned exact configuration, query and history bytes')
    ensure(sha256(previous) == previous_identity['binary_sha256'] and sha256(candidate) == candidate_identity['binary_sha256'],
           'both original verified binaries available and immutable through rollback')
    ensure(failed.read_text() == 'schema_version = 999\n', 'refused fixture available through verification')

    os_version = platform.mac_ver()[0] if platform.system() == 'Darwin' else platform.version()
    return {
        'schema_version': 1,
        'status': 'pass',
        'scope': 'Two distinct historical 0.1.0 archived source revisions, CLI install/replacement/rollback in isolated directories',
        'host': {'os': platform.system(), 'architecture': platform.machine(),
                 'os_version': os_version, 'kernel_release': platform.release()},
        'exact_inputs': {'previous': previous_identity, 'candidate': candidate_identity},
        'observations': {'initial': initial, 'activated': upgraded, 'rolled_back': reverted,
                         'candidate_doctor': proposed_diagnostics, 'rollback_doctor': rolled_back_diagnostics},
        'state': {'config_tree_sha256': before[0], 'data_tree_sha256': before[1], 'file_sha256': file_digests,
                  'histories_and_saved_queries_preserved': True, 'both_binary_slots_preserved_during_rehearsal': True},
        'checks': CHECKS,
        'evidence_limits': {
            'same_product_version': True, 'published_semantic_version_upgrade': False,
            'live_database_repeated': False, 'native_terminal_hand_use': False,
            'cryptographic_trust_or_publication': False,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser(description='Verify and replace the same owned install path using two frozen archives.')
    for argument in ('previous', 'candidate', 'xtask', 'output'):
        parser.add_argument(f'--{argument}', type=Path, required=True)
    parser.add_argument('--target', choices=ARCHIVES, required=True)
    args = parser.parse_args()
    result = args.output
    ensure(result.is_absolute() and result.parent.is_dir() and not result.parent.is_symlink() and not result.exists() and not result.is_symlink(),
           'fresh absolute receipt in the owned runner output directory')
    for value in (args.xtask, args.previous, args.candidate):
        ensure(value.is_absolute() and value.exists(), 'exact absolute evidence and verifier paths')
    with tempfile.TemporaryDirectory(prefix='owned-state-', dir=result.parent) as owned:
        receipt = experiment(Path(owned).resolve(), args)
    receipt['private_runtime_removed'] = not Path(owned).exists()
    ensure(receipt['private_runtime_removed'], 'owned rehearsal state cleaned up')
    with result.open('x', encoding='utf-8') as file:
        json.dump(receipt, file, indent=2)
        file.write('\n')
    print(f'{len(CHECKS)} checks: {receipt["status"]}')
    print(hashlib.sha256(result.read_bytes()).hexdigest())


if __name__ == '__main__':
    main()
