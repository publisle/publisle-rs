use assert_cmd::Command;
#[test]
fn math_is_network_and_font_independent() {
    let output = Command::new(assert_cmd::cargo::cargo_bin!("publisle-rs"))
        .args(["math", r"\hat{x}", "--mathml"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(output).unwrap().contains("<mover"));
}
#[test]
fn execution_commands_and_missing_font_manifests_fail() {
    Command::new(assert_cmd::cargo::cargo_bin!("publisle-rs"))
        .args(["math", r"\input{secret}", "--mathml"])
        .assert()
        .failure();
    Command::new(assert_cmd::cargo::cargo_bin!("publisle-rs"))
        .env_remove("PUBLISLE_NATIVE_FONTS")
        .args(["fonts", "check"])
        .assert()
        .failure();
}
#[test]
fn font_check_reads_a_real_manifest_from_the_environment() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("PUBLISLE_TEST_FONT_DIR").expect("Pinned test fonts required"),
    );
    let path = std::env::temp_dir().join(format!("publisle-cli-fonts-{}.json", std::process::id()));
    std::fs::write(
        &path,
        serde_json::to_vec(
            &serde_json::json!([{"path":directory.join("DejaVuSerif.ttf"),"role":"regular"}]),
        )
        .unwrap(),
    )
    .unwrap();
    Command::new(assert_cmd::cargo::cargo_bin!("publisle-rs"))
        .env("PUBLISLE_NATIVE_FONTS", &path)
        .args(["fonts", "check"])
        .assert()
        .success();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn pdf_compiles_resolved_json_and_refuses_to_overwrite_output() {
    let fonts = std::path::PathBuf::from(
        std::env::var_os("PUBLISLE_TEST_FONT_DIR").expect("Pinned fonts required"),
    );
    let directory = std::env::temp_dir().join(format!(
        "publisle-cli-pdf-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let input = directory.join("input.json");
    let manifest = directory.join("fonts.json");
    let output = directory.join("output.pdf");
    std::fs::write(&input, serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"template":"article","templateVersion":"1","article":{"title":"CLI test","authors":["Ada"],"blocks":[]},"citations":[],"bibliography":[],"assets":[],"fonts":[]})).unwrap()).unwrap();
    std::fs::write(
        &manifest,
        serde_json::to_vec(
            &serde_json::json!([{"path":fonts.join("DejaVuSerif.ttf"),"role":"regular"}]),
        )
        .unwrap(),
    )
    .unwrap();
    Command::new(assert_cmd::cargo::cargo_bin!("publisle-rs"))
        .arg("pdf")
        .arg(&input)
        .arg(&output)
        .arg("--fonts")
        .arg(&manifest)
        .assert()
        .success();
    let original = std::fs::read(&output).unwrap();
    assert!(original.starts_with(b"%PDF-"));
    Command::new(assert_cmd::cargo::cargo_bin!("publisle-rs"))
        .arg("pdf")
        .arg(&input)
        .arg(&output)
        .arg("--fonts")
        .arg(&manifest)
        .assert()
        .failure();
    assert_eq!(std::fs::read(&output).unwrap(), original);
    std::fs::remove_dir_all(directory).unwrap();
}
