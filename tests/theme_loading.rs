//! Theme loading + seeding via the user's themes directory.

mod common;
use ferrosonic::ui::theme::{load_themes, seed_default_themes, ThemeData};
use serial_test::serial;

#[test]
fn default_theme_is_always_present() {
    let _td = common::tempdir();
    let themes = load_themes();
    assert!(
        themes.iter().any(|t| t.name == "Default"),
        "load_themes must always include Default"
    );
}

#[test]
fn default_theme_has_all_colors_set() {
    let t = ThemeData::default_theme();
    assert_eq!(t.name, "Default");
    assert_eq!(t.cava_gradient.len(), 8);
    assert_eq!(t.cava_horizontal_gradient.len(), 8);
}

#[test]
#[serial]
fn seed_default_themes_writes_files_then_load_picks_them_up() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    let themes_dir = dir.path().join("themes");
    seed_default_themes(&themes_dir);

    let entries: Vec<_> = std::fs::read_dir(&themes_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
        .collect();
    assert!(
        !entries.is_empty(),
        "seed must write at least one toml file"
    );

    let themes = load_themes();
    assert!(
        themes.len() > 1,
        "expected Default + seeded themes; got {}",
        themes.len()
    );
}

#[test]
#[serial]
fn corrupt_theme_toml_is_logged_and_skipped() {
    let dir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", dir.path());
    let themes_dir = dir.path().join("themes");
    std::fs::create_dir_all(&themes_dir).unwrap();
    std::fs::write(themes_dir.join("broken.toml"), "[[ this is not toml").unwrap();

    let themes = load_themes();
    assert!(
        themes.iter().any(|t| t.name == "Default"),
        "Default still loads even if a sibling theme is corrupt"
    );
    assert!(
        !themes.iter().any(|t| t.name == "Broken"),
        "corrupt file must not produce a ThemeData entry"
    );
}
