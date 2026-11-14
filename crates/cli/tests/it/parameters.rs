//! `bt-scene parameters`: every address of the scene, as a table or as the parameter format's JSON.

use std::collections::BTreeSet;

use crate::common::{Outcome, Project, bt, text};

#[test]
fn the_table_lists_every_address_of_the_scene_with_its_kind_unit_range_and_value() -> Outcome {
    let project = Project::new()?;
    let ran = bt(&["parameters", text(&project.file("scene.json"))]);
    assert_eq!((ran.code, ran.err.as_str()), (0, ""));
    let mut lines = ran.out.lines();
    let head: Vec<&str> = lines.next().unwrap_or_default().split_whitespace().collect();
    assert_eq!(head, ["ADDRESS", "KIND", "UNIT", "RANGE", "VALUE"]);
    let listed: BTreeSet<&str> = lines.filter_map(|line| line.split_whitespace().next()).collect();
    let expected = project.scene()?.parameters();
    let addresses: BTreeSet<&str> = expected.iter().map(|parameter| parameter.address.as_str()).collect();
    assert_eq!(listed, addresses);
    let opacity = ran
        .out
        .lines()
        .find(|line| line.starts_with("band.opacity "))
        .unwrap_or_default();
    let cells: Vec<&str> = opacity.split_whitespace().collect();
    assert_eq!(cells, ["band.opacity", "float", "fraction", "0..1", "0.82"]);
    Ok(())
}

#[test]
fn json_is_a_list_with_an_object_per_parameter_in_order() -> Outcome {
    let project = Project::new()?;
    let ran = bt(&["parameters", text(&project.file("scene.json")), "--json"]);
    assert_eq!((ran.code, ran.err.as_str()), (0, ""));
    let parsed: serde_json::Value = serde_json::from_str(&ran.out)?;
    let listed: Vec<&str> = parsed
        .as_array()
        .map(|all| all.iter().filter_map(|one| one["address"].as_str()).collect())
        .unwrap_or_default();
    let expected = project.scene()?.parameters();
    let addresses: Vec<&str> = expected.iter().map(|parameter| parameter.address.as_str()).collect();
    assert_eq!(listed, addresses);
    let opacity = parsed
        .as_array()
        .and_then(|all| all.iter().find(|one| one["address"] == "band.opacity"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        (
            opacity["kind"].as_str(),
            opacity["unit"].as_str(),
            opacity["min"].as_f64(),
            opacity["max"].as_f64()
        ),
        (Some("float"), Some("fraction"), Some(0.0), Some(1.0))
    );
    Ok(())
}

#[test]
fn a_scene_that_cannot_be_read_is_exit_1() -> Outcome {
    let project = Project::new()?;
    let ran = bt(&["parameters", text(&project.file("gone.json"))]);
    assert_eq!(ran.code, 1);
    assert!(ran.err.starts_with("error: "), "{}", ran.err);
    Ok(())
}
