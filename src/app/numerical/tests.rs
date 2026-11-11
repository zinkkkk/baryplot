use super::*;
use smol::stream::StreamExt;
fn messages(task: Task<Message>) -> Vec<Message> {
    let mut stream = iced_runtime::task::into_stream(task).expect("worker scheduled");
    smol::block_on(async {
        let mut out = Vec::new();
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Output(message) = action {
                out.push(message);
            } else {
                panic!("unexpected worker action");
            }
        }
        out
    })
}
#[test]
fn spectral_work_waits_for_rendering_coalesces_motion_and_never_rebuilds_curves() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    let _ = app.update(Message::Flag(Flag::Poles, true));
    let _ = app.update(Message::Flag(Flag::Zeros, true));
    let old = app.spectrum.generation;
    app.spectrum.busy = true; // Hold one old eigensolve while rendering newer inputs.
    app.spectrum.running_generation = Some(old);
    for i in 0..100 {
        let _ = app.update(Message::Plot(Interaction::Move(
            Target::Parameter(0, 1),
            C::new(1. + i as f64 / 100., 0.),
        )));
        assert_eq!(
            app.data.as_ref().expect("immediate curve").samples.len(),
            1600
        );
    }
    assert!(app.spectrum.pending.is_some());
    assert!(
        !app.spectrum.busy,
        "obsolete solve is stopped without waiting"
    );
    let samples = Arc::clone(&app.data.as_ref().expect("curve").samples);
    let _ = app.update(Message::Analyzed(old, Ok(Some(Spectrum::default()))));
    assert!(app.spectrum.result.is_none());
    assert!(!app.spectrum.busy);
    assert!(app.spectrum.pending.is_some());
    assert!(Arc::ptr_eq(
        &samples,
        &app.data.as_ref().expect("curve").samples
    ));
    app.busy = false;
    let task = app.launch_all();
    assert!(app.spectrum.busy);
    assert!(!app.busy);
    let revision = app.revision;
    for message in messages(task) {
        let _ = app.update(message);
    }
    assert!(app.spectrum.result.is_some());
    assert!(!app.spectrum.busy);
    assert!(!app.busy);
    assert_eq!(app.revision, revision);
    assert!(Arc::ptr_eq(
        &samples,
        &app.data.as_ref().expect("curve").samples
    ));
    let spectrum = Arc::clone(app.spectrum.result.as_ref().expect("spectrum"));
    let _ = app.update(Message::ResetView);
    let _ = app.update(Message::Flag(Flag::Grid, false));
    assert!(Arc::ptr_eq(
        &spectrum,
        app.spectrum.result.as_ref().expect("cached spectrum")
    ));
}
#[test]
fn fitting_previews_without_replacing_manual_entries_then_imports_full_precision() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    app.project.globals.push(model::Definition {
        kind: model::DefinitionKind::Function,
        name: "target".into(),
        parameters: "x".into(),
        expression: "exp(x)".into(),
    });
    let original = serde_json::to_string(&app.project.entries).expect("original entries");
    let selected = app.update(Message::Fit(FitAction::Select("target".into())));
    assert_eq!(selected.units(), 0);
    let task = app.update(Message::Fit(FitAction::Refit));
    assert!(app.fit.busy);
    for message in messages(task) {
        let _ = app.update(message);
    }
    assert!(!app.fit.busy);
    assert!(app.fit.result.as_ref().expect("fit").converged);
    assert_eq!(
        serde_json::to_string(&app.project.entries).expect("retained entries"),
        original
    );
    assert!(app.fit.preview);
    assert_eq!(
        app.points.len(),
        app.fit.result.as_ref().expect("fit").nodes.len()
    );
    let before = app
        .compiled
        .as_ref()
        .expect("preview")
        .evaluate(C::new(0.333, 0.))
        .expect("finite");
    let _ = app.update(Message::Fit(FitAction::Apply));
    assert!(!app.fit.preview);
    assert!(app.error.is_none());
    assert_ne!(
        serde_json::to_string(&app.project.entries).expect("fitted entries"),
        original
    );
    let after = app
        .compiled
        .as_ref()
        .expect("manual fit")
        .evaluate(C::new(0.333, 0.))
        .expect("finite");
    assert!((before - after).norm() < 1e-14);
    let json = serde_json::to_string(&app.project).expect("save fit settings");
    let project: Project = serde_json::from_str(&json).expect("load fit settings");
    assert_eq!(project.fit.function, "target");
    assert!(project.compile().is_ok());
}
#[test]
fn a_cancelled_fit_releases_the_worker_and_only_the_latest_request_runs() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    app.fit.busy = true; // Hold an old, now obsolete numerical solve.
    app.fit.running_generation = Some(0);
    app.project.fit.function = "target".into();
    app.project.globals = "target(x)=exp(x)".into();
    app.queue_fit(false);
    for end in ["2", "3", "4"] {
        app.project.fit.interval[1] = end.into();
        app.queue_fit(false);
    }
    let generation = app.fit.generation;
    assert_eq!(app.fit.latest.load(Ordering::Relaxed), generation);
    assert_eq!(
        app.fit
            .pending
            .as_ref()
            .expect("latest pending")
            .1
            .fit
            .interval[1],
        "4"
    );
    let _ = app.update(Message::FitProgress(
        0,
        aaa::Progress::Solving {
            nodes: 90,
            grid: 16385,
        },
    ));
    assert!(app.fit.progress.is_none());
    let _ = app.update(Message::Fitted(0, Ok(None)));
    assert!(
        !app.fit.busy,
        "replacement does not wait for old completion"
    );
    assert!(app.fit.pending.is_some());
    let task = app.launch_all();
    assert!(app.fit.busy);
    assert!(app.fit.pending.is_none());
    assert_eq!(app.fit.running_generation, Some(generation));
    let _ = app.update(Message::Fitted(0, Ok(None)));
    assert!(
        app.fit.busy,
        "a duplicate old completion must not release the new worker"
    );
    let mut progress = false;
    for message in messages(task) {
        if let Message::FitProgress(g, _) = &message {
            assert_eq!(*g, generation);
            progress = true;
        }
        let _ = app.update(message);
    }
    assert!(progress);
    assert!(!app.fit.busy);
    assert!(app.fit.running_generation.is_none());
    assert_eq!(
        app.fit.result.as_ref().expect("completed fit").interval,
        [-1., 4.]
    );
}
#[test]
fn fitting_requires_an_explicit_run_and_completed_results_remain_available() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    app.project.globals = "target(x)=exp(x)".into();
    let _ = app.update(Message::Fit(FitAction::Select("target".into())));
    assert!(!app.fit.busy);
    assert!(app.fit.pending.is_none());
    let task = app.update(Message::Fit(FitAction::Refit));
    for message in messages(task) {
        let _ = app.update(message);
    }
    let result = Arc::clone(app.fit.result.as_ref().expect("completed result"));
    for message in [
        Message::Fit(FitAction::Text(FitField::Tolerance, "1e-15".into())),
        Message::Fit(FitAction::Method(aaa::Method::Nonlinear)),
        Message::Fit(FitAction::Text(FitField::SkSteps, "12".into())),
        Message::Fit(FitAction::Text(FitField::WfSteps, "15".into())),
        Message::Fit(FitAction::Text(
            FitField::RefinementTolerance,
            "1e-5".into(),
        )),
        Message::Fit(FitAction::Text(FitField::LawsonDamping, "0.4".into())),
        Message::Fit(FitAction::Density(5)),
        Message::GlobalEdit(0, GlobalField::Expression, "cos(x)".into()),
        Message::Flag(Flag::Grid, false),
        Message::Tab(Tab::Fit),
    ] {
        let _ = app.update(message);
        assert!(!app.fit.busy);
        assert!(app.fit.pending.is_none());
        assert!(Arc::ptr_eq(
            &result,
            app.fit.result.as_ref().expect("retained result")
        ));
    }
    assert!(app.fit.stale);
    // Old workspaces may contain the former automatic-fit flag. Loading one
    // must not implicitly start a fit under the new explicit-run workflow.
    let mut saved = serde_json::to_value(&app.project).expect("saved settings");
    saved["fit"]["live"] = true.into();
    saved["fit"]["max_grid"] = 16385.into();
    let project: Project = serde_json::from_value(saved).expect("legacy workspace");
    let (loaded, _) = App::new(project);
    assert_eq!(loaded.project.fit.method, aaa::Method::Nonlinear);
    assert_eq!(loaded.project.fit.lawson_damping, "0.4");
    assert_eq!(loaded.project.fit.sk_steps, "12");
    assert_eq!(loaded.project.fit.wf_steps, "15");
    assert_eq!(loaded.project.fit.refinement_tolerance, "1e-5");
    assert!(!loaded.fit.busy);
    assert!(loaded.fit.pending.is_none());
}
fn blocked_process() -> (worker::Job, smol::channel::Receiver<worker::Event>) {
    let (job, receiver) = worker::Job::spawn(worker::Request::Block, std::time::Duration::ZERO)
        .expect("blocked process");
    assert!(matches!(
        worker::tests::receive(&receiver),
        worker::Event::Ready
    ));
    (job, receiver)
}
#[test]
fn cancel_and_restart_fit_kill_blocked_processes_and_preserve_completed_output() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true;
    app.project.globals = "target(x)=exp(x)".into();
    app.project.fit.function = "target".into();
    let task = app.update(Message::Fit(FitAction::Refit));
    for message in messages(task) {
        let _ = app.update(message);
    }
    let completed = Arc::clone(app.fit.result.as_ref().expect("completed result"));
    let curve = Arc::clone(&app.data.as_ref().expect("curve").samples);
    for cancel in [true, false] {
        let (job, receiver) = blocked_process();
        app.fit.process = Some(job);
        app.fit.busy = true;
        let old = app.fit.generation;
        app.fit.running_generation = Some(old);
        if cancel {
            let _ = app.update(Message::Fit(FitAction::Cancel));
            assert!(!app.fit.busy);
            assert!(app.fit.process.is_none());
            assert!(app.fit.cancelled);
            assert!(Arc::ptr_eq(
                &completed,
                app.fit.result.as_ref().expect("kept result")
            ));
        } else {
            let task = app.update(Message::Fit(FitAction::Refit));
            assert!(app.fit.busy, "replacement starts immediately");
            let _ = app.update(Message::Fitted(old, Ok(None)));
            assert!(app.fit.busy, "old completion cannot stop the replacement");
            for message in messages(task) {
                let _ = app.update(message);
            }
            assert!(!app.fit.busy);
            assert!(app.fit.result.as_ref().expect("new result").converged);
        }
        assert!(matches!(
            worker::tests::receive(&receiver),
            worker::Event::Cancelled
        ));
        assert!(Arc::ptr_eq(
            &curve,
            &app.data.as_ref().expect("curve").samples
        ));
    }
}
#[test]
fn disabling_or_cancelling_spectra_kills_workers_and_does_not_restart_from_the_config_page() {
    let mut app = super::super::tests::app(Mode::Real);
    app.busy = true; // Defer any replacement while testing the requested work.
    app.tab = Tab::Spectrum;
    app.spectrum.details_requested = true;
    app.project.settings.analysis.poles = true;
    let (job, receiver) = blocked_process();
    app.spectrum.process = Some(job);
    app.spectrum.busy = true;
    app.spectrum.running_generation = Some(app.spectrum.generation);
    let _ = app.update(Message::Flag(Flag::Poles, false));
    assert!(!app.spectrum.busy);
    assert!(app.spectrum.process.is_none());
    assert!(app.spectrum.pending.is_none());
    assert!(matches!(
        worker::tests::receive(&receiver),
        worker::Event::Cancelled
    ));
    assert_eq!(app.launch_all().units(), 0);

    // If zeros are still wanted, stop the old pole/zero solve and queue only zeros.
    app.project.settings.analysis = AnalysisOptions {
        poles: true,
        zeros: true,
        residues: false,
    };
    app.queue_spectrum();
    let (job, receiver) = blocked_process();
    app.spectrum.pending = None;
    app.spectrum.process = Some(job);
    app.spectrum.busy = true;
    app.spectrum.running_generation = Some(app.spectrum.generation);
    let _ = app.update(Message::Flag(Flag::Poles, false));
    assert!(matches!(
        worker::tests::receive(&receiver),
        worker::Event::Cancelled
    ));
    let (_, _, options) = app
        .spectrum
        .pending
        .as_ref()
        .expect("zero-only replacement");
    assert_eq!(
        *options,
        AnalysisOptions {
            poles: false,
            zeros: true,
            residues: false
        }
    );

    let _ = app.update(Message::CancelSpectrum);
    assert!(app.spectrum.paused);
    assert!(app.spectrum.pending.is_none());
    for _ in 0..5 {
        assert_eq!(app.launch_all().units(), 0);
    }
    let _ = app.update(Message::Tab(Tab::Plot));
    let _ = app.update(Message::Tab(Tab::Spectrum));
    assert!(
        app.spectrum.paused,
        "reopening settings must not undo Cancel"
    );
    assert!(app.spectrum.pending.is_none());
    let _ = app.update(Message::CalculateSpectrum);
    assert!(!app.spectrum.paused);
    assert!(app.spectrum.pending.is_some());
}
#[test]
fn spectral_visibility_and_magnitude_filters_reuse_the_current_decomposition() {
    use crate::numerics::filter::{Action, Field, Locations};
    let mut app = super::super::tests::app(Mode::Real);
    app.tab = Tab::Spectrum;
    let all = AnalysisOptions {
        poles: true,
        zeros: true,
        residues: true,
    };
    app.spectrum.key = Some((
        app.compiled
            .as_ref()
            .expect("model")
            .nodes
            .iter()
            .map(|n| [n.z, n.value, n.weight])
            .collect(),
        all,
    ));
    app.spectrum.available = all;
    app.spectrum.result = Some(Arc::new(Spectrum {
        poles: vec![C::new(3., 4.)],
        ..Spectrum::default()
    }));
    let cached = Arc::clone(app.spectrum.result.as_ref().expect("cached"));
    let samples = Arc::clone(&app.data.as_ref().expect("data").samples);
    let generation = app.spectrum.generation;
    for action in [
        Action::Locations(Locations::Real),
        Action::Locations(Locations::All),
        Action::Text(Field::MaxPole, "4".into()),
    ] {
        let task = app.update(Message::Spectrum(action));
        assert_eq!(task.units(), 0);
        assert!(Arc::ptr_eq(
            &cached,
            app.spectrum.result.as_ref().expect("cache")
        ));
        assert!(Arc::ptr_eq(
            &samples,
            &app.data.as_ref().expect("data").samples
        ));
        assert_eq!(app.spectrum.generation, generation);
    }
    assert!(!app.spectrum.filter.pole(&cached, 0));
    let _ = app.update(Message::Spectrum(Action::Text(
        Field::MaxPole,
        "oops".into(),
    )));
    assert!(
        app.issues
            .contains_key(&InputKey::SpectrumFilter(Field::MaxPole))
    );
    assert_eq!(app.spectrum.filter.max_pole, Some(4.));
}
#[test]
fn fitting_and_refitting_after_mode_changes_preserve_the_viewport() {
    let mut app = super::super::tests::app(Mode::Complex);
    let _ = app.update(Message::AddPath(model::InputKind::Interval));
    let _ = app.update(Message::Field(Field::Start, "-3.5".into()));
    let _ = app.update(Message::Field(Field::End, "0.75".into()));
    let _ = app.update(Message::AddPath(model::InputKind::Point));
    let _ = app.update(Message::ControlAxis(0, 0, "pi/3".into()));
    let _ = app.update(Message::ControlAxis(0, 1, "0.75".into()));
    app.project.settings.coloring = false;
    app.project.settings.real_coloring = false;
    app.project.globals.push(model::Definition {
        kind: model::DefinitionKind::Function,
        name: "target".into(),
        parameters: "x".into(),
        expression: "1e6/(x-1.0001)".into(),
    });
    app.project.fit.function = "target".into();
    // The endpoint approaches a pole; framing the output would blow up y.
    let bounds = Bounds {
        x_min: -2.3,
        x_max: 4.1,
        y_min: -1.7,
        y_max: 2.8,
    };
    let _ = app.update(Message::Plot(Interaction::Bounds(bounds)));
    let axes = app.axis.clone();
    let paths = (
        app.project.settings.input.clone(),
        app.project.settings.additional_paths.clone(),
    );
    for mode in [Mode::Complex, Mode::Real, Mode::Complex] {
        let _ = app.update(Message::Mode(mode));
        for interval in [[-1., 1.], [-0.75, 0.5]] {
            app.project.fit.interval = interval.map(|x| model::number(C::new(x, 0.)));
            let request = aaa::Request::new(&app.project).expect("request");
            let fit = aaa::fit(&request, 0, &AtomicU64::new(0), |_| {})
                .expect("fit")
                .expect("current");
            assert!(fit.converged);
            let generation = app.fit.generation;
            app.fit.preview = true;
            app.fit.busy = true;
            app.fit.running_generation = Some(generation);
            let _ = app.update(Message::Fitted(generation, Ok(Some(fit))));
            assert_eq!(
                app.project.settings.bounds, bounds,
                "a completed fit must not frame output extrema"
            );
            assert_eq!(app.axis, axes);
            assert_eq!(app.project.settings.input, paths.0);
            assert_eq!(app.project.settings.additional_paths, paths.1);
            let fitted_count = app.fit.result.as_ref().unwrap().nodes.len();
            for path in [0, 1, 2] {
                let _ = app.update(Message::SelectPath(path));
                assert_eq!(
                    app.points.len(),
                    fitted_count,
                    "selecting paths must retain fitted markers"
                );
            }
            for preview in [false, true] {
                let _ = app.update(Message::Fit(FitAction::Preview(preview)));
                assert_eq!(app.project.settings.input, paths.0);
                assert_eq!(app.project.settings.additional_paths, paths.1);
            }
        }
    }
    // Framing remains an explicit action for callers who request it.
    let _ = app.update(Message::Fit(FitAction::Frame));
    assert_ne!(app.project.settings.bounds, bounds);
    assert_eq!(app.project.settings.input, paths.0);
    assert_eq!(app.project.settings.additional_paths, paths.1);
}
