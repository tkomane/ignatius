use super::*;

#[test]
fn rooted_paths_stay_inside_the_given_root() {
    let root = Path::new("/tmp/ignatius-test-root");
    let paths = Paths::rooted_at(root);
    for (_, path) in paths.labelled() {
        assert!(
            path.starts_with(root),
            "{} escaped the root",
            path.display()
        );
    }
    assert_eq!(paths.config_file, root.join("config.toml"));
}

#[test]
fn resolved_paths_are_absolute_and_named_for_the_product() {
    let paths = Paths::resolve();
    assert!(
        paths.config_dir.is_absolute() || non_empty_env(CONFIG_DIR_ENV).is_some(),
        "config dir should be absolute unless explicitly overridden"
    );
    assert!(paths.config_file.ends_with("config.toml"));
    assert!(paths.log_dir.starts_with(&paths.data_dir));
}

#[test]
fn every_path_carries_a_human_label() {
    let paths = Paths::rooted_at(Path::new("/tmp/x"));
    let labels = paths.labelled();
    assert_eq!(labels.len(), 6);
    for (label, _) in &labels {
        assert!(!label.is_empty());
    }
    assert!(
        labels
            .iter()
            .any(|(label, _)| *label == "Statement history"),
        "a file of SQL on the disk must be listed where the user looks"
    );
}
