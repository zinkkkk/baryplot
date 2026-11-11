use super::*;
#[test]
fn last_hovered_set_stays_in_front_without_a_pin_and_pin_remains_optional() {
    let mut app = super::super::tests::app(Mode::Real);
    let samples = Arc::clone(&app.data.as_ref().expect("curve").samples);
    let revision = app.revision;
    for row in [0, 1] {
        let _ = app.update(Message::HoverEntry(row, true));
        let _ = app.update(Message::HoverEntry(row, false));
        assert_eq!(app.foreground_entry(), Some(row));
        assert_eq!(app.front_entry, None, "hover must not enable a pin");
    }
    let _ = app.update(Message::FrontEntry(2));
    assert_eq!(app.foreground_entry(), Some(2));
    let _ = app.update(Message::HoverEntry(0, true));
    assert_eq!(app.foreground_entry(), Some(0));
    let _ = app.update(Message::HoverEntry(0, false));
    assert_eq!(app.foreground_entry(), Some(2));
    let _ = app.update(Message::FrontEntry(2));
    assert_eq!(
        app.foreground_entry(),
        Some(0),
        "unpin falls back to last hover"
    );
    let _ = app.update(Message::BeginEntryDrag(1));
    let _ = app.update(Message::ReorderEntry(1, 2));
    let _ = app.update(Message::EndEntryDrag);
    assert!(
        app.front_entry.is_none(),
        "reordering must not silently enable a pin"
    );
    assert_eq!(app.revision, revision);
    assert!(Arc::ptr_eq(
        &samples,
        &app.data.as_ref().expect("curve").samples
    ));
    let _ = app.update(Message::Remove(0));
    assert_eq!(
        app.last_hovered_entry,
        Some(0),
        "removal preserves the raised row identity"
    );
}
#[test]
fn hover_pin_reorder_and_line_picker_do_not_resample_the_function() {
    let mut app = super::super::tests::app(Mode::Real);
    let samples = Arc::clone(&app.data.as_ref().expect("curve").samples);
    let revision = app.revision;
    for message in [
        Message::HoverEntry(1, true),
        Message::FrontEntry(2),
        Message::HoverEntry(1, false),
        Message::BeginEntryDrag(2),
        Message::ReorderEntry(2, 0),
        Message::EndEntryDrag,
    ] {
        let task = app.update(message);
        assert_eq!(task.units(), 0);
    }
    assert_eq!(app.project.ordered_entries(), vec![2, 0, 1]);
    assert_eq!(app.front_entry, Some(2));
    assert_eq!(app.revision, revision);
    assert!(Arc::ptr_eq(
        &samples,
        &app.data.as_ref().expect("curve").samples
    ));
    let model = app.project.compile().expect("ordered data");
    let before = model.evaluate(C::new(0.4, 0.)).expect("finite");
    let json = serde_json::to_string(&app.project).expect("save");
    let saved: Project = serde_json::from_str(&json).expect("load");
    assert_eq!(saved.ordered_entries(), vec![2, 0, 1]);
    assert_eq!(
        saved
            .compile()
            .expect("model")
            .evaluate(C::new(0.4, 0.))
            .expect("finite"),
        before
    );
    let original = app.project.settings.output_line.color;
    for action in [
        Action::Width(Line::Output, 4.),
        Action::Pick(picker::Target::Line(Line::Output)),
        Action::PickerEdit(picker::Edit::Swatch([1., 0., 0.])),
        Action::PickerCancel,
    ] {
        let task = app.update(Message::Appearance(action));
        assert_eq!(task.units(), 0);
    }
    assert_eq!(app.project.settings.output_line.color, original);
    assert_eq!(app.revision, revision);
    assert!(Arc::ptr_eq(
        &samples,
        &app.data.as_ref().expect("curve").samples
    ));
}
#[test]
fn exact_resolution_and_palette_changes_refresh_only_the_domain_and_keep_valid_drafts() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.busy = true;
    let samples = Arc::clone(&app.data.as_ref().expect("curve").samples);
    let revision = app.revision;
    let _ = app.update(Message::Appearance(Action::Number(
        Number::Resolution,
        "777".into(),
    )));
    assert_eq!(app.project.settings.resolution, 777);
    assert!(app.domain_pending);
    let _ = app.update(Message::Appearance(Action::Resize(true)));
    assert_eq!(app.project.settings.resolution, 1554);
    let _ = app.update(Message::Appearance(Action::Resize(false)));
    assert_eq!(app.project.settings.resolution, 777);
    let _ = app.update(Message::Appearance(Action::Number(
        Number::Resolution,
        "0".into(),
    )));
    assert_eq!(app.project.settings.resolution, 777);
    assert!(app.appearance_ui.errors.contains_key(&Number::Resolution));
    assert!(app.error.is_none());
    let _ = app.update(Message::Appearance(Action::Palette(Palette::Twilight)));
    assert_eq!(app.project.settings.colour_style.palette, Palette::Twilight);
    assert_eq!(app.revision, revision);
    assert!(Arc::ptr_eq(
        &samples,
        &app.data.as_ref().expect("curve").samples
    ));
}
#[test]
fn guide_styling_preserves_samples_and_raster_but_axis_mapping_invalidates_raster() {
    use crate::axes::{Guide, Scale};
    let mut app = super::super::tests::app(Mode::Complex);
    let samples = Arc::clone(&app.data.as_ref().unwrap().samples);
    let domain_revision = app.domain_revision;
    let revision = app.revision;
    for a in [
        guides::Action::StrokeDash(Guide::Circle, true),
        guides::Action::StrokeOpacity(Guide::Major, 0.5),
        guides::Action::Labels(false, false),
        guides::Action::Subdivisions(true, 8),
    ] {
        let _ = app.update(Message::Appearance(Action::Guide(a)));
    }
    let _ = app.update(Message::Appearance(Action::Pick(
        picker::Target::Background,
    )));
    let _ = app.update(Message::Appearance(Action::PickerEdit(
        picker::Edit::Swatch([1.; 3]),
    )));
    assert_eq!(app.project.settings.guides.background, [1.; 3]);
    let _ = app.update(Message::Appearance(Action::PickerCancel));
    assert_eq!(
        app.project.settings.guides.background,
        crate::axes::Guides::default().background
    );
    assert_eq!(app.domain_revision, domain_revision);
    assert_eq!(app.revision, revision);
    let _ = app.update(Message::Appearance(Action::Guide(guides::Action::Scale(
        true,
        Scale::Log10,
    ))));
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(app.project.settings.bounds.x_min > 0.);
    assert!(app.domain_revision > domain_revision);
    assert!(Arc::ptr_eq(&samples, &app.data.as_ref().unwrap().samples));
    let source = serde_json::to_string(&app.project).unwrap();
    let loaded: model::Project = serde_json::from_str(&source).unwrap();
    assert_eq!(loaded.settings.guides, app.project.settings.guides);
}
#[test]
fn original_colouring_restores_historical_shader_without_resampling_lines() {
    let mut app = super::super::tests::app(Mode::Complex);
    app.project.settings.colour_style.palette = Palette::Inferno;
    app.project.settings.colour_style.reverse = true;
    app.project.settings.colour_style.phase_offset = 90.;
    let samples = Arc::clone(&app.data.as_ref().unwrap().samples);
    let _ = app.update(Message::Appearance(Action::Classic));
    let c = app.project.settings.colour_style;
    assert_eq!(c.saturation, 0.72);
    assert!(c.bands);
    assert_eq!(c.contours, Contours::Off);
    assert_eq!(c.palette, Palette::Phase);
    assert!(c.shading);
    assert!(!c.reverse);
    assert_eq!(c.phase_offset, 0.);
    assert!(Arc::ptr_eq(&samples, &app.data.as_ref().unwrap().samples));
}
#[test]
fn clearing_and_rebuilding_path_keeps_colouring_independent_and_rows_clear_completely() {
    let mut app = super::super::tests::app(Mode::Complex);
    let domain = app.domain_revision;
    let _ = app.update(Message::ClearPath);
    assert!(app.error.is_none());
    assert!(app.control_drafts.is_empty());
    assert!(app.data.as_ref().unwrap().samples.is_empty());
    assert_eq!(app.domain_revision, domain);
    for _ in 0..3 {
        let _ = app.update(Message::AddControl);
    }
    assert_eq!(app.control_drafts.len(), 3);
    assert!(!app.data.as_ref().unwrap().samples.is_empty());
    let _ = app.update(Message::Plot(Interaction::RemoveControl(1)));
    assert_eq!(app.control_drafts.len(), 2);
    assert_eq!(app.project.settings.input.curve_weights.len(), 2);
    let _ = app.update(Message::Plot(Interaction::RemoveControl(1)));
    assert!(app.error.is_none());
    assert!(app.data.as_ref().unwrap().samples.is_empty());
    let _ = app.update(Message::HoverEntry(2, true));
    let _ = app.update(Message::ClearEntries);
    assert!(app.project.entries.is_empty());
    assert!(app.points.is_empty());
    assert!(app.data.is_none());
    assert_eq!(app.foreground_entry(), None);
    let _ = app.update(Message::Add);
    assert!(app.error.is_none());
    assert_eq!(app.project.entries.len(), 1);
}
#[test]
fn older_projects_keep_original_colouring_and_linear_guide_defaults() {
    let mut saved = serde_json::to_value(model::Project::default()).unwrap();
    let settings = saved["settings"].as_object_mut().unwrap();
    settings.remove("colour_style");
    settings.remove("guides");
    settings.remove("grid_y_step");
    let p: model::Project = serde_json::from_value(saved).unwrap();
    assert_eq!(p.settings.colour_style, style::ColourStyle::classic());
    assert_eq!(p.settings.guides, crate::axes::Guides::default());
    assert_eq!(p.settings.grid_y_step, "same");
}
#[test]
fn restored_colouring_remains_editable_and_invalid_band_fields_keep_last_valid_values() {
    let mut app = super::super::tests::app(Mode::Complex);
    let samples = Arc::clone(&app.data.as_ref().unwrap().samples);
    let _ = app.update(Message::Appearance(Action::Classic));
    let z = Some(C::new(1.7, 0.6));
    let original = style::Shader::new(app.project.settings.colour_style).color(z);
    for action in [
        Action::Saturation(0.4),
        Action::ShadingFloor(0.2),
        Action::BandFloor(0.3),
        Action::BandStrength(0.5),
        Action::Number(Number::BandBase, "a+1".into()),
        Action::Number(Number::BandExponent, "2".into()),
    ] {
        let _ = app.update(Message::Appearance(action));
        assert!(app.error.is_none());
    }
    let c = app.project.settings.colour_style;
    assert_eq!(c.band_base, 2.5);
    assert_eq!(c.band_exponent, 2.);
    assert_ne!(style::Shader::new(c).color(z), original);
    let _ = app.update(Message::Appearance(Action::Number(
        Number::BandBase,
        "1".into(),
    )));
    assert_eq!(app.project.settings.colour_style.band_base, 2.5);
    assert!(app.appearance_ui.errors.contains_key(&Number::BandBase));
    let _ = app.update(Message::Appearance(Action::BandFloor(0.9)));
    assert!(app.project.settings.colour_style.valid());
    assert!(Arc::ptr_eq(&samples, &app.data.as_ref().unwrap().samples));
    let json = serde_json::to_string(&app.project).unwrap();
    let restored: model::Project = serde_json::from_str(&json).unwrap();
    assert_eq!(
        restored.settings.colour_style,
        app.project.settings.colour_style
    );
}
