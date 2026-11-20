"""Private native PostgreSQL fixture for the owned Warp Integration test."""

from __future__ import annotations

from contextlib import contextmanager
from dataclasses import dataclass, field
import math
import os
from pathlib import Path
import secrets
import signal
import socket
import subprocess
import sys
import tempfile
import time
from typing import Iterator

MARKER = "warp_live_ok"
QUERY = """SELECT proof FROM warp_fixture.warp_acceptance
WHERE current_setting('transaction_read_only') = 'on'
  AND EXISTS (
    SELECT 1 FROM pg_catalog.pg_stat_ssl
    WHERE pid = pg_backend_pid() AND ssl AND version = 'TLSv1.3'
  );
"""

_VERSION = 180007
_DATABASE = "postgres"
_ADMIN = "warp_fixture_admin"
_READER = "warp_fixture_reader"
_APP = "ignatius_warp_fixture"
_SELFTEST = "ignatius_warp_fixture_setup"
_LISTEN = "127.0.0.1,::1"
_CLIENT_CHECK = f"""SELECT count(*) > 0 AND bool_and(COALESCE(
    s.ssl AND s.version = 'TLSv1.3'
    AND a.client_addr IN (inet '127.0.0.1', inet '::1'), false))
FROM pg_catalog.pg_stat_activity AS a
LEFT JOIN pg_catalog.pg_stat_ssl AS s USING (pid)
WHERE a.application_name = '{_APP}' AND a.usename = '{_READER}'
  AND a.datname = '{_DATABASE}' AND a.backend_type = 'client backend';
"""
_OPENSSL_CONFIG = """[req]
distinguished_name = dn
[dn]
[ca]
basicConstraints = critical,CA:TRUE
keyUsage = critical,keyCertSign,cRLSign
[server]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = DNS:localhost
"""


class FixtureError(RuntimeError):
    """A fixture failure safe to report without command output."""


def _absolute_tool(value: Path, label: str) -> Path:
    try:
        if value.is_absolute():
            tool = value.resolve(strict=True)
            if tool.is_file() and os.access(tool, os.X_OK):
                return tool
    except OSError:
        pass
    raise FixtureError(f"invalid absolute {label} executable")


def _write(path: Path, text: str) -> Path:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "w", encoding="ascii") as stream:
        stream.write(text)
    return path


def _command(
    argv: list[str | Path],
    env: dict[str, str],
    stage: str,
    timeout: float,
    sql: str | None = None,
    *,
    allow_failure: bool = False,
) -> str | None:
    try:
        result = subprocess.run(
            argv,
            input=sql.encode("ascii") if sql is not None else b"",
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            cwd=env["HOME"],
            env=env,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        raise FixtureError(f"{stage} timed out") from None
    except OSError:
        raise FixtureError(f"{stage} failed to start") from None
    if result.returncode:
        if allow_failure:
            return None
        raise FixtureError(f"{stage} failed")
    if len(result.stdout) > 32768:
        raise FixtureError(f"{stage} produced oversized output")
    try:
        return result.stdout.decode("ascii").strip()
    except UnicodeError:
        raise FixtureError(f"{stage} produced invalid output") from None


def _certificates(openssl: Path, work: Path, env: dict[str, str]) -> tuple[Path, Path, Path]:
    config = _write(work / "openssl.cnf", _OPENSSL_CONFIG)
    ca = work / "root.crt"
    ca_key = work / "root.key"
    cert = work / "server.crt"
    key = work / "server.key"
    request = work / "server.csr"
    isolated_env = dict(env, OPENSSL_CONF=str(config))
    commands = (
        ("req", "-new", "-x509", "-newkey", "rsa:2048", "-noenc", "-days", "1", "-sha256",
         "-config", config, "-extensions", "ca", "-subj", "/CN=Ignatius disposable Warp CA",
         "-keyout", ca_key, "-out", ca),
        ("req", "-new", "-newkey", "rsa:2048", "-noenc", "-sha256", "-config", config,
         "-subj", "/CN=localhost", "-keyout", key, "-out", request),
        ("x509", "-req", "-in", request, "-CA", ca, "-CAkey", ca_key, "-CAcreateserial",
         "-days", "1", "-sha256", "-extfile", config, "-extensions", "server", "-out", cert),
        ("verify", "-purpose", "sslserver", "-verify_hostname", "localhost", "-CAfile", ca, cert),
    )
    for argv in commands:
        _command([openssl, *argv], isolated_env, "fixture TLS certificate preparation", 12)
    key.chmod(0o600)
    ca_key.unlink()
    request.unlink()
    return ca, cert, key


def _loopback_port() -> int:
    # Both addresses must be available at selection. Readiness below connects
    # only through this cluster's private Unix directory.
    for _ in range(8):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as ipv4:
            ipv4.bind(("127.0.0.1", 0))
            port = ipv4.getsockname()[1]
            with socket.socket(socket.AF_INET6, socket.SOCK_STREAM) as ipv6:
                try:
                    ipv6.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 1)
                    ipv6.bind(("::1", port))
                except OSError:
                    continue
                return port
    raise FixtureError("could not reserve dual-stack loopback fixture addresses")


def _psql(psql: Path, connection: list[str], env: dict[str, str], stage: str,
          timeout: float, query: str, *, allow_failure: bool = False) -> str | None:
    argv = [psql, "-X", "-w", "-qAt", "-v", "ON_ERROR_STOP=1", *connection]
    return _command(argv, env, stage, timeout, query, allow_failure=allow_failure)


def _seed_sql(password: str) -> str:
    # The only dynamic value is a freshly generated hexadecimal password.
    # SQL is sent by stdin; neither arguments nor the server log carry it.
    return f"""BEGIN;
REVOKE ALL ON DATABASE postgres FROM PUBLIC;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
CREATE SCHEMA warp_fixture;
CREATE TABLE warp_fixture.warp_acceptance (proof text NOT NULL);
INSERT INTO warp_fixture.warp_acceptance VALUES ('{MARKER}');
CREATE ROLE {_READER} LOGIN NOINHERIT NOSUPERUSER NOCREATEDB NOCREATEROLE
    NOREPLICATION NOBYPASSRLS CONNECTION LIMIT 8 PASSWORD '{password}';
GRANT CONNECT ON DATABASE postgres TO {_READER};
GRANT USAGE ON SCHEMA warp_fixture TO {_READER};
GRANT SELECT ON warp_fixture.warp_acceptance TO {_READER};
ALTER ROLE {_READER} SET default_transaction_read_only = on;
ALTER ROLE {_READER} SET statement_timeout = '8s';
COMMIT;
"""


@dataclass(frozen=True)
class PostgresFixture:
    target: str
    environment: dict[str, str] = field(repr=False)
    version_num: int
    _psql_path: Path = field(repr=False)
    _admin_conn: list[str] = field(repr=False)
    _process_env: dict[str, str] = field(repr=False)
    _server: subprocess.Popen = field(repr=False)

    def wait_for_client(self, timeout: float) -> None:
        """Wait for the owned role and application name using real TLSv1.3."""
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError("fixture observation timeout must be finite and positive")
        deadline = time.monotonic() + timeout
        while (remaining := deadline - time.monotonic()) > 0:
            if self._server.poll() is not None:
                raise FixtureError("owned PostgreSQL exited during observation")
            matches = _psql(self._psql_path, self._admin_conn, self._process_env,
                            "fixture application observation", min(2.0, remaining), _CLIENT_CHECK)
            if matches == "t":
                return
            if matches != "f":
                raise FixtureError("unexpected fixture application observation")
            time.sleep(min(0.05, max(0, deadline - time.monotonic())))
        raise FixtureError("owned TLS application session was not observed")


def _stop(server: subprocess.Popen) -> None:
    if server.poll() is not None:
        return
    server.send_signal(signal.SIGINT)
    try:
        if server.wait(timeout=6) == 0:
            return
        raise FixtureError("owned PostgreSQL did not stop cleanly")
    except subprocess.TimeoutExpired:
        # PostgreSQL documents SIGQUIT as immediate shutdown. The fallback
        # targets the same Popen child, and the final group only its session.
        server.send_signal(signal.SIGQUIT)
    try:
        server.wait(timeout=3)
    except subprocess.TimeoutExpired:
        try:
            if os.getsid(server.pid) == server.pid:
                os.killpg(server.pid, signal.SIGKILL)
            else:
                server.kill()
        except ProcessLookupError:
            pass
        try:
            server.wait(timeout=2)
        except subprocess.TimeoutExpired:
            raise FixtureError("owned PostgreSQL did not exit") from None
    raise FixtureError("owned PostgreSQL required forced shutdown")


@contextmanager
def start_postgres() -> Iterator[PostgresFixture]:
    if sys.platform != "darwin" or os.geteuid() == 0:
        raise FixtureError("fixture requires a native unprivileged macOS process")
    pg_bin = Path(os.environ.get("IGNATIUS_WARP_PGBIN", ""))
    openssl = _absolute_tool(Path(os.environ.get("IGNATIUS_WARP_OPENSSL", "")), "OpenSSL")
    initdb = _absolute_tool(pg_bin / "initdb", "initdb")
    postgres = _absolute_tool(pg_bin / "postgres", "postgres")
    psql = _absolute_tool(pg_bin / "psql", "psql")

    # Short private path for Darwin's Unix-domain socket length limit.
    with tempfile.TemporaryDirectory(prefix="iwpg.", dir="/private/tmp") as private:
        work = Path(private)
        data = work / "cluster"
        sockets = work / "sock"
        sockets.mkdir(mode=0o700)
        env = {"PATH": "/usr/bin:/bin", "HOME": private, "LC_ALL": "C", "PGCONNECT_TIMEOUT": "2"}
        if _command([postgres, "--version"], env, "fixture version", 4) != "postgres (PostgreSQL) 18.7":
            raise FixtureError("fixture requires PostgreSQL 18.7")
        ca, cert, key = _certificates(openssl, work, env)
        _command([initdb, "--pgdata", data, "--username", _ADMIN, "--encoding", "UTF8",
                  "--locale", "C", "--auth-local=trust", "--auth-host=scram-sha-256"],
                 env, "fixture cluster initialization", 25)
        port = _loopback_port()
        hba = _write(work / "pg_hba.conf", f"""local {_DATABASE} {_ADMIN} trust
hostssl {_DATABASE} {_READER} 127.0.0.1/32 scram-sha-256
hostssl {_DATABASE} {_READER} ::1/128 scram-sha-256
""")
        config = _write(work / "postgresql.conf", f"""listen_addresses = '{_LISTEN}'
port = {port}
unix_socket_directories = '{sockets}'
unix_socket_permissions = 0700
hba_file = '{hba}'
ssl = on
ssl_cert_file = '{cert}'
ssl_key_file = '{key}'
ssl_min_protocol_version = 'TLSv1.3'
ssl_max_protocol_version = 'TLSv1.3'
password_encryption = 'scram-sha-256'
log_statement = 'none'
log_min_error_statement = 'panic'
log_error_verbosity = 'terse'
""")

        try:
            server = subprocess.Popen(
                [postgres, "-D", data, "-c", f"config_file={config}"],
                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                cwd=work, env=env, start_new_session=True,
            )
        except OSError:
            raise FixtureError("owned PostgreSQL failed to start") from None
        admin_conn = ["-h", str(sockets), "-p", str(port), "-d", _DATABASE, "-U", _ADMIN]
        try:
            deadline = time.monotonic() + 12
            readiness_sql = """SELECT current_setting('server_version_num') || '|' ||
current_setting('ssl') || '|' || current_setting('listen_addresses');"""
            while (remaining := deadline - time.monotonic()) > 0:
                if server.poll() is not None:
                    raise FixtureError("owned PostgreSQL did not start with the TLS configuration")
                readiness = _psql(psql, admin_conn, env, "private PostgreSQL readiness",
                                  min(2.0, remaining), readiness_sql, allow_failure=True)
                if readiness is not None:
                    if readiness != f"{_VERSION}|on|{_LISTEN}":
                        raise FixtureError("owned PostgreSQL does not match the fixture configuration")
                    break
                time.sleep(min(0.05, max(0, deadline - time.monotonic())))
            else:
                raise FixtureError("owned PostgreSQL did not become ready")

            password = secrets.token_hex(24)
            _psql(psql, admin_conn, env, "fixture role and data setup", 5, _seed_sql(password))
            passfile = _write(work / "pgpass", f"localhost:{port}:{_DATABASE}:{_READER}:{password}\n")
            environment = {"PGPASSFILE": str(passfile), "PGSSLROOTCERT": str(ca)}
            endpoint = f"postgresql://{_READER}@localhost:{port}/{_DATABASE}?sslmode=verify-full&connect_timeout=4"
            selftest = f"{endpoint}&application_name={_SELFTEST}"
            if _psql(psql, ["-d", selftest], dict(env, **environment), "fixture read-only verify-full check",
                     6, QUERY) != MARKER:
                raise FixtureError("fixture did not return the verified TLS read-only row")
            if _psql(psql, admin_conn, env, "fixture TLS session isolation", 3, _CLIENT_CHECK) != "f":
                raise FixtureError("fixture application session was present before the client started")
            fixture = PostgresFixture(f"{endpoint}&application_name={_APP}", environment, _VERSION,
                                      psql, admin_conn, env, server)
            yield fixture
            if server.poll() is not None:
                raise FixtureError("owned PostgreSQL exited before fixture teardown")
        finally:
            _stop(server)
