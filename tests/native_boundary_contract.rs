//! Decision-independent boundary guard, not proof of native-driver parity.

use std::fs;
use std::path::{Path, PathBuf};

const PURE_LAYER_ROOTS: &[&str] = &["src/app", "src/query", "src/ui"];
const FORBIDDEN_MARKERS: &[&str] = &[
    "tokio_postgres",
    "tokio-postgres",
    "tokio_postgres_rustls",
    "tokio-postgres-rustls",
    "libpq",
    "pq_sys",
    "libpq_sys",
    "extern",
    "unsafe",
];

#[test]
fn pure_layers_do_not_contain_native_driver_or_ffi_markers() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut rust_files = Vec::new();

    for relative_root in PURE_LAYER_ROOTS {
        collect_rust_files(&repository.join(relative_root), &mut rust_files);
    }
    rust_files.sort();

    let mut violations = Vec::new();
    for path in rust_files {
        let source = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("read {}: {error}", path.display());
        });

        for marker in FORBIDDEN_MARKERS {
            if contains_marker(&source, marker) {
                violations.push(format!("{} contains `{marker}`", path.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "native driver or FFI details crossed the pure-layer boundary:\n{}",
        violations.join("\n")
    );
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory).unwrap_or_else(|error| {
        panic!("read {}: {error}", directory.display());
    });

    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!("read an entry under {}: {error}", directory.display());
        });
        let path = entry.path();
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!("inspect {}: {error}", path.display());
        });

        if file_type.is_dir() {
            collect_rust_files(&path, files);
        } else if file_type.is_file() && path.extension().is_some_and(|extension| extension == "rs")
        {
            files.push(path);
        }
    }
}

fn contains_marker(source: &str, marker: &str) -> bool {
    source.match_indices(marker).any(|(index, _)| {
        let before = source[..index].chars().next_back();
        let after = source[index + marker.len()..].chars().next();

        before.is_none_or(|character| !is_identifier_character(character))
            && after.is_none_or(|character| !is_identifier_character(character))
    })
}

fn is_identifier_character(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}
