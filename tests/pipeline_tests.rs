use std::io::Write;
use ud::core::model::Verdict;
use ud::core::pipeline::Pipeline;
use ud::ecosystems::cargo::CargoEcosystem;

#[tokio::test]
async fn test_pipeline_cargo_integration() {
    let temp_dir = tempfile::tempdir().unwrap();
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

    if let Verdict::Outdated { target } = verdict {
        assert_eq!(target.0, "1.0.219");
    } else {
        panic!("Expected Outdated verdict, got {:?}", verdict);
    }
}
