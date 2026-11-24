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

fn preview_is_ready(app: &App, index: usize) -> bool {
    app.state
        .borrow()
        .active
        .as_ref()
        .is_some_and(|active| active.index == index && active.ready)
}

fn preview_center(app: &App) -> [u8; 4] {
    app.preview.borrow().as_ref().unwrap().last_pixel.get()
}

#[test]
#[ignore = "requires a GTK display with OpenGL and FFmpeg encoders"]
fn desktop_edit_preview_persist_export() {
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
    wait_for("the OpenGL preview to initialize", || {
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

    app.request_preview(690_000, false);
    app.ui.split.emit_clicked();
    assert_eq!(app.state.borrow().editor.project.clips.len(), 3);
    app.ui.later.emit_clicked();
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

    {
        let mut state = app.state.borrow_mut();
        let pts = state.editor.source("source-1").unwrap().1.info.safe_in_pts[1];
        state.editor.set_boundary(1, false, pts).unwrap();
    }
    app.refresh();
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

    // Requests arriving in the same GTK tick coalesce to the latest source.
    for position in [90_000, 840_000, 1_470_000, 920_000] {
        app.request_preview(position, false);
    }
    wait_for("the blue clip after rapid scrubbing", || {
        preview_is_ready(&app, 1)
    });
    assert!((app.preview.borrow().as_ref().unwrap().position().unwrap() - 0.28).abs() < 0.01);
    wait_for("a blue frame in the OpenGL preview", || {
        let blue = preview_center(&app);
        blue[2] > 180 && blue[0] < 40
    });
    app.request_preview(1_570_000, false);
    wait_for("the last red clip", || preview_is_ready(&app, 2));
    wait_for("a red frame in the OpenGL preview", || {
        let red = preview_center(&app);
        red[0] > 180 && red[2] < 40
    });

    app.request_preview(0, true);
    wait_for("playback to enter the blue clip", || {
        preview_is_ready(&app, 1)
    });
    wait_for("playback to enter the final red clip", || {
        preview_is_ready(&app, 2)
    });
    wait_for("playback to end", || !app.state.borrow().playing);
    assert_eq!(app.state.borrow().position, 1_920_000);

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
