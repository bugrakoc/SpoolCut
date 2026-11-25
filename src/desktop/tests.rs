use std::{fs, process::Command, time::Instant};

use super::*;

fn wait_for(description: &str, ready: impl Fn() -> bool) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(12);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "Timed out waiting for {description}"
        );
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(8));
    }
}

fn fixture(path: &Path, color: &str, frequency: u32) {
    let result = Command::new("ffmpeg")
        .args(["-nostdin", "-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("color={color}:s=160x96:r=25:d=1.28"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "sine=frequency={frequency}:sample_rate=48000:duration=1.28"
        ))
        .args([
            "-c:v",
            "libx264",
            "-g",
            "16",
            "-bf",
            "2",
            "-threads:v",
            "1",
            "-x264-params",
            "open-gop=0:scenecut=0",
            "-c:a",
            "aac",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn timeline_x(app: &App, time: i64) -> f64 {
    seconds(time - app.ui.timeline.time_at(0.0)) * app.ui.timeline.scale()
}

fn timeline_drag(app: &App, start: f64, y: f64, end: f64) {
    let controllers = app.ui.timeline.area.observe_controllers();
    let gesture = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .find_map(|controller| controller.downcast::<gtk::GestureDrag>().ok())
        .expect("timeline drag controller");
    gesture.emit_by_name::<()>("drag-begin", &[&start, &y]);
    gesture.emit_by_name::<()>("drag-update", &[&(end - start), &0.0f64]);
    gesture.emit_by_name::<()>("drag-end", &[&(end - start), &0.0f64]);
}

fn preview_is_ready(app: &App, index: usize) -> bool {
    let state = app.state.borrow();
    state.pending.is_none()
        && state
            .active
            .as_ref()
            .is_some_and(|active| active.index == index && active.ready)
}

fn wait_for_paused_frame(app: &App, clip: usize, source_seconds: f64, red: bool) {
    let color = if red { "red" } else { "blue" };
    wait_for(
        &format!("clip {clip} to seek to {source_seconds:.2}s"),
        || {
            preview_is_ready(app, clip)
                && app
                    .preview
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .position()
                    .is_some_and(|position| (position - source_seconds).abs() < 0.01)
        },
    );
    wait_for(
        &format!("a {color} frame from clip {clip} at {source_seconds:.2}s"),
        || {
            let [r, _, b, _] = app.preview.borrow().as_ref().unwrap().last_pixel.get();
            if red {
                r > 180 && b < 40
            } else {
                b > 180 && r < 40
            }
        },
    );
}

#[test]
#[ignore = "requires a GTK display and FFmpeg encoders"]
fn desktop_edit_preview_persist_export() {
    initialize_locale();
    gtk::init().unwrap();
    let gtk_app = gtk::Application::new(None::<&str>, gio::ApplicationFlags::NON_UNIQUE);
    gtk_app.register(gio::Cancellable::NONE).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let a = scratch.path().join("red.mp4");
    let b = scratch.path().join("blue.mp4");
    fixture(&a, "red", 440);
    fixture(&b, "blue", 880);

    let app = App::new(&gtk_app);
    app.ui.window.present();
    wait_for("the preview to initialize", || {
        app.preview.borrow().is_some()
    });
    app.start_index(vec![a, b.clone()], IndexIntent::Import);
    wait_for("media import", || app.state.borrow().job.is_none());
    assert_eq!(app.state.borrow().editor.project.sources.len(), 2);

    // Files are listed by source ID assigned in sorted path order.
    app.ui
        .media
        .select_row(app.ui.media.row_at_index(1).as_ref());
    app.ui.append.emit_clicked();
    app.ui
        .media
        .select_row(app.ui.media.row_at_index(0).as_ref());
    app.ui.append.emit_clicked();
    assert_eq!(app.state.borrow().total(), 2_560_000);
    assert!(app.ui.play.is_sensitive());

    let split_x = timeline_x(&app, 690_000);
    timeline_drag(&app, split_x, 12.0, split_x);
    assert_eq!(app.state.borrow().position, 640_000);
    app.ui.split.emit_clicked();
    assert_eq!(app.state.borrow().editor.project.clips.len(), 3);
    assert_eq!(app.state.borrow().position, 640_000);
    let short_width = timeline_x(&app, 640_000) - timeline_x(&app, 0);
    let long_width = timeline_x(&app, 2_560_000) - timeline_x(&app, 1_280_000);
    assert!((long_width - 2.0 * short_width).abs() < 0.01);
    let before_zoom = app.ui.timeline.scale();
    app.ui.zoom_in.emit_clicked();
    assert!(app.ui.timeline.scale() > before_zoom);
    app.ui.zoom_out.emit_clicked();
    assert!((app.ui.timeline.scale() - before_zoom).abs() < 0.01);
    app.ui.zoom_fit.emit_clicked();
    timeline_drag(
        &app,
        timeline_x(&app, 950_000),
        64.0,
        timeline_x(&app, 2_450_000),
    );
    let ordered: Vec<_> = app
        .state
        .borrow()
        .editor
        .project
        .clips
        .iter()
        .map(|clip| clip.source.clone())
        .collect();
    assert_eq!(&ordered, &["source-2", "source-1", "source-2"]);
    app.ui.undo.emit_clicked();
    assert_eq!(
        app.state.borrow().editor.project.clips[1].source,
        "source-2"
    );
    app.ui.redo.emit_clicked();
    assert_eq!(
        app.state.borrow().editor.project.clips[1].source,
        "source-1"
    );

    let out_handle = timeline_x(&app, 1_920_000) - 3.0;
    timeline_drag(
        &app,
        out_handle,
        64.0,
        out_handle - app.ui.timeline.scale() * 0.64,
    );
    assert_eq!(app.state.borrow().total(), 1_920_000);
    let in_handle = timeline_x(&app, 1_280_000) + 3.0;
    timeline_drag(
        &app,
        in_handle,
        64.0,
        in_handle - app.ui.timeline.scale() * 0.64,
    );
    assert_eq!(app.state.borrow().total(), 2_560_000);
    app.ui.undo.emit_clicked();
    assert_eq!(app.state.borrow().total(), 1_920_000);

    let project = scratch.path().join("timeline.json");
    app.save_to(project.clone());
    assert_eq!(
        Project::load(&project).unwrap().sources["source-1"],
        PathBuf::from("blue.mp4")
    );
    app.new_project();
    app.open_project(project.clone());
    wait_for("project reopen", || app.state.borrow().job.is_none());
    assert_eq!(app.state.borrow().total(), 1_920_000);
    wait_for_paused_frame(&app, 0, 0.0, true);

    let join_x = timeline_x(&app, 590_000);
    timeline_drag(&app, join_x, 12.0, join_x);
    assert_eq!(app.state.borrow().position, 640_000);
    wait_for_paused_frame(&app, 1, 0.0, false);
    let last_x = timeline_x(&app, 1_900_000);
    timeline_drag(&app, last_x, 12.0, last_x);
    assert_eq!(app.state.borrow().position, 1_280_000);
    wait_for_paused_frame(&app, 2, 0.64, true);

    timeline_drag(
        &app,
        timeline_x(&app, 90_000),
        12.0,
        timeline_x(&app, 920_000),
    );
    wait_for_paused_frame(&app, 1, 0.28, false);

    // Exercise both source switches and seeking a second time while paused.
    for (position, clip, source_seconds, red) in [
        (1_570_000, 2, 0.92, true),
        (710_000, 1, 0.04, false),
        (1_120_000, 1, 0.48, false),
        (100_000, 0, 0.08, true),
        (1_250_000, 1, 0.60, false),
        (1_570_000, 2, 0.92, true),
    ] {
        app.request_preview(position, false);
        wait_for_paused_frame(&app, clip, source_seconds, red);
    }

    app.request_preview(1_920_000, false);
    app.ui.play.emit_clicked();
    wait_for("playback to enter the blue clip", || {
        preview_is_ready(&app, 1)
    });
    wait_for("playback to enter the final red clip", || {
        preview_is_ready(&app, 2)
    });
    wait_for("playback to end", || !app.state.borrow().playing);
    assert_eq!(app.state.borrow().position, 1_920_000);

    app.schedule_software_preview(anyhow::anyhow!("Testing OpenGL fallback"));
    wait_for("software preview fallback", || {
        app.preview
            .borrow()
            .as_ref()
            .is_some_and(Preview::is_software)
    });
    app.request_preview(920_000, false);
    wait_for_paused_frame(&app, 1, 0.28, false);

    let output = scratch.path().join("output.mkv");
    app.start_export(output.clone());
    wait_for("desktop export", || app.state.borrow().job.is_none());
    assert!(output.is_file(), "{}", app.ui.status.text());

    let relocated = scratch.path().join("relocated.mp4");
    fs::rename(&b, &relocated).unwrap();
    app.open_project(project);
    wait_for("opening with a missing file", || {
        app.state.borrow().job.is_none()
    });
    assert!(app.state.borrow().editor.source("source-1").is_err());
    assert!(!app.ui.export.is_sensitive());
    app.start_index(vec![relocated], IndexIntent::Relink("source-1".into()));
    wait_for("relink", || app.state.borrow().job.is_none());
    assert!(app.ui.export.is_sensitive(), "{}", app.ui.status.text());

    app.closed.set(true);
    app.ui.window.close();
}
