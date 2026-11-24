mod editor;
mod preview;
mod widgets;

use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use gtk::{gio, glib, prelude::*};
use spoolcut::{ExportProgress, Project, export_with_progress};

use editor::{Editor, IndexedMedia, Segment, source_name, timecode};
use preview::{Event, Preview};
use widgets::{Widgets, seconds};

#[derive(Default)]
struct State {
    editor: Editor,
    selected_source: Option<String>,
    selected_clip: Option<usize>,
    segments: Vec<Segment>,
    timeline_error: Option<String>,
    position: i64,
    playing: bool,
    pending: Option<(i64, bool)>,
    active: Option<ActivePreview>,
    started_id: Option<i64>,
    job: Option<Arc<AtomicBool>>,
}

struct ActivePreview {
    id: i64,
    index: usize,
    loaded: bool,
    ready: bool,
}

enum IndexIntent {
    Open,
    Import,
    Relink(String),
}

enum JobEvent {
    Indexed(IndexIntent, Vec<(PathBuf, Result<IndexedMedia, String>)>),
    Progress(ExportProgress),
    Exported(Result<PathBuf, String>),
}

struct App {
    ui: Widgets,
    state: RefCell<State>,
    preview: RefCell<Option<Preview>>,
    tx: mpsc::Sender<JobEvent>,
    rx: mpsc::Receiver<JobEvent>,
    closed: Cell<bool>,
    rebuilding_bin: Cell<bool>,
}

fn main() -> glib::ExitCode {
    glib::set_application_name("SpoolCut");
    let application = gtk::Application::new(
        Some("io.github.bugrakoc.SpoolCut"),
        gio::ApplicationFlags::HANDLES_OPEN,
    );
    application.connect_activate(|application| {
        if let Some(window) = application.active_window() {
            window.present();
        } else {
            App::new(application).ui.window.present();
        }
    });
    application.connect_open(|application, files, _| {
        for path in files.iter().filter_map(gio::File::path) {
            let app = App::new(application);
            app.ui.window.present();
            app.open_project(path);
        }
    });
    application.run()
}

impl App {
    fn new(application: &gtk::Application) -> Rc<Self> {
        let (tx, rx) = mpsc::channel();
        let app = Rc::new(Self {
            ui: Widgets::new(application),
            state: RefCell::new(State::default()),
            preview: RefCell::new(None),
            tx,
            rx,
            closed: Cell::new(false),
            rebuilding_bin: Cell::new(false),
        });
        app.connect();
        app.refresh();
        let current = Rc::clone(&app);
        glib::timeout_add_local(Duration::from_millis(24), move || {
            if current.closed.get() {
                return glib::ControlFlow::Break;
            }
            current.update();
            glib::ControlFlow::Continue
        });
        app
    }

    fn bind(self: &Rc<Self>, button: &gtk::Button, action: impl Fn(&Rc<Self>) + 'static) {
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                action(&app);
            }
        });
    }

    fn connect(self: &Rc<Self>) {
        self.bind(&self.ui.new, |app| {
            app.confirm_discard(|app| app.new_project())
        });
        self.bind(&self.ui.open, |app| {
            app.confirm_discard(|app| app.pick_project())
        });
        self.bind(&self.ui.save, |app| app.save(false));
        self.bind(&self.ui.save_as, |app| app.save(true));
        self.bind(&self.ui.import, |app| app.pick_media());
        self.bind(&self.ui.append, |app| app.append());
        self.bind(&self.ui.relink, |app| app.relink());
        self.bind(&self.ui.export, |app| app.pick_export());
        self.bind(&self.ui.cancel, |app| app.cancel_job());
        self.bind(&self.ui.undo, |app| {
            app.state.borrow_mut().editor.undo();
            app.refresh();
        });
        self.bind(&self.ui.redo, |app| {
            app.state.borrow_mut().editor.redo();
            app.refresh();
        });
        self.bind(&self.ui.play, |app| {
            let (position, playing) = {
                let state = app.state.borrow();
                let position = if state.position >= state.total() {
                    0
                } else {
                    state.position
                };
                (position, !state.playing)
            };
            app.request_preview(position, playing);
        });
        self.bind(&self.ui.trim_in, |app| app.trim(true));
        self.bind(&self.ui.trim_out, |app| app.trim(false));
        self.bind(&self.ui.split, |app| app.split());
        self.bind(&self.ui.earlier, |app| app.move_clip(-1));
        self.bind(&self.ui.later, |app| app.move_clip(1));
        self.bind(&self.ui.remove, |app| app.remove_clip());

        let weak = Rc::downgrade(self);
        self.ui.media.connect_row_selected(move |_, row| {
            let Some(app) = weak.upgrade() else { return };
            if app.rebuilding_bin.get() {
                return;
            }
            let source = row.and_then(|row| {
                app.state
                    .borrow()
                    .editor
                    .project
                    .sources
                    .keys()
                    .nth(row.index() as usize)
                    .cloned()
            });
            app.state.borrow_mut().selected_source = source;
            app.controls();
        });
        let weak = Rc::downgrade(self);
        self.ui.media.connect_row_activated(move |_, _| {
            if let Some(app) = weak.upgrade() {
                app.append();
            }
        });
        let weak = Rc::downgrade(self);
        self.ui.scale.connect_change_value(move |_, _, value| {
            if let Some(app) = weak.upgrade() {
                app.request_preview((value * 1_000_000.0).round() as i64, false);
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        self.ui.window.connect_close_request(move |_| {
            let Some(app) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if app.closed.get() {
                return glib::Propagation::Proceed;
            }
            if app.state.borrow().job.is_some() {
                app.cancel_job();
                return glib::Propagation::Stop;
            }
            app.confirm_discard(|app| {
                app.closed.set(true);
                app.ui.window.close();
            });
            glib::Propagation::Stop
        });

        let weak = Rc::downgrade(self);
        self.ui.canvas.connect_realize(move |area| {
            let Some(app) = weak.upgrade() else { return };
            area.make_current();
            let result = area
                .error()
                .map_or_else(Preview::new, |error| Err(error.into()));
            match result {
                Ok(player) => *app.preview.borrow_mut() = Some(player),
                Err(error) => {
                    app.ui.placeholder.set_text("Preview unavailable");
                    app.ui.message(&format!("Preview unavailable. {error:#}"));
                }
            }
            app.controls();
        });
        let weak = Rc::downgrade(self);
        self.ui.canvas.connect_render(move |_, _| {
            let Some(app) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if let Some(player) = app.preview.borrow().as_ref()
                && let Err(error) = player.draw()
            {
                app.ui
                    .message(&format!("Preview rendering failed. {error:#}"));
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        self.ui.canvas.connect_unrealize(move |area| {
            if let Some(app) = weak.upgrade() {
                area.make_current();
                app.preview.borrow_mut().take();
            }
        });
    }

    fn confirm_discard(self: &Rc<Self>, proceed: impl FnOnce(&Rc<Self>) + 'static) {
        if !self.state.borrow().editor.dirty() {
            proceed(self);
            return;
        }
        let dialog = gtk::AlertDialog::builder()
            .message("Discard unsaved project changes?")
            .buttons(["Keep editing", "Discard changes"])
            .cancel_button(0)
            .default_button(0)
            .modal(true)
            .build();
        let app = Rc::clone(self);
        dialog.choose(
            Some(&self.ui.window),
            gio::Cancellable::NONE,
            move |choice| {
                if choice == Ok(1) {
                    proceed(&app);
                }
            },
        );
    }

    fn new_project(self: &Rc<Self>) {
        self.stop_preview();
        *self.state.borrow_mut() = State::default();
        self.refresh();
        self.ui.message("Import MP4 or MOV files to get started.");
    }

    fn pick_project(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Open project")
            .modal(true)
            .build();
        widgets::files(&dialog, "SpoolCut projects", &["*.json"]);
        let app = Rc::clone(self);
        dialog.open(
            Some(&self.ui.window),
            gio::Cancellable::NONE,
            move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        app.open_project(path);
                    } else {
                        app.ui.message("Choose a local project file.");
                    }
                }
            },
        );
    }

    fn open_project(self: &Rc<Self>, path: PathBuf) {
        match Project::load(&path) {
            Ok(project) => {
                self.stop_preview();
                let paths = {
                    let mut state = self.state.borrow_mut();
                    *state = State::default();
                    state.editor.open(project, path);
                    state.editor.project.sources.values().cloned().collect()
                };
                self.start_index(paths, IndexIntent::Open);
                self.refresh();
            }
            Err(error) => self.ui.message(&format!("{error:#}")),
        }
    }

    fn save(self: &Rc<Self>, save_as: bool) {
        let previous = self.state.borrow().editor.path.clone();
        if !save_as && let Some(path) = previous.as_ref() {
            self.save_to(path.clone());
            return;
        }
        let dialog = gtk::FileDialog::builder()
            .title("Save project")
            .initial_name(
                previous
                    .as_deref()
                    .map(source_name)
                    .unwrap_or_else(|| "Untitled.spoolcut.json".into()),
            )
            .modal(true)
            .build();
        let app = Rc::clone(self);
        dialog.save(
            Some(&self.ui.window),
            gio::Cancellable::NONE,
            move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        app.save_to(path);
                    } else {
                        app.ui
                            .message("Choose a local destination for the project.");
                    }
                }
            },
        );
    }

    fn save_to(&self, path: PathBuf) {
        let result = self.state.borrow_mut().editor.save(path.clone());
        match result {
            Ok(()) => {
                self.title();
                self.ui.message(&format!("Saved {}", source_name(&path)));
            }
            Err(error) => self.ui.message(&format!("{error:#}")),
        }
    }

    fn pick_media(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Import media")
            .modal(true)
            .build();
        widgets::files(
            &dialog,
            "MP4 and MOV video",
            &["*.mp4", "*.MP4", "*.mov", "*.MOV"],
        );
        let app = Rc::clone(self);
        dialog.open_multiple(
            Some(&self.ui.window),
            gio::Cancellable::NONE,
            move |result| {
                if let Ok(files) = result {
                    let paths: Vec<_> = (0..files.n_items())
                        .filter_map(|i| files.item(i))
                        .filter_map(|item| item.downcast::<gio::File>().ok())
                        .filter_map(|file| file.path())
                        .collect();
                    if paths.is_empty() {
                        app.ui.message("Choose local MP4 or MOV files.");
                    } else {
                        app.start_index(paths, IndexIntent::Import);
                    }
                }
            },
        );
    }

    fn relink(self: &Rc<Self>) {
        let Some(source) = self.state.borrow().selected_source.clone() else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title("Locate source file")
            .modal(true)
            .build();
        let app = Rc::clone(self);
        dialog.open(
            Some(&self.ui.window),
            gio::Cancellable::NONE,
            move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        app.start_index(vec![path], IndexIntent::Relink(source));
                    } else {
                        app.ui.message("Choose a local source file.");
                    }
                }
            },
        );
    }

    fn start_index(&self, mut paths: Vec<PathBuf>, intent: IndexIntent) {
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            self.ui.message("Project has no source files.");
            return;
        }
        self.ui
            .message(&format!("Inspecting {} source file(s)…", paths.len()));
        let cancel = self.start_job();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let results = paths
                .into_iter()
                .map(|path| {
                    let result =
                        IndexedMedia::load(&path, &cancel).map_err(|error| format!("{error:#}"));
                    (path, result)
                })
                .collect();
            let _ = tx.send(JobEvent::Indexed(intent, results));
        });
    }

    fn indexed(
        self: &Rc<Self>,
        intent: IndexIntent,
        results: Vec<(PathBuf, Result<IndexedMedia, String>)>,
    ) {
        let mut state = self.state.borrow_mut();
        let cancelled = state
            .job
            .take()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed));
        let mut failures = Vec::new();
        if !cancelled || matches!(intent, IndexIntent::Open) {
            match intent {
                IndexIntent::Relink(source) => {
                    if let Some((path, result)) = results.into_iter().next() {
                        match result {
                            Ok(media) => {
                                let failed_clip = state
                                    .editor
                                    .project
                                    .clips
                                    .iter()
                                    .filter(|clip| clip.source == source)
                                    .find_map(|clip| media.index.plan_clip(clip).err());
                                if let Some(error) = failed_clip {
                                    failures.push(format!(
                                        "The replacement cannot play the existing cuts. {error:#}"
                                    ));
                                } else {
                                    state.editor.media.insert(path.clone(), Ok(media));
                                    state.editor.change(|project| {
                                        project.sources.insert(source, path);
                                    });
                                }
                            }
                            Err(error) => failures.push(error),
                        }
                    }
                }
                intent => {
                    let paths: Vec<_> = results
                        .iter()
                        .filter(|(_, result)| result.is_ok())
                        .map(|(path, _)| path.clone())
                        .collect();
                    if matches!(intent, IndexIntent::Import) {
                        state.editor.import(&paths);
                    }
                    for (path, result) in results {
                        if let Err(error) = &result {
                            failures.push(error.clone());
                        }
                        state.editor.media.insert(path, result);
                    }
                }
            }
        }
        if state.selected_source.is_none() {
            state.selected_source = state.editor.project.sources.keys().next().cloned();
        }
        drop(state);
        self.refresh();
        if cancelled {
            self.ui.message(
                "Inspection cancelled. Use Relink to inspect an unavailable source again.",
            );
        } else if failures.is_empty() {
            self.ui.message("Source files ready.");
        } else {
            let prefix = if failures.len() == 1 {
                "One source is unavailable".into()
            } else {
                format!("{} sources are unavailable", failures.len())
            };
            self.ui.message(&format!("{prefix}. {}", failures[0]));
        }
    }

    fn append(self: &Rc<Self>) {
        let result = {
            let state = self.state.borrow();
            if state.job.is_some() {
                return;
            }
            state
                .selected_source
                .as_deref()
                .context("Select a source in the media bin")
                .and_then(|id| state.editor.source(id)?.1.initial_clip(id))
        };
        match result {
            Ok(clip) => {
                {
                    let mut state = self.state.borrow_mut();
                    state.selected_clip = Some(state.editor.project.clips.len());
                    state.editor.change(|project| project.clips.push(clip));
                }
                self.refresh();
                let selected = self.state.borrow().selected_clip;
                if let Some(index) = selected {
                    self.select_clip(index);
                }
            }
            Err(error) => self.ui.message(&format!("{error:#}")),
        }
    }

    fn trim(self: &Rc<Self>, trim_in: bool) {
        let details = {
            let state = self.state.borrow();
            state
                .selected_clip
                .context("Select a clip")
                .and_then(|index| {
                    let clip = &state.editor.project.clips[index];
                    let media = state.editor.source(&clip.source)?.1;
                    let points = state.editor.cut_points(index, trim_in)?;
                    let current = if trim_in { clip.in_pts } else { clip.out_pts };
                    Ok((index, points, current, media.info.video_time_base))
                })
        };
        let (index, points, current, base) = match details {
            Ok(result) => result,
            Err(error) => {
                self.ui.message(&format!("{error:#}"));
                return;
            }
        };
        let title = if trim_in { "Trim in" } else { "Trim out" };
        let labels: Vec<_> = points
            .iter()
            .map(|&pts| timecode(base.seconds(pts)))
            .collect();
        let label_refs: Vec<_> = labels.iter().map(String::as_str).collect();
        let list = gtk::DropDown::from_strings(&label_refs);
        list.set_enable_search(true);
        list.set_selected(points.iter().position(|&pts| pts == current).unwrap_or(0) as u32);
        let apply = gtk::Button::with_label("Apply");
        apply.add_css_class("suggested-action");
        apply.set_sensitive(!points.is_empty());
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.append(&gtk::Label::new(Some("Choose a safe source timestamp.")));
        content.append(&list);
        content.append(&apply);
        let dialog = gtk::Window::builder()
            .title(title)
            .transient_for(&self.ui.window)
            .modal(true)
            .default_width(320)
            .child(&content)
            .build();
        let window = dialog.downgrade();
        let app = Rc::downgrade(self);
        apply.connect_clicked(move |_| {
            if let (Some(app), Some(&pts)) = (app.upgrade(), points.get(list.selected() as usize)) {
                let result = app
                    .state
                    .borrow_mut()
                    .editor
                    .set_boundary(index, trim_in, pts);
                match result {
                    Ok(()) => {
                        app.refresh();
                        app.select_clip(index);
                    }
                    Err(error) => app.ui.message(&format!("{error:#}")),
                }
            }
            if let Some(window) = window.upgrade() {
                window.close();
            }
        });
        dialog.present();
    }

    fn split(self: &Rc<Self>) {
        let result = {
            let mut state = self.state.borrow_mut();
            let position = state.position;
            state
                .at(position)
                .context("Place the playhead on a clip")
                .and_then(|index| {
                    let segment = &state.segments[index];
                    let target = segment.source_start + seconds(position - segment.start_us);
                    state.editor.split(index, target)?;
                    state.selected_clip = Some(index + 1);
                    Ok(index + 1)
                })
        };
        match result {
            Ok(index) => {
                self.refresh();
                self.select_clip(index);
            }
            Err(error) => self.ui.message(&format!("{error:#}")),
        }
    }

    fn move_clip(self: &Rc<Self>, distance: isize) {
        {
            let mut state = self.state.borrow_mut();
            let Some(index) = state.selected_clip else {
                return;
            };
            let Some(destination) = index.checked_add_signed(distance) else {
                return;
            };
            if destination >= state.editor.project.clips.len() {
                return;
            }
            state
                .editor
                .change(|project| project.clips.swap(index, destination));
            state.selected_clip = Some(destination);
        }
        self.refresh();
    }

    fn remove_clip(self: &Rc<Self>) {
        {
            let mut state = self.state.borrow_mut();
            let Some(index) = state.selected_clip else {
                return;
            };
            state.editor.change(|project| {
                project.clips.remove(index);
            });
            state.selected_clip = if state.editor.project.clips.is_empty() {
                None
            } else {
                Some(index.min(state.editor.project.clips.len() - 1))
            };
        }
        self.refresh();
    }

    fn pick_export(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Export timeline")
            .initial_name("SpoolCut.mkv")
            .modal(true)
            .build();
        let app = Rc::clone(self);
        dialog.save(
            Some(&self.ui.window),
            gio::Cancellable::NONE,
            move |result| {
                if let Ok(file) = result {
                    if let Some(path) = file.path() {
                        app.start_export(path);
                    } else {
                        app.ui.message("Choose a local .mkv destination.");
                    }
                }
            },
        );
    }

    fn start_export(&self, path: PathBuf) {
        let project = self.state.borrow().editor.project.clone();
        let tx = self.tx.clone();
        let cancel = self.start_job();
        self.ui
            .message("Checking the timeline and stream compatibility…");
        thread::spawn(move || {
            let result =
                export_with_progress(&project, Path::new("."), &path, &cancel, |progress| {
                    let _ = tx.send(JobEvent::Progress(progress));
                })
                .map(|()| path)
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(JobEvent::Exported(result));
        });
    }

    fn start_job(&self) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.state.borrow_mut().job = Some(Arc::clone(&cancel));
        self.ui.progress.set_fraction(0.0);
        self.controls();
        cancel
    }

    fn cancel_job(&self) {
        if let Some(cancel) = self.state.borrow().job.as_ref() {
            cancel.store(true, Ordering::Relaxed);
            self.ui.message("Cancelling…");
        }
    }

    fn export_progress(&self, progress: ExportProgress) {
        if self
            .state
            .borrow()
            .job
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
        {
            return;
        }
        let (message, fraction) = match progress {
            ExportProgress::Planning => (
                "Checking the timeline and stream compatibility…".into(),
                0.0,
            ),
            ExportProgress::Copying { completed, total } => (
                format!("Copying clip {} of {total}…", completed + 1),
                0.05 + 0.8 * completed as f64 / total as f64,
            ),
            ExportProgress::Joining => ("Joining copied clips…".into(), 0.85),
            ExportProgress::Verifying => ("Verifying the staged MKV…".into(), 0.95),
        };
        self.ui.message(&message);
        self.ui.progress.set_fraction(fraction);
    }

    fn update(self: &Rc<Self>) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                JobEvent::Indexed(intent, results) => self.indexed(intent, results),
                JobEvent::Progress(progress) => self.export_progress(progress),
                JobEvent::Exported(result) => {
                    self.state.borrow_mut().job = None;
                    match result {
                        Ok(path) => self.ui.message(&format!("Exported {}", path.display())),
                        Err(error) => self.ui.message(&format!("Export stopped. {error}")),
                    }
                    self.controls();
                }
            }
        }
        self.update_preview();
    }

    fn refresh(self: &Rc<Self>) {
        self.stop_preview();
        let (media, clips, selected_source) = {
            let mut state = self.state.borrow_mut();
            let segments = state.editor.segments();
            (state.segments, state.timeline_error) = match segments {
                Ok(segments) => (segments, None),
                Err(error) => (Vec::new(), Some(format!("{error:#}"))),
            };
            state.position = state.position.min(state.total());
            state.selected_clip = state
                .selected_clip
                .filter(|&i| i < state.editor.project.clips.len());
            state.selected_source = state
                .selected_source
                .take()
                .filter(|id| state.editor.project.sources.contains_key(id));
            let editor = &state.editor;
            let media: Vec<_> = editor
                .project
                .sources
                .iter()
                .map(|(id, path)| {
                    let detail = match editor.media.get(path) {
                        Some(Ok(media)) => format!(
                            "{} · {} safe cut points",
                            media.info.video_codec.to_uppercase(),
                            media.info.safe_in_pts.len()
                        ),
                        Some(Err(_)) => "Unavailable · Relink source".into(),
                        None => "Inspecting…".into(),
                    };
                    let error = editor
                        .media
                        .get(path)
                        .and_then(|result| result.as_ref().err().cloned());
                    (
                        id.clone(),
                        source_name(path),
                        detail,
                        error.unwrap_or_else(|| path.display().to_string()),
                    )
                })
                .collect();
            let clips: Vec<_> = editor
                .project
                .clips
                .iter()
                .enumerate()
                .map(|(index, clip)| {
                    let name = editor
                        .project
                        .sources
                        .get(&clip.source)
                        .map_or_else(|| clip.source.clone(), |path| source_name(path));
                    let duration = state
                        .segments
                        .get(index)
                        .map(|segment| segment.end_us - segment.start_us);
                    (name, duration)
                })
                .collect();
            (media, clips, state.selected_source.clone())
        };

        self.rebuilding_bin.set(true);
        while let Some(child) = self.ui.media.first_child() {
            self.ui.media.remove(&child);
        }
        for (id, name, detail, tooltip) in media {
            let row = gtk::ListBoxRow::new();
            let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
            text.set_margin_start(9);
            text.set_margin_end(9);
            text.set_margin_top(9);
            text.set_margin_bottom(9);
            let name = gtk::Label::new(Some(&name));
            name.set_xalign(0.0);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let detail = gtk::Label::new(Some(&detail));
            detail.set_xalign(0.0);
            detail.add_css_class("dim-label");
            text.append(&name);
            text.append(&detail);
            row.set_tooltip_text(Some(&tooltip));
            row.set_child(Some(&text));
            self.ui.media.append(&row);
            if selected_source.as_ref() == Some(&id) {
                self.ui.media.select_row(Some(&row));
            }
        }
        self.rebuilding_bin.set(false);

        while let Some(child) = self.ui.lane.first_child() {
            self.ui.lane.remove(&child);
        }
        if clips.is_empty() {
            self.ui.lane.append(&self.ui.lane_hint);
        }
        for (index, (name, duration)) in clips.into_iter().enumerate() {
            let button = widgets::clip_button(&name, duration);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.select_clip(index);
                }
            });
            self.ui.lane.append(&button);
        }
        self.title();
        self.controls();
    }

    fn title(&self) {
        let state = self.state.borrow();
        let name = state
            .editor
            .path
            .as_deref()
            .map(source_name)
            .unwrap_or_else(|| "Untitled".into());
        let changed = if state.editor.dirty() { " •" } else { "" };
        self.ui
            .window
            .set_title(Some(&format!("{name}{changed} — SpoolCut")));
    }

    fn controls(&self) {
        let state = self.state.borrow();
        let selected = state.selected_clip;
        let busy = state.job.is_some();
        let count = state.editor.project.clips.len();
        let has_source = state
            .selected_source
            .as_deref()
            .is_some_and(|id| state.editor.source(id).is_ok());
        let preview = self.preview.borrow().is_some();
        let valid_timeline = !state.segments.is_empty();
        for button in [
            &self.ui.new,
            &self.ui.open,
            &self.ui.save,
            &self.ui.save_as,
            &self.ui.import,
        ] {
            button.set_sensitive(!busy);
        }
        self.ui.export.set_sensitive(!busy && valid_timeline);
        self.ui
            .export
            .set_tooltip_text(state.timeline_error.as_deref());
        self.ui.progress.set_visible(busy);
        self.ui.cancel.set_visible(busy);
        self.ui.append.set_sensitive(!busy && has_source);
        self.ui
            .relink
            .set_sensitive(!busy && state.selected_source.is_some());
        self.ui.undo.set_sensitive(!busy && state.editor.can_undo());
        self.ui.redo.set_sensitive(!busy && state.editor.can_redo());
        self.ui.play.set_sensitive(preview && valid_timeline);
        let placeholder = if !preview && self.ui.canvas.is_realized() {
            "Preview unavailable"
        } else if valid_timeline {
            "Select a clip or press Play"
        } else if count > 0 {
            "Resolve unavailable clips to preview the timeline"
        } else {
            "Add a clip to preview the timeline"
        };
        self.ui.placeholder.set_text(placeholder);
        self.ui
            .play
            .set_label(if state.playing { "Pause" } else { "Play" });
        self.ui.scale.set_sensitive(preview && valid_timeline);
        self.ui
            .scale
            .set_range(0.0, seconds(state.total()).max(0.001));
        self.ui.scale.set_value(seconds(state.position));
        self.ui
            .clock(seconds(state.position), seconds(state.total()));
        self.ui.trim_in.set_sensitive(!busy && selected.is_some());
        self.ui.trim_out.set_sensitive(!busy && selected.is_some());
        self.ui
            .split
            .set_sensitive(!busy && state.at(state.position).is_some());
        self.ui
            .earlier
            .set_sensitive(!busy && selected.is_some_and(|i| i > 0));
        self.ui
            .later
            .set_sensitive(!busy && selected.is_some_and(|i| i + 1 < count));
        self.ui.remove.set_sensitive(!busy && selected.is_some());
        let detail = selected
            .and_then(|index| state.editor.project.clips.get(index))
            .map(|clip| {
                state.editor.source(&clip.source).map_or_else(
                    |error| format!("{error:#}"),
                    |(_, media)| {
                        let base = media.info.video_time_base;
                        format!(
                            "In {}   Out {}",
                            timecode(base.seconds(clip.in_pts)),
                            timecode(base.seconds(clip.out_pts))
                        )
                    },
                )
            });
        let label = detail
            .as_deref()
            .or(state.timeline_error.as_deref())
            .unwrap_or("");
        self.ui.clip_details.set_text(label);
        self.ui.clip_details.set_tooltip_text(Some(label));
        let mut child = self.ui.lane.first_child();
        let mut index = 0;
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(button) = widget.downcast::<gtk::ToggleButton>() {
                button.set_active(selected == Some(index));
                index += 1;
            }
        }
    }
}

impl State {
    fn total(&self) -> i64 {
        self.segments.last().map_or(0, |segment| segment.end_us)
    }

    fn at(&self, position: i64) -> Option<usize> {
        if self.segments.is_empty() {
            return None;
        }
        Some(
            self.segments
                .partition_point(|segment| segment.end_us <= position)
                .min(self.segments.len() - 1),
        )
    }
}

impl App {
    fn select_clip(&self, index: usize) {
        let position = {
            let mut state = self.state.borrow_mut();
            state.selected_clip = Some(index);
            state.segments.get(index).map(|segment| segment.start_us)
        };
        if let Some(position) = position {
            self.request_preview(position, false);
        }
        self.controls();
    }

    fn request_preview(&self, position: i64, playing: bool) {
        if self.preview.borrow().is_none() {
            return;
        }
        let mut state = self.state.borrow_mut();
        state.position = position.clamp(0, state.total());
        state.playing = playing;
        state.pending = Some((state.position, playing));
        self.show_playhead(&state);
    }

    fn show_playhead(&self, state: &State) {
        self.ui.scale.set_value(seconds(state.position));
        self.ui
            .clock(seconds(state.position), seconds(state.total()));
        self.ui
            .play
            .set_label(if state.playing { "Pause" } else { "Play" });
    }

    fn stop_preview(&self) {
        if let Some(player) = self.preview.borrow().as_ref() {
            player.stop();
        }
        let mut state = self.state.borrow_mut();
        state.pending = None;
        state.active = None;
        state.started_id = None;
        state.playing = false;
        self.ui.placeholder.set_visible(true);
    }

    fn update_preview(&self) {
        let preview = self.preview.borrow();
        let Some(player) = preview.as_ref() else {
            return;
        };
        let mut state = self.state.borrow_mut();
        let previous = (state.position, state.playing);
        for event in player.events() {
            match event {
                Event::Started(id) => state.started_id = Some(id),
                Event::Ready => {
                    let started = state.started_id;
                    if let Some(active) = state
                        .active
                        .as_mut()
                        .filter(|active| Some(active.id) == started)
                    {
                        active.loaded = true;
                        active.ready = true;
                        self.ui.placeholder.set_visible(false);
                        self.ui.canvas.queue_render();
                    }
                }
                Event::Ended { id, eof }
                    if state.active.as_ref().is_some_and(|active| active.id == id) =>
                {
                    let index = state.active.take().unwrap().index;
                    if state.pending.is_none() && eof && state.playing {
                        if let Some(next) = state.segments.get(index + 1) {
                            state.position = next.start_us;
                            state.pending = Some((state.position, true));
                        } else {
                            state.position = state.total();
                            state.playing = false;
                        }
                    }
                }
                Event::Failed { id, message }
                    if state.active.as_ref().is_some_and(|active| active.id == id) =>
                {
                    state.active = None;
                    state.playing = false;
                    self.ui.message(&format!("Preview failed. {message}"));
                }
                _ => {}
            }
        }
        if let Some((position, playing)) = state.pending.take() {
            if let Err(error) = Self::seek(player, &mut state, position, playing) {
                state.active = None;
                state.playing = false;
                self.ui.message(&format!("Preview failed. {error:#}"));
            }
        } else if state.playing
            && let Some(active) = state.active.as_ref().filter(|active| active.ready)
            && let Some(position) = player.position()
        {
            let segment = &state.segments[active.index];
            state.position = (segment.start_us
                + ((position - segment.source_start) * 1_000_000.0).round() as i64)
                .clamp(segment.start_us, segment.end_us);
        }
        if previous != (state.position, state.playing) {
            self.show_playhead(&state);
        }
        if player.wants_frame() {
            self.ui.canvas.queue_render();
        }
    }

    fn seek(player: &Preview, state: &mut State, position: i64, playing: bool) -> Result<()> {
        let index = state
            .at(position)
            .context("The timeline has no playable ranges")?;
        let segment = &state.segments[index];
        let clip = &state.editor.project.clips[index];
        let media = state.editor.source(&clip.source)?.1;
        let timestamp = segment.source_start + seconds(position - segment.start_us);
        let frame = media.index.frame_at(clip, timestamp);
        if let Some(active) = state
            .active
            .as_mut()
            .filter(|active| active.index == index && frame >= 0.0)
        {
            if active.loaded {
                player.seek(frame, playing)?;
                active.ready = false;
            } else {
                state.pending = Some((position, playing));
            }
        } else {
            media.index.check_source()?;
            let id = player.load(&segment.path, frame, segment.source_end, playing)?;
            state.active = Some(ActivePreview {
                id,
                index,
                loaded: false,
                ready: false,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
