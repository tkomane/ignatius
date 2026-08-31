#![allow(clippy::print_stderr)]

use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{TempDir, tempdir};

fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/create-release-archive.py")
}

fn python_or_skip() -> Option<String> {
    let candidates = match env::var("IGNATIUS_PYTHON") {
        Ok(path) if !path.is_empty() => vec![path],
        _ => vec!["python3".to_owned(), "python".to_owned()],
    };

    for candidate in candidates {
        let Ok(output) = Command::new(&candidate).arg("--version").output() else {
            continue;
        };
        let version = String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr);
        if output.status.success() && version.starts_with("Python 3") {
            return Some(candidate);
        }
    }

    eprintln!("skipping: Python 3 is required for the archive helper contract");
    None
}

fn run_archive(
    python: &str,
    archive_format: &str,
    source_dir: &Path,
    binary: &str,
    output: &Path,
) -> Output {
    Command::new(python)
        .arg(script_path())
        .args(["--format", archive_format, "--source-dir"])
        .arg(source_dir)
        .args(["--binary", binary, "--output"])
        .arg(output)
        .output()
        .expect("run archive helper")
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        output_text(output)
    );
}

fn inspect_archive(python: &str, archive_format: &str, archive: &Path) -> Value {
    let inspector = r#"
import json
import sys
import tarfile
import zipfile

archive_format, archive_path = sys.argv[1:]
if archive_format == "tar.gz":
    with tarfile.open(archive_path, "r:gz") as handle:
        members = handle.getmembers()
        member = members[0]
        value = {
            "member_count": len(members),
            "name": member.name,
            "content": handle.extractfile(member).read().decode("ascii"),
            "regular": member.isfile(),
            "mode": member.mode,
            "uid": member.uid,
            "gid": member.gid,
            "mtime": member.mtime,
            "uname": member.uname,
            "gname": member.gname,
        }
else:
    with zipfile.ZipFile(archive_path) as handle:
        members = handle.infolist()
        member = members[0]
        value = {
            "member_count": len(members),
            "name": member.filename,
            "content": handle.read(member).decode("ascii"),
            "regular": not member.is_dir(),
            "date_time": list(member.date_time),
            "create_system": member.create_system,
            "external_mode": member.external_attr >> 16,
            "compression": member.compress_type,
            "flags": member.flag_bits,
            "extra": member.extra.hex(),
            "comment": member.comment.hex(),
        }
print(json.dumps(value, sort_keys=True))
"#;

    let output = Command::new(python)
        .args(["-c", inspector, archive_format])
        .arg(archive)
        .output()
        .expect("inspect archive");
    assert!(
        output.status.success(),
        "archive inspection failed: {}",
        output_text(&output)
    );
    serde_json::from_slice(&output.stdout).expect("archive inspection JSON")
}

fn source_and_output(temp: &TempDir) -> (PathBuf, PathBuf) {
    let source = temp.path().join("source");
    let output = temp.path().join("output");
    fs::create_dir(&source).expect("source directory");
    fs::create_dir(&output).expect("output directory");
    fs::write(source.join("ignatius"), b"deterministic payload\n").expect("source binary");
    (source, output)
}

#[test]
fn archives_are_byte_identical_and_have_fixed_root_member_metadata() {
    let Some(python) = python_or_skip() else {
        return;
    };
    let temp = tempdir().expect("temporary directory");
    let (source, output) = source_and_output(&temp);

    for (archive_format, suffix) in [("tar.gz", ".tar.gz"), ("zip", ".zip")] {
        let first = output.join(format!("first{suffix}"));
        let second = output.join(format!("second{suffix}"));
        let first_result = run_archive(&python, archive_format, &source, "ignatius", &first);
        assert!(
            first_result.status.success(),
            "{}",
            output_text(&first_result)
        );
        let second_result = run_archive(&python, archive_format, &source, "ignatius", &second);
        assert!(
            second_result.status.success(),
            "{}",
            output_text(&second_result)
        );
        assert_eq!(
            fs::read(&first).expect("first archive"),
            fs::read(&second).expect("second archive"),
            "{archive_format} bytes changed between identical preparations"
        );

        let metadata = inspect_archive(&python, archive_format, &first);
        assert_eq!(metadata["member_count"], json!(1));
        assert_eq!(metadata["name"], json!("ignatius"));
        assert_eq!(metadata["content"], json!("deterministic payload\n"));
        assert_eq!(metadata["regular"], json!(true));
        if archive_format == "tar.gz" {
            assert_eq!(metadata["mode"], json!(0o755));
            assert_eq!(metadata["uid"], json!(0));
            assert_eq!(metadata["gid"], json!(0));
            assert_eq!(metadata["mtime"], json!(0));
            assert_eq!(metadata["uname"], json!(""));
            assert_eq!(metadata["gname"], json!(""));
        } else {
            assert_eq!(metadata["date_time"], json!([1980, 1, 1, 0, 0, 0]));
            assert_eq!(metadata["create_system"], json!(3));
            assert_eq!(metadata["external_mode"], json!(0o100755));
            assert_eq!(metadata["compression"], json!(8));
            assert_eq!(metadata["flags"], json!(0));
            assert_eq!(metadata["extra"], json!(""));
            assert_eq!(metadata["comment"], json!(""));
        }
    }
}

#[test]
fn an_existing_output_is_preserved_and_invalid_members_are_refused() {
    let Some(python) = python_or_skip() else {
        return;
    };
    let temp = tempdir().expect("temporary directory");
    let (source, output) = source_and_output(&temp);
    let existing = output.join("existing.tar.gz");
    fs::write(&existing, b"do not replace").expect("existing output");
    let result = run_archive(&python, "tar.gz", &source, "ignatius", &existing);
    assert_failure(&result);
    assert_eq!(
        fs::read(&existing).expect("existing output bytes"),
        b"do not replace"
    );

    let mut invalid_members = vec![
        "../ignatius".to_owned(),
        r"..\ignatius".to_owned(),
        "CON".to_owned(),
        "ignatius.".to_owned(),
        "ignatius ".to_owned(),
        "bad\nname".to_owned(),
    ];
    invalid_members.push("a".repeat(101));

    for (index, binary) in invalid_members.iter().enumerate() {
        let output_path = output.join(format!("invalid-{index}.tar.gz"));
        let result = run_archive(&python, "tar.gz", &source, binary, &output_path);
        assert_failure(&result);
        assert!(
            !output_path.exists(),
            "invalid member {binary:?} created an archive"
        );
    }
}

#[cfg(unix)]
#[test]
fn source_and_output_redirectors_are_rejected_without_writing_through_them() {
    use std::os::unix::fs::symlink;

    let Some(python) = python_or_skip() else {
        return;
    };
    let temp = tempdir().expect("temporary directory");
    let (source, output) = source_and_output(&temp);

    let source_root = temp.path().join("source-root");
    fs::create_dir(&source_root).expect("source root");
    fs::rename(&source, source_root.join("bundle")).expect("move source bundle");
    let source_alias = temp.path().join("source-alias");
    symlink(&source_root, &source_alias).expect("source alias");
    let redirected_source = source_alias.join("bundle");
    let source_result = run_archive(
        &python,
        "tar.gz",
        &redirected_source,
        "ignatius",
        &output.join("source.tar.gz"),
    );
    assert_failure(&source_result);
    assert!(!output.join("source.tar.gz").exists());

    let real_parent = temp.path().join("real-parent");
    let nested_parent = real_parent.join("nested");
    fs::create_dir(&real_parent).expect("real output parent");
    fs::create_dir(&nested_parent).expect("nested output parent");
    let output_alias = temp.path().join("output-alias");
    symlink(&real_parent, &output_alias).expect("nested output alias");
    let nested_output = output_alias.join("nested/archive.tar.gz");
    let nested_result = run_archive(
        &python,
        "tar.gz",
        &source_root.join("bundle"),
        "ignatius",
        &nested_output,
    );
    assert_failure(&nested_result);
    assert!(!nested_parent.join("archive.tar.gz").exists());

    let direct_alias = temp.path().join("direct-output-alias");
    symlink(&real_parent, &direct_alias).expect("direct output alias");
    let direct_output = direct_alias.join("archive.tar.gz");
    let direct_result = run_archive(
        &python,
        "tar.gz",
        &source_root.join("bundle"),
        "ignatius",
        &direct_output,
    );
    assert_failure(&direct_result);
    assert!(!real_parent.join("archive.tar.gz").exists());

    let sentinel = temp.path().join("sentinel");
    fs::write(&sentinel, b"keep this target").expect("sentinel");
    let output_link = output.join("existing-link.tar.gz");
    symlink(&sentinel, &output_link).expect("output file symlink");
    let link_result = run_archive(
        &python,
        "tar.gz",
        &source_root.join("bundle"),
        "ignatius",
        &output_link,
    );
    assert_failure(&link_result);
    assert_eq!(
        fs::read(&sentinel).expect("sentinel bytes"),
        b"keep this target"
    );
    assert!(
        fs::symlink_metadata(&output_link)
            .expect("output link metadata")
            .file_type()
            .is_symlink()
    );
}
