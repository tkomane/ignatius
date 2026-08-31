use serde_json::Value;

fn schema(text: &str) -> Value {
    serde_json::from_str(text).expect("release schema must be valid JSON")
}

#[test]
fn scope_and_manifest_entry_arrays_require_unique_items() {
    let scope = schema(include_str!("../release-evidence/scope.schema.json"));
    assert_eq!(
        scope["properties"]["files"]["uniqueItems"],
        Value::Bool(true),
        "scope paths must be unique before the executable checker reads them"
    );

    let manifest = schema(include_str!(
        "../release-notes/release-manifest.schema.json"
    ));
    assert_eq!(
        manifest["properties"]["artefacts"]["uniqueItems"],
        Value::Bool(true),
        "manifest artefact entries must reject exact duplicates at schema validation"
    );
}
