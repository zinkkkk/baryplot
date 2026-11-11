//! A completed CPU image must not replace the front image until its allocation is ready.
use super::*;
use iced::advanced::image::Renderer as _;
use iced::widget::image::{Allocation, Handle};
use smol::stream::StreamExt;

fn finish_worker(task: Task<Message>) -> Vec<RenderResult> {
    let mut stream = iced_runtime::task::into_stream(task).expect("worker dispatched immediately");
    smol::block_on(async {
        let mut results = Vec::new();
        while let Some(action) = stream.next().await {
            match action {
                iced_runtime::Action::Output(Message::Rendered(result)) => results.push(result),
                _ => panic!("unexpected worker action"),
            }
        }
        results
    })
}

fn allocate(handle: &Handle) -> Allocation {
    iced_tiny_skia::Renderer::new(Default::default())
        .load_image(handle)
        .expect("software allocation")
}
fn front() -> (App, Handle) {
    let mut app = super::tests::app(Mode::Complex);
    let handle = Handle::from_rgba(1, 1, vec![50, 90, 140, 255]);
    let mut data = (**app.data.as_ref().expect("data")).clone();
    data.image = Some(handle.clone());
    app.data = Some(Arc::new(data));
    app.image_allocation = Some(allocate(&handle));
    app.domain_pending = false;
    app.dirty = false;
    (app, handle)
}
fn stage(app: &mut App, image: Handle) -> TextureUpload {
    let upload = TextureUpload {
        epoch: app.latest.load(Ordering::Relaxed),
        domain_revision: app.domain_revision,
        domain: app.project.settings.bounds,
        elapsed_ms: 1.,
    };
    let mut data = (**app.data.as_ref().expect("data")).clone();
    data.image = Some(image);
    app.domain_pending = true;
    app.busy = true;
    let _ = app.update(Message::Rendered(RenderResult {
        epoch: upload.epoch,
        revision: app.revision,
        domain_revision: upload.domain_revision,
        complete: true,
        data: Some(Arc::new(data)),
    }));
    upload
}
#[test]
fn pending_gpu_upload_keeps_the_old_front_pinned_then_swaps_atomically() {
    let (mut app, old) = front();
    let old_memory = app.image_allocation.as_ref().expect("pinned").downgrade();
    let replacement = Handle::from_rgba(1, 1, vec![180, 70, 90, 255]);
    let upload = stage(&mut app, replacement.clone());
    assert!(app.busy);
    assert_eq!(app.data.as_ref().expect("front").image.as_ref(), Some(&old));
    assert!(Allocation::upgrade(&old_memory).is_some());
    let samples = Arc::clone(&app.data.as_ref().expect("data").samples);
    let _ = app.update(Message::ImageReady(upload, Ok(allocate(&replacement))));
    assert_eq!(
        app.data.as_ref().expect("new front").image.as_ref(),
        Some(&replacement)
    );
    assert!(Arc::ptr_eq(
        &samples,
        &app.data.as_ref().expect("samples").samples
    ));
    assert!(
        Allocation::upgrade(&old_memory).is_none(),
        "the old allocation is released only after a ready replacement"
    );
    assert_eq!(
        app.image_allocation
            .as_ref()
            .expect("pinned replacement")
            .handle(),
        &replacement
    );
    assert!(!app.busy);
}
#[test]
fn obsolete_or_failed_uploads_never_blank_the_front_image() {
    let (mut app, old) = front();
    let replacement = Handle::from_rgba(1, 1, vec![180, 70, 90, 255]);
    let upload = stage(&mut app, replacement.clone());
    app.latest.fetch_add(1, Ordering::Relaxed);
    app.domain_pending = false;
    let _ = app.update(Message::ImageReady(upload, Ok(allocate(&replacement))));
    assert_eq!(app.data.as_ref().expect("front").image.as_ref(), Some(&old));
    let upload = stage(&mut app, replacement);
    let _ = app.update(Message::ImageReady(
        upload,
        Err(iced::widget::image::Error::OutOfMemory),
    ));
    assert_eq!(app.data.as_ref().expect("front").image.as_ref(), Some(&old));
    assert_eq!(
        app.image_allocation
            .as_ref()
            .expect("still pinned")
            .handle(),
        &old
    );
    assert!(!app.busy);
    assert!(!app.domain_pending);
}
#[test]
fn invalid_drag_keeps_the_last_valid_colour_image_available() {
    let (mut app, old) = front();
    let _ = app.update(Message::Plot(Interaction::Move(
        Target::Parameter(0, 0),
        C::new(0., 1.),
    )));
    assert!(
        app.error.is_some(),
        "this coincides with the second active node"
    );
    assert_eq!(app.data.as_ref().expect("front").image.as_ref(), Some(&old));
    assert_eq!(
        app.image_allocation
            .as_ref()
            .expect("still pinned")
            .handle(),
        &old
    );
    assert_eq!(app.points.len(), 3);
}

#[test]
fn drag_jobs_use_full_resolution_and_sample_count_without_waiting_for_release() {
    for resolution in [128, 640, 1536] {
        let (mut app, _) = front();
        app.busy = true; // Drive worker/upload completions explicitly for a deterministic race.
        let _ = app.update(Message::Appearance(appearance::Action::Number(
            appearance::Number::Resolution,
            resolution.to_string(),
        )));
        let _ = app.update(Message::Field(Field::Samples, "5000".into()));
        let retained = Arc::clone(&app.data.as_ref().expect("complete curve").samples);
        let position = C::new(-0.99, 1.); // Approximately one screen pixel from the original value.
        app.busy = false;
        let task = app.update(Message::Plot(Interaction::Move(
            Target::Parameter(0, 1),
            position,
        )));
        assert!(app.busy);
        assert!(app.line_pending);
        assert!(Arc::ptr_eq(
            &retained,
            &app.data.as_ref().expect("retained curve").samples
        ));
        let epoch = app.latest.load(Ordering::Relaxed);
        // Exercise App::launch itself, including the settings handed to the actual Rayon job.
        // No settling tick or release event is sent, and every published line is complete.
        let results = finish_worker(task);
        assert_eq!(results.len(), 2, "one full line followed by one full image");
        assert!(!results[0].complete);
        assert!(results[1].complete);
        for result in &results {
            assert_eq!(
                result.data.as_ref().expect("full samples").samples.len(),
                5000
            );
        }
        let full = results[1].data.as_ref().expect("full render");
        let image = full.image.as_ref().expect("full texture").clone();
        let allocation = allocate(&image);
        assert_eq!(allocation.size().width, resolution);
        let upload = TextureUpload {
            epoch,
            domain_revision: app.domain_revision,
            domain: full.domain,
            elapsed_ms: full.elapsed_ms,
        };
        for result in results {
            let _ = app.update(Message::Rendered(result));
        }
        assert_eq!(app.data.as_ref().expect("full samples").samples.len(), 5000);
        let samples = Arc::clone(&app.data.as_ref().expect("data").samples);
        // A duplicate/rounded cursor message can arrive while the full texture is uploading.
        // There is deliberately no mouse-release event anywhere in this test.
        let _ = app.update(Message::Plot(Interaction::Move(
            Target::Parameter(0, 1),
            position,
        )));
        assert_eq!(
            app.data.as_ref().expect("still complete").samples.len(),
            5000
        );
        assert!(Arc::ptr_eq(
            &samples,
            &app.data.as_ref().expect("data").samples
        ));
        assert_eq!(app.latest.load(Ordering::Relaxed), epoch);
        let idle = app.update(Message::ImageReady(upload, Ok(allocation)));
        assert!(
            iced_runtime::task::into_stream(idle).is_none(),
            "no second quality pass"
        );
        assert_eq!(
            app.image_allocation
                .as_ref()
                .expect("full image")
                .size()
                .width,
            app.project.settings.resolution
        );
        for _ in 0..3 {
            let _ = app.update(Message::Plot(Interaction::Move(
                Target::Parameter(0, 1),
                position,
            )));
        }
        // Integer rounding can also turn real pointer movement into a numerical no-op.
        let revision = app.revision;
        let _ = app.update(Message::Lock(0, 2, 0, true));
        for x in [1.001, 1.002] {
            let _ = app.update(Message::Plot(Interaction::Move(
                Target::Parameter(0, 2),
                C::new(x, 0.),
            )));
        }
        assert_eq!(app.revision, revision);
        assert_eq!(app.data.as_ref().expect("full samples").samples.len(), 5000);
        assert!(!app.busy);
        assert!(!app.dirty);
        assert!(!app.domain_pending);
        assert!(!app.line_pending);
    }
}

#[test]
fn colour_survives_mode_switches_and_changed_imaginary_coefficients_request_a_refresh() {
    let mut app = super::tests::app(Mode::Real);
    app.busy = true;
    let _ = app.update(Message::Flag(Flag::Coloring, true));
    let image = Handle::from_rgba(1, 1, vec![20, 70, 140, 255]);
    let mut data = (**app.data.as_ref().expect("data")).clone();
    data.image = Some(image.clone());
    app.data = Some(Arc::new(data));
    app.image_allocation = Some(allocate(&image));
    app.domain_pending = false;
    app.dirty = false;
    app.busy = false;
    for mode in [Mode::Complex, Mode::Real, Mode::Complex] {
        let task = app.update(Message::Mode(mode));
        assert!(
            app.project.settings.coloring_enabled(),
            "Colour stays enabled when switching mode"
        );
        assert_eq!(
            app.data.as_ref().expect("data").image.as_ref(),
            Some(&image),
            "identical coefficients must retain the ready colour image"
        );
        assert!(!app.domain_pending);
        assert_eq!(
            task.units(),
            0,
            "reused colour does not need a new numerical render"
        );
    }
    app.busy = true;
    let _ = app.update(Message::EditImaginary(0, 1, "2".into()));
    let complex_revision = app.domain_revision;
    assert!(app.domain_pending);
    let _ = app.update(Message::Mode(Mode::Real));
    assert!(app.domain_pending);
    assert!(
        app.domain_revision > complex_revision,
        "dropping an imaginary coefficient must schedule the real-coefficient colour field"
    );
    assert_eq!(
        app.data.as_ref().expect("held colour").image.as_ref(),
        Some(&image)
    );
}
