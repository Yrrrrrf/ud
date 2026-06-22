use std::io::Write;
use tempfile::tempdir;
use ud::core::contract::Ecosystem;
use ud::core::model::{Coordinate, Verdict, Version};
use ud::core::pipeline::Pipeline;
use ud::ecosystems::cargo::CargoEcosystem;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// This test hits the live crates.io network — gate it with #[ignore].
/// Run explicitly with: cargo test -- --ignored
#[tokio::test]
#[ignore]
async fn test_pipeline_cargo_integration() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("Cargo.toml");
    let mut file = std::fs::File::create(&file_path).unwrap();
    writeln!(
        file,
        r#"
[dependencies]
serde = "1.0.0"
"#
    )
    .unwrap();

    let mut pipeline = Pipeline::new();
    pipeline.register(Box::new(CargoEcosystem::new()));

    let report = pipeline.run(&file_path).await.unwrap();

    assert_eq!(report.verdicts.len(), 1);
    let (dep, verdict) = &report.verdicts[0];
    assert_eq!(dep.coordinate.0, "serde");

    if let Verdict::Outdated {
        compatible, latest, ..
    } = verdict
    {
        assert!(latest.0.starts_with("1.0."));
        // serde 1.0.0 with ^1.0.0 constraint — compatible should exist
        assert!(compatible.is_some());
    } else {
        panic!("Expected Outdated verdict, got {:?}", verdict);
    }
}

#[tokio::test]
async fn test_mocked_source() {
    let mock_server = MockServer::start().await;

    // test-dep has length 8, so sparse path is te/st/test-dep
    Mock::given(method("GET"))
        .and(path("/te/st/test-dep"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"name":"test-dep","vers":"1.0.0","deps":[],"cksum":"123","features":{},"yanked":false}
{"name":"test-dep","vers":"1.1.0","deps":[],"cksum":"456","features":{},"yanked":false}
{"name":"test-dep","vers":"1.2.0-alpha.1","deps":[],"cksum":"789","features":{},"yanked":false}
{"name":"test-dep","vers":"1.1.5","deps":[],"cksum":"aaa","features":{},"yanked":true}
"#
        ))
        .mount(&mock_server)
        .await;

    let eco = CargoEcosystem::with_base_url(&mock_server.uri());
    let availability = eco
        .source(&Coordinate("test-dep".to_string()))
        .await
        .unwrap();

    assert_eq!(availability.versions.len(), 4);

    let v_1_0_0 = &availability.versions[0];
    assert_eq!(v_1_0_0.version.0, "1.0.0");
    assert!(!v_1_0_0.yanked);
    assert!(!v_1_0_0.prerelease);

    let v_1_1_0 = &availability.versions[1];
    assert_eq!(v_1_1_0.version.0, "1.1.0");
    assert!(!v_1_1_0.yanked);
    assert!(!v_1_1_0.prerelease);

    let v_1_2_0 = &availability.versions[2];
    assert_eq!(v_1_2_0.version.0, "1.2.0-alpha.1");
    assert!(!v_1_2_0.yanked);
    assert!(v_1_2_0.prerelease);

    let v_1_1_5 = &availability.versions[3];
    assert_eq!(v_1_1_5.version.0, "1.1.5");
    assert!(v_1_1_5.yanked);
    assert!(!v_1_1_5.prerelease);
}

#[tokio::test]
async fn test_pipeline_offline_mocked() {
    let mock_server = MockServer::start().await;

    // Mock index endpoints for:
    // - test-dep (outdated with compatible update: 1.0.0 -> 1.1.0)
    // - breaking-dep (outdated with only breaking update: ^0.4.0 -> 0.5.0)
    // - current-dep (current: 2.0.0)
    // - yanked-dep (yanked: 1.0.0 but 1.0.0 is the only version)

    // test-dep -> te/st/test-dep
    Mock::given(method("GET"))
        .and(path("/te/st/test-dep"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"name":"test-dep","vers":"1.0.0","deps":[],"cksum":"1","features":{},"yanked":false}
{"name":"test-dep","vers":"1.1.0","deps":[],"cksum":"2","features":{},"yanked":false}
"#,
        ))
        .mount(&mock_server)
        .await;

    // breaking-dep -> br/ea/breaking-dep
    Mock::given(method("GET"))
        .and(path("/br/ea/breaking-dep"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"name":"breaking-dep","vers":"0.4.0","deps":[],"cksum":"3","features":{},"yanked":false}
{"name":"breaking-dep","vers":"0.5.0","deps":[],"cksum":"4","features":{},"yanked":false}
"#
        ))
        .mount(&mock_server)
        .await;

    // current-dep -> cu/rr/current-dep
    Mock::given(method("GET"))
        .and(path("/cu/rr/current-dep"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"name":"current-dep","vers":"2.0.0","deps":[],"cksum":"5","features":{},"yanked":false}
"#
        ))
        .mount(&mock_server)
        .await;

    // yanked-dep -> ya/nk/yanked-dep
    Mock::given(method("GET"))
        .and(path("/ya/nk/yanked-dep"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"name":"yanked-dep","vers":"1.0.0","deps":[],"cksum":"6","features":{},"yanked":true}
"#
        ))
        .mount(&mock_server)
        .await;

    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("Cargo.toml");
    let mut file = std::fs::File::create(&file_path).unwrap();
    writeln!(
        file,
        r#"
[dependencies]
test-dep = "1.0.0"
breaking-dep = "0.4.0"
current-dep = "2.0.0"
yanked-dep = "1.0.0"
"#
    )
    .unwrap();

    let mut pipeline = Pipeline::new();
    pipeline.register(Box::new(CargoEcosystem::with_base_url(&mock_server.uri())));

    let report = pipeline.run(&file_path).await.unwrap();

    assert_eq!(report.verdicts.len(), 4);

    let test_verdict = report
        .verdicts
        .iter()
        .find(|(d, _)| d.coordinate.0 == "test-dep")
        .unwrap();
    assert_eq!(
        test_verdict.1,
        Verdict::Outdated {
            compatible: Some(Version("1.1.0".to_string())),
            latest: Version("1.1.0".to_string()),
            latest_pre: None,
        }
    );

    let breaking_verdict = report
        .verdicts
        .iter()
        .find(|(d, _)| d.coordinate.0 == "breaking-dep")
        .unwrap();
    assert_eq!(
        breaking_verdict.1,
        Verdict::Outdated {
            compatible: Some(Version("0.4.0".to_string())),
            latest: Version("0.5.0".to_string()),
            latest_pre: None,
        }
    );

    let current_verdict = report
        .verdicts
        .iter()
        .find(|(d, _)| d.coordinate.0 == "current-dep")
        .unwrap();
    assert_eq!(
        current_verdict.1,
        Verdict::Current {
            latest: Version("2.0.0".to_string()),
            latest_pre: None,
        }
    );

    let yanked_verdict = report
        .verdicts
        .iter()
        .find(|(d, _)| d.coordinate.0 == "yanked-dep")
        .unwrap();
    assert_eq!(
        yanked_verdict.1,
        // Because the only version was yanked, it is unsatisfiable or yanked.
        // Wait, resolve.rs handles empty parsed candidates (because it filters out yanked) by returning Verdict::Unsatisfiable.
        // Let's verify: Verdict::Unsatisfiable { constraint: Constraint("1.0.0") }
        Verdict::Unsatisfiable {
            constraint: ud::core::model::Constraint("1.0.0".to_string())
        }
    );
}

#[tokio::test]
async fn test_pipeline_offline_breadth() {
    let mock_server = MockServer::start().await;

    // Mock index endpoints for:
    // - ws-dep (workspace dependency) -> ws/-d/ws-dep
    // - linux-dep (target-specific dependency) -> li/nu/linux-dep
    // - err-dep (500 internal error dependency) -> er/r-/err-dep
    // - missing-dep (404 not found dependency) -> mi/ss/missing-dep

    Mock::given(method("GET"))
        .and(path("/ws/-d/ws-dep"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"name":"ws-dep","vers":"1.0.0","deps":[],"cksum":"1","features":{},"yanked":false}
{"name":"ws-dep","vers":"1.2.0","deps":[],"cksum":"2","features":{},"yanked":false}
"#,
        ))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/li/nu/linux-dep"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"name":"linux-dep","vers":"0.1.0","deps":[],"cksum":"3","features":{},"yanked":false}
{"name":"linux-dep","vers":"0.1.5","deps":[],"cksum":"4","features":{},"yanked":false}
"#
        ))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/er/r-/err-dep"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/mi/ss/missing-dep"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("Cargo.toml");
    let mut file = std::fs::File::create(&file_path).unwrap();
    writeln!(
        file,
        r#"
[workspace.dependencies]
ws-dep = "1.0.0"

[target.'cfg(target_os = "linux")'.dependencies]
linux-dep = "0.1.0"

[dependencies]
err-dep = "1.0.0"
missing-dep = "2.0.0"
"#
    )
    .unwrap();

    let mut pipeline = Pipeline::new();
    pipeline.register(Box::new(CargoEcosystem::with_base_url(&mock_server.uri())));

    let report = pipeline.run(&file_path).await.unwrap();

    assert_eq!(report.verdicts.len(), 4);

    let ws_verdict = report
        .verdicts
        .iter()
        .find(|(d, _)| d.coordinate.0 == "ws-dep")
        .unwrap();
    assert_eq!(
        ws_verdict.1,
        Verdict::Outdated {
            compatible: Some(Version("1.2.0".to_string())),
            latest: Version("1.2.0".to_string()),
            latest_pre: None,
        }
    );

    let linux_verdict = report
        .verdicts
        .iter()
        .find(|(d, _)| d.coordinate.0 == "linux-dep")
        .unwrap();
    assert_eq!(
        linux_verdict.1,
        Verdict::Outdated {
            compatible: Some(Version("0.1.5".to_string())),
            latest: Version("0.1.5".to_string()),
            latest_pre: None,
        }
    );

    let err_verdict = report
        .verdicts
        .iter()
        .find(|(d, _)| d.coordinate.0 == "err-dep")
        .unwrap();
    match &err_verdict.1 {
        Verdict::Errored(msg) => {
            assert!(msg.contains("HTTP error 500"));
        }
        other => panic!("Expected Errored verdict, got {:?}", other),
    }

    let missing_verdict = report
        .verdicts
        .iter()
        .find(|(d, _)| d.coordinate.0 == "missing-dep")
        .unwrap();
    assert_eq!(
        missing_verdict.1,
        // 404 from crates.io sparse index signifies 0 versions (availability default).
        // That results in Verdict::Unsatisfiable.
        Verdict::Unsatisfiable {
            constraint: ud::core::model::Constraint("2.0.0".to_string())
        }
    );
}
