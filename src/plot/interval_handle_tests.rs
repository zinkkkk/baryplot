use super::*;
#[test]
fn interval_handles_win_overlapping_nodes_in_input_editing_and_remain_editable_for_fits() {
    let project = crate::model::reference_project(Mode::Real);
    let model = project.compile().unwrap();
    let plan = crate::sampling::plan(&project.settings, &model).unwrap();
    let mut plot = Plot {
        project: &project,
        model: Some(&model),
        points: &model.nodes,
        plan: Some(&plan),
        data: None,
        revision: 1,
        draw_path: false,
        spectrum: None,
        spectrum_filter: Default::default(),
        editable_points: true,
        input_priority: true,
        active_path: 0,
        emphasized_entry: Some(0),
        highlighted_entry: None,
    };
    let a = area(Size::new(900., 700.));
    let b = project.settings.bounds;
    let p = screen(C::new(plan.paths[0].interval[0], 0.), b, a);
    assert_eq!(plot.hit(p, b, a), Some(Target::Interval(0)));
    plot.input_priority = false;
    assert_eq!(plot.hit(p, b, a), Some(Target::Parameter(0, 0)));
    plot.editable_points = false;
    assert_eq!(plot.hit(p, b, a), Some(Target::Interval(0)));
}
