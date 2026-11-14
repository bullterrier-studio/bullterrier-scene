//! Usage: `--version`, `--help`, and a wrong command line.

use crate::common::{Outcome, Project, bt, text};

#[test]
fn version_prints_the_packages_version_and_succeeds() {
    let ran = bt(&["--version"]);
    assert_eq!((ran.code, ran.out.trim(), ran.err.as_str()), (0, "bt-scene 9.9.9", ""));
}

#[test]
fn help_lists_the_commands_and_the_exit_codes_on_stdout() {
    let ran = bt(&["--help"]);
    assert_eq!(ran.code, 0);
    for word in ["render", "parameters", "validate", "exit codes:", "130"] {
        assert!(ran.out.contains(word), "{word}: {}", ran.out);
    }
}

#[test]
fn a_wrong_command_line_is_usage_error_2_explained_on_stderr() -> Outcome {
    let project = Project::new()?;
    let scene = text(&project.file("scene.json")).to_owned();
    let rows: [&[&str]; 10] = [
        &[],
        &["frobnicate"],
        &["render"],
        &["render", &scene],
        &["render", &scene, "out.png", "--nonsense"],
        &["render", &scene, "out.png", "--set", "no-equals-sign"],
        &["render", &scene, "out.png", "--set", "=5"],
        &["render", &scene, "out.png", "--size", "big"],
        &["parameters"],
        &["validate", &scene, "extra"],
    ];
    for args in rows {
        let ran = bt(args);
        assert_eq!(ran.code, 2, "{args:?}");
        assert_eq!(ran.out, "", "{args:?}");
        assert!(
            ran.err.contains("error") || ran.err.contains("Usage"),
            "{args:?}: {}",
            ran.err
        );
    }
    assert!(!std::path::Path::new("out.png").exists());
    Ok(())
}
