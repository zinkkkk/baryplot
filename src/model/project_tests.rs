use super::*;
#[test]
fn example_projects_round_trip_and_validate() {
    for source in [
        include_str!("../../examples/real.json"),
        include_str!("../../examples/complex.json"),
        include_str!("../../examples/definitions.json"),
        include_str!("../../examples/aaa-real.json"),
        include_str!("../../examples/aaa-complex.json"),
        include_str!("../../examples/numeric-functions.json"),
        include_str!("../../examples/complex-zeros-real-mode.json"),
    ] {
        let project: Project = serde_json::from_str(source).expect("example JSON");
        let compiled = project.compile().expect("valid nodes");
        crate::sampling::plan(&project.settings, &compiled).expect("valid settings");
        let encoded = serde_json::to_string(&project).expect("serialize");
        let loaded: Project = serde_json::from_str(&encoded).expect("round trip");
        assert_eq!(loaded.entries[0].real_y, project.entries[0].real_y);
        assert_eq!(
            loaded.entries[0].expressions,
            project.entries[0].expressions
        );
    }
}
