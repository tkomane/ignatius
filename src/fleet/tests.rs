use super::*;
use crate::snapshot::ViewSnapshot;

fn observation(profile: &str) -> TargetProbe {
    TargetProbe {
        profile: profile.to_owned(),
        environment: "test",
        result: Ok(Observation {
            health: Health {
                server_version_num: 180_007,
                in_recovery: false,
                read_only: true,
                tls: TlsObservation::Disabled,
                sslmode: "disable",
            },
            schema: Ok(SchemaSnapshot::new(None).finalize()),
            extensions: Ok(BTreeMap::from([("plpgsql".to_owned(), "1.0".to_owned())])),
            settings: Ok(BTreeMap::from([(
                "work_mem".to_owned(),
                Setting {
                    value: "4096".to_owned(),
                    unit: Some("kB".to_owned()),
                },
            )])),
        }),
    }
}

#[test]
fn compares_dimensions_and_omits_raw_schema_contents() {
    let baseline = observation("dev");
    let mut staging = observation("staging");
    let observed = staging.result.as_mut().unwrap();
    let schema = observed.schema.as_mut().unwrap();
    schema.views.push(ViewSnapshot {
        schema: "public".to_owned(),
        name: "private_object".to_owned(),
        materialized: false,
        definition_sha256: "fixture-definition-hash".to_owned(),
    });
    *schema = std::mem::take(schema).finalize();
    let extensions = observed.extensions.as_mut().unwrap();
    extensions.insert("plpgsql".to_owned(), "1.2".to_owned());
    extensions.insert("hstore".to_owned(), "1.5".to_owned());
    let settings = observed.settings.as_mut().unwrap();
    settings.get_mut("work_mem").unwrap().value = "8192".to_owned();

    let report = FleetReport::build(&baseline, &[staging]);
    assert!(report.has_findings());
    assert!(report.complete);
    assert!(report.drift);
    let json = serde_json::to_value(&report).unwrap();
    let comparison = &json["comparisons"][0];
    assert_eq!(comparison["schema"]["detail"]["info"], 1);
    assert_eq!(comparison["extensions"]["detail"][0]["name"], "hstore");
    assert!(comparison["extensions"]["detail"][0]["baseline"].is_null());
    assert_eq!(comparison["extensions"]["detail"][1]["baseline"], "1.0");
    assert_eq!(comparison["extensions"]["detail"][1]["candidate"], "1.2");
    assert_eq!(
        comparison["settings"]["detail"][0]["candidate"]["value"],
        "8192"
    );
    assert!(report.render_plain().contains("4096 kB -> 8192 kB"));
    for text in [
        json.to_string(),
        report.render_plain(),
        report.render_table(),
    ] {
        assert!(!text.contains("private_object") && !text.contains("fixture-definition"));
    }
}

#[test]
fn reports_equal_prefixes_as_unavailable_and_keeps_healthy_targets() {
    let baseline = observation("dev");
    let mut partial = observation("staging");
    let observed = partial.result.as_mut().unwrap();
    observed.schema.as_mut().unwrap().truncated = true;
    observed.extensions = Err(Issue::PermissionDenied);
    let failed = TargetProbe {
        profile: "down".to_owned(),
        environment: "test",
        result: Err(Issue::Timeout),
    };
    let candidates = [partial, failed, observation("good")];
    let report = FleetReport::build(&baseline, &candidates);
    assert!(report.has_findings());
    assert!(!report.complete && !report.drift);
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["targets"][1]["status"], "partial");
    assert_eq!(json["targets"][2]["status"], "unavailable");
    assert_eq!(json["targets"][2]["issue"], "timeout");
    let comparison = &json["comparisons"][0];
    assert_eq!(comparison["schema"]["candidate_issue"], "bounded");
    assert!(comparison["schema"]["detail"].is_null());
    assert_eq!(
        comparison["extensions"]["candidate_issue"],
        "permission-denied"
    );
    assert_eq!(comparison["settings"]["status"], "match");
    assert_eq!(json["comparisons"][2]["schema"]["status"], "match");
}
