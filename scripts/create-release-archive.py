#!/usr/bin/env python3
"""Create a one-binary release archive with stable metadata."""

import argparse
import gzip
import io
import os
import stat
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path


ARCHIVE_FORMATS = ("tar.gz", "zip")
FIXED_MODE = stat.S_IFREG | 0o755
USTAR_MEMBER_NAME_BYTES = 100
WINDOWS_RESERVED_BASENAMES = frozenset(
    (
        "CON",
        "PRN",
        "AUX",
        "NUL",
        *(f"COM{number}" for number in range(1, 10)),
        *(f"LPT{number}" for number in range(1, 10)),
    )
)


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format", choices=ARCHIVE_FORMATS, required=True)
    parser.add_argument("--source-dir", type=Path, required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def fail(message):
    raise SystemExit("release archive: " + message)


def is_trusted_system_alias(path):
    """Allow only verified system aliases used by temporary directories."""
    if sys.platform == "darwin":
        aliases = {
            Path("/etc"): Path("/private/etc"),
            Path("/tmp"): Path("/private/tmp"),
            Path("/var"): Path("/private/var"),
        }
        target = aliases.get(path)
        if target is None:
            return False
    elif os.name == "posix" and path in (Path("/tmp"), Path("/var")):
        # A few Unix layouts expose a system temporary root through a symlink.
        # It is safe to recognise only these root-owned locations and only when
        # their resolved target is still a directory.
        try:
            return path.resolve(strict=False).is_dir()
        except (OSError, RuntimeError):
            return False
    else:
        return False
    try:
        return path.resolve(strict=False) == target
    except (OSError, RuntimeError):
        return False


def is_path_redirector(path):
    """Return whether a path component can redirect traversal elsewhere."""
    if path.is_symlink():
        return not is_trusted_system_alias(path)

    is_junction = getattr(path, "is_junction", None)
    if is_junction is not None:
        try:
            if is_junction():
                return True
        except OSError:
            return True

    # Python versions before Path.is_junction() still expose Windows reparse
    # points through lstat(). Rejecting the attribute is deliberately
    # conservative: a reparse point is not a stable ordinary directory for a
    # release path, even when its specific tag is unavailable here.
    try:
        attributes = os.lstat(path).st_file_attributes
    except AttributeError:
        return False
    except FileNotFoundError:
        return False
    except OSError:
        return True
    return bool(attributes & 0x0400)  # FILE_ATTRIBUTE_REPARSE_POINT


def reject_symlinked_ancestors(path, label):
    """Reject redirecting path components without resolving the path first."""
    candidate = Path(path)
    if not candidate.is_absolute():
        candidate = Path.cwd() / candidate

    normalized_candidate = os.path.normcase(os.path.normpath(os.fspath(candidate)))
    current = Path(candidate.anchor)
    for component in candidate.parts[1:]:
        if component in ("", "."):
            continue
        if component == "..":
            current = current.parent
            continue
        current /= component
        current_key = os.path.normcase(os.path.normpath(os.fspath(current)))
        if (current.is_symlink() and current_key == normalized_candidate) or is_path_redirector(
            current
        ):
            fail(f"{label} must not be a symlink or junction: {current}")


def validate_member_name(binary):
    if not binary or Path(binary).name != binary or binary in (".", ".."):
        fail("--binary must be one file name")
    if "\\" in binary or "/" in binary:
        fail("--binary must not contain a path separator")
    if any(ord(character) < 0x20 or ord(character) == 0x7F for character in binary):
        fail("--binary must not contain control characters")
    if any(character in '<>:"|?*' for character in binary):
        fail("--binary contains a character unsafe on Windows")
    if binary[-1] in (".", " "):
        fail("--binary must not end with a dot or space")
    try:
        encoded = binary.encode("utf-8")
    except UnicodeEncodeError:
        fail("--binary must be valid UTF-8")
    if len(encoded) > USTAR_MEMBER_NAME_BYTES:
        fail("--binary is too long for a portable tar member")
    if binary.split(".", 1)[0].upper() in WINDOWS_RESERVED_BASENAMES:
        fail("--binary is reserved on Windows")


def read_binary(source_dir, binary):
    validate_member_name(binary)
    if is_path_redirector(source_dir) or not source_dir.is_dir():
        fail("--source-dir must be a real directory")
    path = source_dir / binary
    if is_path_redirector(path) or not path.is_file():
        fail("source binary must be a regular, non-symlink file")
    return path.read_bytes()


def tar_info(binary, size):
    info = tarfile.TarInfo(binary)
    info.mode = FIXED_MODE & 0o7777
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    info.mtime = 0
    info.size = size
    return info


def write_tar_gz(output, binary, data):
    with gzip.GzipFile(
        fileobj=output, mode="wb", filename="", mtime=0, compresslevel=9
    ) as compressed:
        with tarfile.open(
            fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT
        ) as archive:
            archive.addfile(tar_info(binary, len(data)), io.BytesIO(data))


def write_zip(output, binary, data):
    info = zipfile.ZipInfo(binary, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.create_version = 20
    info.extract_version = 20
    info.flag_bits = 0
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = FIXED_MODE << 16
    info.internal_attr = 0
    info.extra = b""
    info.comment = b""
    with zipfile.ZipFile(
        output, mode="w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        archive.writestr(info, data)


def create_archive(archive_format, source_dir, binary, output):
    if archive_format not in ARCHIVE_FORMATS:
        fail("unsupported archive format")
    source_dir = Path(source_dir)
    output = Path(output)
    validate_member_name(binary)
    reject_symlinked_ancestors(source_dir, "source directory")
    reject_symlinked_ancestors(output.parent, "output parent directory")
    data = read_binary(source_dir, binary)
    if os.path.lexists(output):
        fail("refusing to overwrite an existing output")
    if not output.parent.is_dir():
        fail("output parent directory does not exist")
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=output.name + ".", suffix=".tmp", dir=output.parent
    )
    try:
        with os.fdopen(descriptor, "w+b") as temporary:
            if archive_format == "tar.gz":
                write_tar_gz(temporary, binary, data)
            else:
                write_zip(temporary, binary, data)
            temporary.flush()
            os.fsync(temporary.fileno())
        # Recheck the path after creating the temporary file. If an ancestor
        # was replaced during packaging, fail before installing any output.
        reject_symlinked_ancestors(output.parent, "output parent directory")
        if not output.parent.is_dir():
            fail("output parent directory does not exist")
        if os.path.lexists(output):
            fail("refusing to overwrite an existing output")
        # A hard link installs the completed temporary file only when the
        # destination is still absent. Unlike os.replace, it cannot silently
        # overwrite an output created after the initial existence check.
        os.link(temporary_name, output)
        os.unlink(temporary_name)
    except BaseException:
        try:
            os.unlink(temporary_name)
        except FileNotFoundError:
            pass
        raise


def main():
    args = parse_args()
    try:
        create_archive(args.format, args.source_dir, args.binary, args.output)
    except OSError as error:
        fail(str(error))
    print("Created deterministic archive: " + str(args.output))


if __name__ == "__main__":
    main()
