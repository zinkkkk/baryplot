use super::*;
#[test]
fn legacy_combined_expressions_split_without_losing_live_dependencies() {
    let mut project = crate::model::reference_project(crate::model::Mode::Real);
    project.settings.mode = Mode::Complex;
    project.entries[0].expressions[1] = "a+2i".into();
    project.split_components();
    assert_eq!(
        project.compile().expect("split").nodes[0].value,
        C::new(2., 2.)
    );
    project.globals = "a = 3".into();
    assert_eq!(
        project.compile().expect("dependency").nodes[0].value,
        C::new(3., 2.)
    );
    project.settings.mode = Mode::Real;
    assert_eq!(
        project.compile().expect("real projection").nodes[0].value,
        C::new(3., 0.)
    );
    let json = serde_json::to_string(&project).expect("save");
    let mut restored: Project = serde_json::from_str(&json).expect("load");
    restored.settings.mode = Mode::Complex;
    assert_eq!(
        restored.compile().expect("restore imaginary").nodes[0].value,
        C::new(3., 2.)
    );
}
