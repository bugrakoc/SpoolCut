use gtk::prelude::*;

pub struct Widgets {
    pub window: gtk::ApplicationWindow,
    pub canvas: gtk::GLArea,
    pub placeholder: gtk::Label,
    pub media: gtk::ListBox,
    pub lane: gtk::Box,
    pub lane_hint: gtk::Label,
    pub clip_details: gtk::Label,
    pub clock: gtk::Label,
    pub scale: gtk::Scale,
    pub status: gtk::Label,
    pub progress: gtk::ProgressBar,
    pub new: gtk::Button,
    pub open: gtk::Button,
    pub save: gtk::Button,
    pub save_as: gtk::Button,
    pub export: gtk::Button,
    pub cancel: gtk::Button,
    pub import: gtk::Button,
    pub append: gtk::Button,
    pub relink: gtk::Button,
    pub play: gtk::Button,
    pub trim_in: gtk::Button,
    pub trim_out: gtk::Button,
    pub split: gtk::Button,
    pub earlier: gtk::Button,
    pub later: gtk::Button,
    pub remove: gtk::Button,
    pub undo: gtk::Button,
    pub redo: gtk::Button,
}

impl Widgets {
    pub fn new(application: &gtk::Application) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("SpoolCut")
            .default_width(1120)
            .default_height(760)
            .build();
        let dark = gtk::Settings::default()
            .is_some_and(|settings| settings.is_gtk_application_prefer_dark_theme());
        let provider = gtk::CssProvider::new();
        provider.load_from_data(if dark { DARK_CSS } else { CSS });
        gtk::style_context_add_provider_for_display(
            &WidgetExt::display(&window),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let new = gtk::Button::with_label("New");
        let open = gtk::Button::with_label("Open…");
        let save = gtk::Button::with_label("Save");
        let save_as = gtk::Button::with_label("Save As…");
        let export = gtk::Button::with_label("Export MKV…");
        export.add_css_class("suggested-action");
        let header = gtk::HeaderBar::new();
        header.pack_start(&new);
        header.pack_start(&open);
        header.pack_start(&save);
        header.pack_start(&save_as);
        header.pack_end(&export);
        window.set_titlebar(Some(&header));

        let media = gtk::ListBox::new();
        media.add_css_class("boxed-list");
        media.set_activate_on_single_click(false);
        let media_scroll = gtk::ScrolledWindow::builder()
            .child(&media)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let import = gtk::Button::with_label("Import…");
        let append = gtk::Button::with_label("Add to timeline");
        let relink = gtk::Button::with_label("Relink…");
        let bin_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = heading("Media bin");
        title.set_hexpand(true);
        bin_header.append(&title);
        bin_header.append(&import);
        let bin_footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        bin_footer.append(&append);
        bin_footer.append(&relink);
        let bin = gtk::Box::new(gtk::Orientation::Vertical, 10);
        bin.set_width_request(258);
        bin.append(&bin_header);
        bin.append(&media_scroll);
        bin.append(&bin_footer);

        let canvas = gtk::GLArea::builder()
            .auto_render(false)
            .hexpand(true)
            .vexpand(true)
            .build();
        canvas.set_required_version(3, 3);
        canvas.set_use_es(false);
        let placeholder = gtk::Label::new(Some("Add a clip to preview the timeline"));
        placeholder.add_css_class("preview-placeholder");
        let video = gtk::Overlay::new();
        video.add_css_class("video-frame");
        video.set_overflow(gtk::Overflow::Hidden);
        video.set_child(Some(&canvas));
        video.add_overlay(&placeholder);
        let play = gtk::Button::with_label("Play");
        play.set_width_request(80);
        let clock = gtk::Label::new(None);
        clock.add_css_class("timecode");
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.001);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.set_tooltip_text(Some("Scrub the timeline"));
        let playback = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        playback.append(&play);
        playback.append(&scale);
        playback.append(&clock);
        let preview = gtk::Box::new(gtk::Orientation::Vertical, 8);
        preview.set_hexpand(true);
        preview.append(&video);
        preview.append(&playback);
        let workspace = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        workspace.set_vexpand(true);
        workspace.append(&bin);
        workspace.append(&preview);

        let lane = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let lane_hint = gtk::Label::new(Some(
            "Add clips from the media bin to assemble your sequence.",
        ));
        lane_hint.add_css_class("dim-label");
        lane_hint.set_margin_start(12);
        lane.append(&lane_hint);
        let lane_scroll = gtk::ScrolledWindow::builder()
            .child(&lane)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .min_content_height(72)
            .build();
        lane_scroll.add_css_class("timeline-lane");
        let undo = gtk::Button::with_label("Undo");
        let redo = gtk::Button::with_label("Redo");
        let trim_in = gtk::Button::with_label("Trim in…");
        let trim_out = gtk::Button::with_label("Trim out…");
        let split = gtk::Button::with_label("Split at playhead");
        let earlier = gtk::Button::with_label("Move left");
        let later = gtk::Button::with_label("Move right");
        let remove = gtk::Button::with_label("Remove");
        let clip_details = gtk::Label::new(None);
        clip_details.set_xalign(0.0);
        clip_details.set_ellipsize(gtk::pango::EllipsizeMode::End);
        clip_details.add_css_class("timecode");

        let timeline_title = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let lane_heading = heading("Timeline");
        lane_heading.set_hexpand(true);
        timeline_title.append(&lane_heading);
        timeline_title.append(&undo);
        timeline_title.append(&redo);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        for button in [&trim_in, &trim_out, &split, &earlier, &later, &remove] {
            tools.append(button);
        }
        tools.append(&clip_details);

        let status = gtk::Label::new(Some("Import MP4 or MOV files to get started."));
        status.set_xalign(0.0);
        status.set_hexpand(true);
        status.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let progress = gtk::ProgressBar::new();
        progress.set_width_request(125);
        progress.set_visible(false);
        let cancel = gtk::Button::with_label("Cancel");
        cancel.set_visible(false);
        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        footer.append(&status);
        footer.append(&progress);
        footer.append(&cancel);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(14);
        content.set_margin_end(14);
        content.append(&workspace);
        content.append(&timeline_title);
        content.append(&lane_scroll);
        content.append(&tools);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&footer);
        window.set_child(Some(&content));

        Self {
            window,
            canvas,
            placeholder,
            media,
            lane,
            lane_hint,
            clip_details,
            clock,
            scale,
            status,
            progress,
            new,
            open,
            save,
            save_as,
            export,
            cancel,
            import,
            append,
            relink,
            play,
            trim_in,
            trim_out,
            split,
            earlier,
            later,
            remove,
            undo,
            redo,
        }
    }

    pub fn message(&self, text: &str) {
        self.status.set_text(text);
        self.status.set_tooltip_text(Some(text));
    }

    pub fn clock(&self, current: f64, total: f64) {
        self.clock.set_text(&format!(
            "{} / {}",
            super::editor::timecode(current),
            super::editor::timecode(total)
        ));
    }
}

fn heading(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    label
}

pub fn seconds(us: i64) -> f64 {
    us as f64 / 1_000_000.0
}

pub fn clip_button(name: &str, duration: Option<i64>) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::new();
    button.add_css_class("timeline-clip");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    let title = gtk::Label::new(Some(name));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    text.append(&title);
    let subtitle = duration
        .map(|us| super::editor::timecode(seconds(us)))
        .unwrap_or_else(|| "Unavailable".into());
    let detail = gtk::Label::new(Some(&subtitle));
    detail.set_xalign(0.0);
    detail.add_css_class("dim-label");
    text.append(&detail);
    button.set_child(Some(&text));
    button.set_width_request(
        duration.map_or(155, |us| (seconds(us) * 90.0).clamp(130.0, 540.0) as i32),
    );
    button.set_tooltip_text(Some(name));
    button
}

pub fn files(dialog: &gtk::FileDialog, label: &str, patterns: &[&str]) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(label));
    for pattern in patterns {
        filter.add_pattern(pattern);
    }
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));
}

const CSS: &str = "
.video-frame { background-color: #10151c; border-radius: 8px; }
.preview-placeholder { color: #d0d9e4; }
.timeline-lane { background-color: alpha(currentColor, 0.07); border-radius: 6px; }
.timeline-clip { border-radius: 0; border-right: 1px solid alpha(currentColor, 0.2); padding: 8px 11px; }
.timeline-clip:checked { background: #d8e7fc; color: #15395f; box-shadow: inset 0 -3px #2d65a2; }
.timecode { font-family: monospace; font-variant-numeric: tabular-nums; }
";
const DARK_CSS: &str = "
.video-frame { background-color: #10151c; border-radius: 8px; }
.preview-placeholder { color: #d0d9e4; }
.timeline-lane { background-color: alpha(currentColor, 0.07); border-radius: 6px; }
.timeline-clip { border-radius: 0; border-right: 1px solid alpha(currentColor, 0.2); padding: 8px 11px; }
.timeline-clip:checked { background: #264c72; color: #edf5ff; box-shadow: inset 0 -3px #88b7eb; }
.timecode { font-family: monospace; font-variant-numeric: tabular-nums; }
";
