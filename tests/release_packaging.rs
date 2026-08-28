use std::{fs, path::Path};

fn read(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

#[test]
fn release_builds_an_inno_setup_installer_instead_of_a_zip() {
    let workflow = read(".github/workflows/release.yml");
    assert!(workflow.contains("choco install innosetup"));
    assert!(workflow.contains("codex-weather-widget.iss"));
    assert!(workflow.contains("CodexWeatherWidgetSetup-$env:RELEASE_VERSION.exe"));
    assert!(!workflow.contains("Compress-Archive"));
    assert_eq!(workflow.matches("uses:").count(), 3);
}

#[test]
fn installer_is_per_user_and_contains_the_app_icon_and_quota_skill() {
    let installer = read("installer/windows/codex-weather-widget.iss");
    assert!(installer.contains("PrivilegesRequired=lowest"));
    assert!(installer.contains("DefaultDirName={localappdata}\\Programs\\CodexWeatherWidget"));
    assert!(installer.contains("SetupIconFile=..\\..\\assets\\icons\\codex-weather-widget.ico"));
    assert!(installer.contains("DestName: \"codex-weather-widget.exe\""));
    assert!(installer.contains(".codex\\skills\\quota"));
}

#[test]
fn application_icon_exists_and_is_an_icon_container() {
    let icon = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons/codex-weather-widget.ico"),
    )
    .expect("read application icon");
    assert!(icon.len() > 1_000);
    assert_eq!(&icon[..4], &[0, 0, 1, 0]);

    let frame_count = u16::from_le_bytes([icon[4], icon[5]]) as usize;
    assert_eq!(frame_count, 7);

    let sizes = (0..frame_count)
        .map(|index| {
            let entry = 6 + index * 16;
            let width = match icon[entry] {
                0 => 256,
                value => usize::from(value),
            };
            let height = match icon[entry + 1] {
                0 => 256,
                value => usize::from(value),
            };
            let bit_depth = u16::from_le_bytes([icon[entry + 6], icon[entry + 7]]);
            assert_eq!(width, height);
            assert_eq!(bit_depth, 32);
            width
        })
        .collect::<Vec<_>>();
    assert_eq!(sizes, [256, 128, 64, 48, 32, 24, 16]);
}
