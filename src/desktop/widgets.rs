#[cfg(not(windows))]
use gtk::cairo;
use gtk::{gdk, glib, prelude::*};

use crate::{APP_ICON, APPLICATION_ID, timeline::Timeline};

pub struct Widgets {
    pub window: gtk::ApplicationWindow,
    pub canvas: gtk::GLArea,
    pub software_canvas: gtk::Picture,
    pub preview_stack: gtk::Stack,
    pub placeholder: gtk::Label,
    pub media_bin: gtk::Box,
    pub media: gtk::ListBox,
    pub timeline: Timeline,
    pub zoom_out: gtk::Button,
    pub zoom_in: gtk::Button,
    pub zoom_fit: gtk::Button,
    pub clip_details: gtk::Label,
    pub clock: gtk::Label,
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
    pub split: gtk::Button,
    pub remove: gtk::Button,
    pub undo: gtk::Button,
    pub redo: gtk::Button,
}

impl Widgets {
    pub fn new(application: &gtk::Application) -> Self {
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title("SpoolCut")
            .icon_name(APPLICATION_ID)
            .default_width(1120)
            .default_height(760)
            .build();
        let icon_texture = gdk::Texture::from_bytes(&glib::Bytes::from_static(APP_ICON))
            .expect("the embedded SpoolCut icon is a valid PNG");
        let window_icon = icon_texture.clone();
        window.connect_realize(move |window| {
            if let Some(toplevel) = window.surface().and_downcast::<gdk::Toplevel>() {
                toplevel.set_icon_list(std::slice::from_ref(&window_icon));
            }
        });
        let dark = gtk::Settings::default()
            .is_some_and(|settings| settings.is_gtk_application_prefer_dark_theme());
        let provider = gtk::CssProvider::new();
        provider.load_from_data(CSS);
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
        #[cfg(not(windows))]
        {
            let header = gtk::HeaderBar::new();
            header.set_show_title_buttons(false);
            header.set_title_widget(Some(&window_title(&window, &icon_texture)));
            for button in [&new, &open, &save, &save_as] {
                header.pack_start(button);
            }
            header.pack_end(&window_controls(&window));
            header.pack_end(&export);
            window.set_titlebar(Some(&header));
        }

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
        let media_bin = gtk::Box::new(gtk::Orientation::Vertical, 10);
        media_bin.append(&bin_header);
        media_bin.append(&media_scroll);
        media_bin.append(&bin_footer);

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
        let software_canvas = gtk::Picture::builder()
            .hexpand(true)
            .vexpand(true)
            .can_shrink(true)
            .content_fit(gtk::ContentFit::Contain)
            .build();
        let preview_stack = gtk::Stack::new();
        preview_stack.add_child(&canvas);
        preview_stack.add_child(&software_canvas);
        if crate::graphics::prefer_opengl() {
            preview_stack.set_visible_child(&canvas);
        } else {
            preview_stack.set_visible_child(&software_canvas);
        }
        video.set_child(Some(&preview_stack));
        video.add_overlay(&placeholder);
        let play = gtk::Button::with_label("Play");
        play.set_width_request(80);
        let clock = gtk::Label::new(None);
        clock.add_css_class("timecode");
        clock.set_hexpand(true);
        clock.set_xalign(1.0);
        let playback = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        playback.append(&play);
        playback.append(&clock);
        let preview = gtk::Box::new(gtk::Orientation::Vertical, 8);
        preview.set_hexpand(true);
        preview.set_width_request(300);
        preview.append(&video);
        preview.append(&playback);
        let workspace = gtk::Paned::builder()
            .orientation(gtk::Orientation::Horizontal)
            .start_child(&media_bin)
            .end_child(&preview)
            .resize_start_child(false)
            .shrink_start_child(false)
            .shrink_end_child(false)
            .wide_handle(true)
            .position(258)
            .vexpand(true)
            .build();
        workspace.add_css_class("media-workspace");

        let timeline = Timeline::new(dark);
        let undo = gtk::Button::with_label("Undo");
        undo.set_tooltip_text(Some("Undo (Ctrl+Z)"));
        let redo = gtk::Button::with_label("Redo");
        redo.set_tooltip_text(Some("Redo (Ctrl+Shift+Z)"));
        let zoom_out = gtk::Button::with_label("−");
        zoom_out.set_tooltip_text(Some("Zoom out (Ctrl + scroll)"));
        let zoom_in = gtk::Button::with_label("+");
        zoom_in.set_tooltip_text(Some("Zoom in (Ctrl + scroll)"));
        let zoom_fit = gtk::Button::with_label("Fit");
        zoom_fit.set_tooltip_text(Some("Fit the timeline to the window"));
        let split = gtk::Button::with_label("Split at playhead");
        split.set_tooltip_text(Some("Split at the nearest safe cut (S)"));
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
        let zoom = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        zoom.add_css_class("linked");
        zoom.append(&zoom_out);
        zoom.append(&zoom_fit);
        zoom.append(&zoom_in);
        timeline_title.append(&zoom);
        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        tools.append(&split);
        tools.append(&remove);
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
        #[cfg(windows)]
        {
            let project_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            project_buttons.set_hexpand(true);
            for button in [&new, &open, &save, &save_as] {
                project_buttons.append(button);
            }
            let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            toolbar.append(&project_buttons);
            toolbar.append(&export);
            content.append(&toolbar);
        }
        content.append(&workspace);
        content.append(&timeline_title);
        content.append(&timeline.area);
        content.append(&timeline.scrollbar);
        content.append(&tools);
        content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content.append(&footer);
        window.set_child(Some(&content));

        Self {
            window,
            canvas,
            software_canvas,
            preview_stack,
            placeholder,
            media_bin,
            media,
            timeline,
            zoom_out,
            zoom_in,
            zoom_fit,
            clip_details,
            clock,
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
            split,
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

#[derive(Clone, Copy)]
#[cfg(not(windows))]
enum WindowControl {
    Minimize,
    Maximize,
    Close,
}

#[cfg(not(windows))]
fn window_title(window: &gtk::ApplicationWindow, texture: &gdk::Texture) -> gtk::Box {
    let icon = gtk::Image::from_paintable(Some(texture));
    icon.set_pixel_size(26);
    let label = gtk::Label::builder()
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    label.add_css_class("title");
    window
        .bind_property("title", &label, "label")
        .sync_create()
        .build();
    let title = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    title.append(&icon);
    title.append(&label);
    title
}

#[cfg(not(windows))]
fn window_controls(window: &gtk::ApplicationWindow) -> gtk::Box {
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    for (kind, label, action) in [
        (WindowControl::Minimize, "Minimize", "window.minimize"),
        (
            WindowControl::Maximize,
            "Maximize",
            "window.toggle-maximized",
        ),
        (WindowControl::Close, "Close", "window.close"),
    ] {
        let glyph = gtk::DrawingArea::builder()
            .content_width(14)
            .content_height(14)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build();
        let button = gtk::Button::builder()
            .child(&glyph)
            .action_name(action)
            .width_request(32)
            .build();
        button.add_css_class("flat");
        label_window_button(&button, label);
        let weak_window = window.downgrade();
        glyph.set_draw_func(move |glyph, cr, width, height| {
            let maximized = weak_window
                .upgrade()
                .is_some_and(|window| window.is_maximized());
            let color = glyph.color();
            cr.set_source_rgba(
                f64::from(color.red()),
                f64::from(color.green()),
                f64::from(color.blue()),
                f64::from(color.alpha()),
            );
            cr.translate(f64::from(width) / 2.0, f64::from(height) / 2.0);
            draw_window_glyph(cr, kind, maximized);
        });
        if matches!(kind, WindowControl::Maximize) {
            let weak_button = button.downgrade();
            window.connect_maximized_notify(move |window| {
                if let Some(button) = weak_button.upgrade() {
                    let label = if window.is_maximized() {
                        "Restore"
                    } else {
                        "Maximize"
                    };
                    label_window_button(&button, label);
                    if let Some(glyph) = button.child() {
                        glyph.queue_draw();
                    }
                }
            });
        }
        controls.append(&button);
    }
    controls
}

#[cfg(not(windows))]
fn label_window_button(button: &gtk::Button, label: &str) {
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
}

#[cfg(not(windows))]
fn draw_window_glyph(cr: &cairo::Context, kind: WindowControl, maximized: bool) {
    cr.set_line_width(1.5);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    match kind {
        WindowControl::Minimize => {
            cr.move_to(-4.5, 3.0);
            cr.line_to(4.5, 3.0);
        }
        WindowControl::Maximize if maximized => {
            cr.rectangle(-5.0, -2.0, 7.0, 7.0);
            cr.move_to(-2.0, -2.0);
            cr.line_to(-2.0, -5.0);
            cr.line_to(5.0, -5.0);
            cr.line_to(5.0, 2.0);
            cr.line_to(2.0, 2.0);
        }
        WindowControl::Maximize => cr.rectangle(-4.5, -4.5, 9.0, 9.0),
        WindowControl::Close => {
            cr.move_to(-4.0, -4.0);
            cr.line_to(4.0, 4.0);
            cr.move_to(4.0, -4.0);
            cr.line_to(-4.0, 4.0);
        }
    }
    let _ = cr.stroke();
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
.media-unavailable { color: @error_color; font-weight: bold; }
.media-workspace > separator {
    background: transparent;
    border: none;
    box-shadow: none;
}
.timeline { border-radius: 6px; }
.timecode { font-family: monospace; font-variant-numeric: tabular-nums; }
";
