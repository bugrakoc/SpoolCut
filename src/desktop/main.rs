#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

mod editor;
mod graphics;
mod interactive;
#[cfg(target_os = "linux")]
mod launcher;
mod preview;
mod timeline;
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
use gtk::{gdk, gio, glib, prelude::*};
use spoolcut::{Clip, ExportProgress, Project, export_with_progress};

use editor::{Editor, IndexedMedia, Segment, source_name, timecode};
use preview::{Event, Preview};
use timeline::{ClipVisual, Draft, Edge, Hit};
use widgets::{Widgets, seconds};

const APPLICATION_ID: &str = "io.github.bugrakoc.SpoolCut";
const APP_ICON: &[u8] = include_bytes!("../../assets/io.github.bugrakoc.SpoolCut.png");

#[derive(Default)]
struct State {
    editor: Editor,
    selected_source: Option<String>,
    selected_clip: Option<usize>,
    clipboard: Option<(Clip, PathBuf)>,
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

struct TimelineDrag {
    origin_x: f64,
    origin_time: i64,
    pointer_x: f64,
    moved: bool,
    mode: DragMode,
}

enum DragMode {
    Scrub {
        snap_click: bool,
    },
    Reorder {
        clip: usize,
        slot: usize,
    },
    Trim {
        clip: usize,
        edge: Edge,
        original_time: f64,
        cuts: Vec<(f64, i64)>,
        snapped_pts: Option<i64>,
    },
}

struct App {
    ui: Widgets,
    state: RefCell<State>,
    preview: RefCell<Option<Preview>>,
    preview_error: RefCell<Option<String>>,
    fallback_queued: Cell<bool>,
    tx: mpsc::Sender<JobEvent>,
    rx: mpsc::Receiver<JobEvent>,
    closed: Cell<bool>,
    rebuilding_bin: Cell<bool>,
    timeline_drag: RefCell<Option<TimelineDrag>>,
}

fn initialize_locale() {
    gtk::disable_setlocale();
    // SAFETY: Called at startup before GTK or libmpv starts worker threads.
    // Keep the user's locale except for libmpv's required numeric locale.
    unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
        libc::setlocale(libc::LC_NUMERIC, c"C".as_ptr());
    }
}

fn main() -> glib::ExitCode {
    initialize_locale();
    glib::set_application_name("SpoolCut");
    glib::set_prgname(Some(APPLICATION_ID));
    let application =
        gtk::Application::new(Some(APPLICATION_ID), gio::ApplicationFlags::HANDLES_OPEN);
    #[cfg(target_os = "linux")]
    application.connect_startup(|_| {
        if let Err(error) = launcher::register_appimage() {
            eprintln!("Could not register the SpoolCut desktop launcher: {error:#}");
        }
    });
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
            preview_error: RefCell::new(None),
            fallback_queued: Cell::new(false),
            tx,
            rx,
            closed: Cell::new(false),
            rebuilding_bin: Cell::new(false),
            timeline_drag: RefCell::new(None),
        });
        app.connect();
        if !graphics::prefer_opengl() {
            app.schedule_software_preview(None);
        }
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
        self.bind(&self.ui.undo, |app| app.edit_history(false));
        self.bind(&self.ui.redo, |app| app.edit_history(true));
        self.bind(&self.ui.play, |app| app.toggle_play());
        self.bind(&self.ui.zoom_in, |app| app.ui.timeline.zoom(1.5, None));
        self.bind(&self.ui.zoom_out, |app| {
            app.ui.timeline.zoom(1.0 / 1.5, None)
        });
        self.bind(&self.ui.zoom_fit, |app| app.ui.timeline.fit());
        self.bind(&self.ui.split, |app| app.split());
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
        let file_drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let weak = Rc::downgrade(self);
        file_drop.connect_drop(move |_, value, _, _| {
            let Ok(files) = value.get::<gdk::FileList>() else {
                return false;
            };
            weak.upgrade()
                .is_some_and(|app| app.import_files(files.files()))
        });
        self.ui.media_bin.add_controller(file_drop);
        self.connect_timeline();
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
            if !app.state.borrow().editor.dirty() {
                app.closed.set(true);
                return glib::Propagation::Proceed;
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
            if !graphics::prefer_opengl() {
                return;
            }
            area.make_current();
            let result = area
                .error()
                .map_or_else(Preview::new, |error| Err(error.into()));
            match result {
                Ok(player) => {
                    *app.preview.borrow_mut() = Some(player);
                    app.ui
                        .preview_stack
                        .set_tooltip_text(Some("OpenGL preview"));
                }
                Err(error) => app.schedule_software_preview(Some(error)),
            }
            app.controls();
        });
        let weak = Rc::downgrade(self);
        self.ui.canvas.connect_render(move |_, _| {
            let Some(app) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let result = app
                .preview
                .borrow()
                .as_ref()
                .filter(|player| !player.is_software())
                .map(Preview::draw);
            if let Some(Err(error)) = result {
                app.schedule_software_preview(Some(error));
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        self.ui.canvas.connect_unrealize(move |area| {
            if let Some(app) = weak.upgrade() {
                let mut preview = app.preview.borrow_mut();
                if preview.as_ref().is_some_and(|player| !player.is_software()) {
                    area.make_current();
                    preview.take();
                }
            }
        });
    }

    fn schedule_software_preview(self: &Rc<Self>, cause: Option<anyhow::Error>) {
        if self.fallback_queued.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            let Some(app) = weak.upgrade() else { return };
            app.fallback_queued.set(false);
            if app.closed.get()
                || app
                    .preview
                    .borrow()
                    .as_ref()
                    .is_some_and(Preview::is_software)
            {
                return;
            }
            if app.preview.borrow().is_some() {
                app.ui.canvas.make_current();
                app.preview.borrow_mut().take();
            }
            let (position, playing) = {
                let mut state = app.state.borrow_mut();
                state.active = None;
                state.started_id = None;
                state.pending = None;
                (state.position, state.playing)
            };
            match Preview::new_software() {
                Ok(player) => {
                    let tooltip = if let Some(cause) = cause {
                        eprintln!(
                            "OpenGL preview unavailable, using software rendering: {cause:#}"
                        );
                        format!("Software preview\nOpenGL unavailable: {cause:#}")
                    } else {
                        "Software preview".into()
                    };
                    app.ui.preview_stack.set_tooltip_text(Some(&tooltip));
                    *app.preview.borrow_mut() = Some(player);
                    app.ui
                        .preview_stack
                        .set_visible_child(&app.ui.software_canvas);
                    if !app.state.borrow().segments.is_empty() {
                        app.request_preview(position, playing);
                    }
                }
                Err(error) => {
                    let message = match cause {
                        Some(cause) => {
                            format!("OpenGL preview: {cause:#}. Software preview: {error:#}")
                        }
                        None => format!("Software preview: {error:#}"),
                    };
                    eprintln!("{message}");
                    *app.preview_error.borrow_mut() = Some(message.clone());
                    app.state.borrow_mut().playing = false;
                    app.ui.message(&message);
                }
            }
            app.controls();
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
        self.ui.timeline.fit();
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
                self.ui.timeline.fit();
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
                    let files = (0..files.n_items())
                        .filter_map(|i| files.item(i))
                        .filter_map(|item| item.downcast::<gio::File>().ok());
                    app.import_files(files);
                }
            },
        );
    }

    fn import_files(&self, files: impl IntoIterator<Item = gio::File>) -> bool {
        if self.closed.get() || self.state.borrow().job.is_some() {
            return false;
        }
        let paths: Vec<_> = files.into_iter().filter_map(|file| file.path()).collect();
        if paths.is_empty() {
            self.ui.message("Choose local MP4 or MOV files.");
            return false;
        }
        self.start_index(paths, IndexIntent::Import);
        true
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
        let selection = {
            let state = self.state.borrow();
            state
                .selected_source
                .clone()
                .map(|source| (source, state.editor.project.clips.len()))
        };
        if let Some((source, end)) = selection {
            self.insert_source(&source, end);
        }
    }

    fn insert_source(self: &Rc<Self>, source: &str, slot: usize) -> bool {
        let result = {
            let state = self.state.borrow();
            if state.job.is_some() {
                return false;
            }
            state
                .editor
                .source(source)
                .and_then(|(_, media)| media.initial_clip(source))
        };
        match result {
            Ok(clip) => {
                let index = {
                    let mut state = self.state.borrow_mut();
                    let index = slot.min(state.editor.project.clips.len());
                    state.selected_clip = Some(index);
                    state
                        .editor
                        .change(|project| project.clips.insert(index, clip));
                    index
                };
                self.refresh();
                self.select_clip(index);
                true
            }
            Err(error) => {
                self.ui.message(&format!("{error:#}"));
                false
            }
        }
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
                    anyhow::ensure!(
                        position > segment.start_us && position < segment.end_us,
                        "The playhead is already at a clip boundary"
                    );
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

    fn copy_clip(self: &Rc<Self>, cut: bool) {
        {
            let mut state = self.state.borrow_mut();
            let Some(clip) = state
                .selected_clip
                .and_then(|index| state.editor.project.clips.get(index))
            else {
                return;
            };
            let Some(path) = state.editor.project.sources.get(&clip.source) else {
                return;
            };
            state.clipboard = Some((clip.clone(), path.clone()));
        }
        if cut {
            self.remove_clip();
        }
        self.ui
            .message(if cut { "Clip cut." } else { "Clip copied." });
    }

    fn paste_clip(self: &Rc<Self>) {
        let slot = {
            let mut state = self.state.borrow_mut();
            let Some((clip, path)) = state.clipboard.clone() else {
                return;
            };
            let slot = state.at(state.position).map_or_else(
                || {
                    state
                        .selected_clip
                        .map_or(state.editor.project.clips.len(), |i| i + 1)
                },
                |index| {
                    let segment = &state.segments[index];
                    let from_start = state.position - segment.start_us;
                    let from_end = segment.end_us - state.position;
                    index + usize::from(from_start >= from_end)
                },
            );
            state.editor.paste(slot, clip, &path);
            state.selected_clip = Some(slot);
            slot
        };
        self.refresh();
        self.select_clip(slot);
        self.ui.message("Clip pasted.");
    }

    fn edit_history(self: &Rc<Self>, redo: bool) {
        {
            let mut state = self.state.borrow_mut();
            if redo && state.editor.can_redo() {
                state.editor.redo();
            } else if !redo && state.editor.can_undo() {
                state.editor.undo();
            } else {
                return;
            }
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
        self.pan_during_drag();
        self.update_preview();
    }

    fn refresh(self: &Rc<Self>) {
        self.cancel_timeline_drag();
        self.stop_preview();
        let (media, clips, selected_source, timed) = {
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
                    let (detail, error) = match editor.media.get(path) {
                        Some(Ok(media)) => (media_details(&media.info), None),
                        Some(Err(error)) => ("Unavailable · Relink source".into(), Some(error)),
                        None => ("Inspecting…".into(), None),
                    };
                    (
                        id.clone(),
                        source_name(path),
                        detail,
                        error.is_some(),
                        error.cloned().unwrap_or_else(|| path.display().to_string()),
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
                    let (start, end, keyframes) = state.segments.get(index).map_or_else(
                        || {
                            (
                                index as i64 * 2_000_000,
                                (index as i64 + 1) * 2_000_000,
                                Rc::default(),
                            )
                        },
                        |segment| {
                            (
                                segment.start_us,
                                segment.end_us,
                                Rc::clone(&segment.keyframes),
                            )
                        },
                    );
                    let color = editor
                        .project
                        .sources
                        .keys()
                        .position(|id| *id == clip.source)
                        .unwrap_or(0);
                    ClipVisual {
                        name,
                        color,
                        start,
                        end,
                        keyframes,
                    }
                })
                .collect();
            (
                media,
                clips,
                state.selected_source.clone(),
                !state.segments.is_empty(),
            )
        };

        self.rebuilding_bin.set(true);
        while let Some(child) = self.ui.media.first_child() {
            self.ui.media.remove(&child);
        }
        for (id, name, detail, unavailable, tooltip) in media {
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
            detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
            detail.add_css_class(if unavailable {
                "media-unavailable"
            } else {
                "dim-label"
            });
            text.append(&name);
            text.append(&detail);
            row.set_tooltip_text(Some(&tooltip));
            row.set_child(Some(&text));
            let drag_source = gtk::DragSource::builder()
                .actions(gdk::DragAction::COPY)
                .build();
            let dragged_id = id.clone();
            drag_source.connect_prepare(move |_, _, _| {
                Some(gdk::ContentProvider::for_value(&dragged_id.to_value()))
            });
            row.add_controller(drag_source);
            self.ui.media.append(&row);
            if selected_source.as_ref() == Some(&id) {
                self.ui.media.select_row(Some(&row));
            }
        }
        self.rebuilding_bin.set(false);

        self.ui.timeline.show_clips(clips, timed);
        if timed {
            let position = self.state.borrow().position;
            self.request_preview(position, false);
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
            .placeholder
            .set_tooltip_text(self.preview_error.borrow().as_deref());
        self.ui.play.set_tooltip_text(Some(
            self.preview_error
                .borrow()
                .as_deref()
                .unwrap_or("Play / pause (Space)"),
        ));
        self.ui
            .play
            .set_label(if state.playing { "Pause" } else { "Play" });
        self.ui.timeline.select(selected);
        self.ui.timeline.playhead(state.position, false);
        self.ui
            .clock(seconds(state.position), seconds(state.total()));
        self.ui
            .split
            .set_sensitive(!busy && state.at(state.position).is_some());
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
    }
}

fn media_details(info: &spoolcut::MediaInfo) -> String {
    let resolution = info.video_resolution.map_or_else(
        || "Unknown resolution".into(),
        |(width, height)| format!("{width} × {height}"),
    );
    let frame_rate = info.average_frame_rate.map_or_else(
        || "Unknown frame rate".into(),
        |rate| {
            let rate = format!(
                "{:.3}",
                f64::from(rate.numerator) / f64::from(rate.denominator)
            );
            format!("{} fps", rate.trim_end_matches('0').trim_end_matches('.'))
        },
    );
    let codec = info.video_codec.to_uppercase();
    format!("{resolution} · {codec} · {frame_rate}")
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
        let mut state = self.state.borrow_mut();
        state.position = position.clamp(0, state.total());
        let preview_available = self.preview.borrow().is_some() && !state.segments.is_empty();
        state.playing = preview_available && playing;
        state.pending = preview_available.then_some((state.position, playing));
        self.show_playhead(&state);
    }

    fn show_playhead(&self, state: &State) {
        self.ui.timeline.playhead(state.position, state.playing);
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
                        if !player.is_software() {
                            self.ui.canvas.queue_render();
                        }
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
            if player.is_software() {
                let picture = &self.ui.software_canvas;
                match player.draw_software(picture.width().max(1), picture.height().max(1)) {
                    Ok(texture) => picture.set_paintable(Some(&texture)),
                    Err(error) => self
                        .ui
                        .message(&format!("Preview rendering failed. {error:#}")),
                }
            } else {
                self.ui.canvas.queue_render();
            }
        }
    }

    fn seek(player: &Preview, state: &mut State, position: i64, playing: bool) -> Result<()> {
        let index = state
            .at(position)
            .context("The timeline has no playable ranges")?;
        let segment = &state.segments[index];
        let clip = &state.editor.project.clips[index];
        let media = state.editor.source(&clip.source)?.1;
        // A keyframe can round down when mapped to timeline microseconds.
        let timestamp = segment.source_start + seconds(position - segment.start_us) + 0.000_000_5;
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
