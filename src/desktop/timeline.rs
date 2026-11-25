use std::{cell::RefCell, rc::Rc};

use gtk::{cairo, prelude::*};

use crate::{editor::timecode, widgets::seconds};

const INSET: f64 = 12.0;
const RULER: f64 = 29.0;
const CLIP_TOP: f64 = 39.0;
const CLIP_HEIGHT: f64 = 58.0;
const HANDLE: f64 = 8.0;
const US_PER_SECOND: f64 = 1_000_000.0;

pub struct ClipVisual {
    pub name: String,
    pub color: usize,
    pub start: i64,
    pub end: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    In,
    Out,
}

#[derive(Clone, Copy)]
pub enum Hit {
    Ruler,
    Clip(usize, Option<Edge>),
    Empty,
}

#[derive(Clone, Copy)]
pub enum Draft {
    Trim {
        clip: usize,
        start: i64,
        end: i64,
    },
    Move {
        slot: usize,
        clip: Option<(usize, i64)>,
    },
}

#[derive(Default)]
struct View {
    clips: Vec<ClipVisual>,
    timed: bool,
    zoom: Option<f64>,
    playhead: i64,
    selected: Option<usize>,
    draft: Option<Draft>,
}

impl View {
    fn duration(&self) -> i64 {
        self.clips.last().map_or(0, |clip| clip.end)
    }

    fn fit(&self, width: f64) -> f64 {
        let duration = seconds(self.duration());
        if duration > 0.0 {
            ((width - 2.0 * INSET).max(1.0) / duration).min(4000.0)
        } else {
            50.0
        }
    }

    fn scale(&self, width: f64) -> f64 {
        self.zoom.unwrap_or_else(|| self.fit(width))
    }
}

pub struct Timeline {
    pub area: gtk::DrawingArea,
    pub scrollbar: gtk::Scrollbar,
    offset: gtk::Adjustment,
    view: Rc<RefCell<View>>,
}

impl Timeline {
    pub fn new(dark: bool) -> Self {
        let area = gtk::DrawingArea::builder()
            .hexpand(true)
            .height_request(107)
            .focusable(true)
            .build();
        area.set_tooltip_text(Some(
            "Scrub the ruler, drag clips to reorder, or drag an edge to trim",
        ));
        area.add_css_class("timeline");
        let offset = gtk::Adjustment::new(0.0, 0.0, 1.0, 30.0, 200.0, 1.0);
        let scrollbar = gtk::Scrollbar::new(gtk::Orientation::Horizontal, Some(&offset));
        scrollbar.set_visible(false);
        let view = Rc::new(RefCell::new(View::default()));

        let draw_view = Rc::clone(&view);
        let draw_offset = offset.clone();
        area.set_draw_func(move |_, cr, width, height| {
            let width = f64::from(width);
            let scale = draw_view.borrow().scale(width);
            draw(
                &draw_view.borrow(),
                cr,
                width,
                f64::from(height),
                scale,
                draw_offset.value(),
                dark,
            );
        });
        let weak_area = area.downgrade();
        offset.connect_value_changed(move |_| {
            if let Some(area) = weak_area.upgrade() {
                area.queue_draw();
            }
        });
        let resize_view = Rc::clone(&view);
        let resize_offset = offset.clone();
        let resize_scrollbar = scrollbar.clone();
        area.connect_resize(move |_, width, _| {
            configure(
                &resize_view.borrow(),
                &resize_offset,
                &resize_scrollbar,
                f64::from(width),
            );
        });
        Self {
            area,
            scrollbar,
            offset,
            view,
        }
    }

    fn width(&self) -> f64 {
        f64::from(self.area.width()).max(1.0)
    }

    fn configure(&self) {
        configure(
            &self.view.borrow(),
            &self.offset,
            &self.scrollbar,
            self.width(),
        );
        self.area.queue_draw();
    }

    pub fn show_clips(&self, clips: Vec<ClipVisual>, timed: bool) {
        {
            let mut view = self.view.borrow_mut();
            view.clips = clips;
            view.timed = timed;
            view.draft = None;
        }
        self.configure();
    }

    pub fn fit(&self) {
        self.view.borrow_mut().zoom = None;
        self.configure();
        self.offset.set_value(0.0);
    }

    pub fn zoom(&self, factor: f64, anchor: Option<f64>) {
        let width = self.width();
        let mut view = self.view.borrow_mut();
        let scale = view.scale(width);
        let scale_next = (scale * factor).clamp((view.fit(width) / 16.0).min(1.0), 4000.0);
        let anchor = anchor.unwrap_or_else(|| {
            let playhead = INSET + seconds(view.playhead) * scale - self.offset.value();
            if (0.0..width).contains(&playhead) {
                playhead
            } else {
                width / 2.0
            }
        });
        let time_under_anchor = ((self.offset.value() + anchor - INSET) / scale).max(0.0);
        view.zoom = Some(scale_next);
        drop(view);
        self.configure();
        self.offset
            .set_value(INSET + time_under_anchor * scale_next - anchor);
    }

    pub fn pan(&self, by: f64) -> bool {
        let start = self.offset.value();
        self.offset.set_value(start + by);
        self.offset.value() != start
    }

    pub fn playhead(&self, position: i64, follow: bool) {
        self.view.borrow_mut().playhead = position;
        if follow {
            let width = self.width();
            let x = INSET + seconds(position) * self.scale() - self.offset.value();
            if x < INSET || x > width - INSET {
                self.pan(x - width * 0.28);
            }
        }
        self.area.queue_draw();
    }

    pub fn select(&self, index: Option<usize>) {
        self.view.borrow_mut().selected = index;
        self.area.queue_draw();
    }

    pub fn draft(&self, draft: Option<Draft>) {
        self.view.borrow_mut().draft = draft;
        self.area.queue_draw();
    }

    pub fn scale(&self) -> f64 {
        self.view.borrow().scale(self.width())
    }

    pub fn time_at(&self, x: f64) -> i64 {
        (((self.offset.value() + x - INSET) / self.scale()) * US_PER_SECOND).round() as i64
    }

    pub fn slot_at(&self, x: f64) -> usize {
        let time = self.time_at(x);
        self.view
            .borrow()
            .clips
            .partition_point(|clip| clip.start + (clip.end - clip.start) / 2 <= time)
    }

    pub fn clip_start(&self, index: usize) -> Option<i64> {
        self.view.borrow().clips.get(index).map(|clip| clip.start)
    }

    pub fn hit(&self, x: f64, y: f64) -> Hit {
        let view = self.view.borrow();
        if y < CLIP_TOP {
            return Hit::Ruler;
        }
        if y > CLIP_TOP + CLIP_HEIGHT {
            return Hit::Empty;
        }
        let time = self.time_at(x);
        let index = view.clips.partition_point(|clip| clip.end <= time);
        let Some(clip) = view.clips.get(index).filter(|clip| clip.start <= time) else {
            return Hit::Empty;
        };
        let edge = if view.timed {
            let scale = self.scale();
            let handle = HANDLE.min(seconds(clip.end - clip.start) * scale / 3.0);
            if seconds(time - clip.start) * scale <= handle {
                Some(Edge::In)
            } else if seconds(clip.end - time) * scale <= handle {
                Some(Edge::Out)
            } else {
                None
            }
        } else {
            None
        };
        Hit::Clip(index, edge)
    }
}

fn configure(view: &View, offset: &gtk::Adjustment, scrollbar: &gtk::Scrollbar, width: f64) {
    let width = width.max(1.0);
    let upper = (seconds(view.duration()) * view.scale(width) + 2.0 * INSET).max(width);
    offset.configure(offset.value(), 0.0, upper, 36.0, width * 0.85, width);
    scrollbar.set_sensitive(upper > width + 1.0);
    scrollbar.set_visible(!view.clips.is_empty());
}

type Color = (f64, f64, f64);

fn paint(cr: &cairo::Context, color: Color) {
    cr.set_source_rgb(color.0, color.1, color.2);
}

fn line(cr: &cairo::Context, x: f64, y1: f64, y2: f64) {
    let x = x.floor() + 0.5;
    cr.move_to(x, y1);
    cr.line_to(x, y2);
    let _ = cr.stroke();
}

fn tick_interval(scale: f64) -> i64 {
    let want = (105.0 * US_PER_SECOND / scale).max(1.0);
    let power = 10.0f64.powf(want.log10().floor());
    for unit in [1.0, 2.0, 5.0, 10.0] {
        if unit * power >= want {
            return (unit * power).round() as i64;
        }
    }
    i64::MAX
}

fn draw(
    view: &View,
    cr: &cairo::Context,
    width: f64,
    height: f64,
    scale: f64,
    offset: f64,
    dark: bool,
) {
    let (ruler, track, grid, text) = if dark {
        (
            (0.14, 0.16, 0.20),
            (0.095, 0.11, 0.14),
            (0.25, 0.27, 0.32),
            (0.82, 0.85, 0.90),
        )
    } else {
        (
            (0.91, 0.93, 0.95),
            (0.95, 0.96, 0.98),
            (0.79, 0.82, 0.86),
            (0.23, 0.29, 0.36),
        )
    };
    paint(cr, track);
    let _ = cr.paint();
    paint(cr, ruler);
    cr.rectangle(0.0, 0.0, width, RULER);
    let _ = cr.fill();
    cr.set_line_width(1.0);
    let x_at = |time| INSET + seconds(time) * scale - offset;

    if view.timed {
        let major = tick_interval(scale);
        let minor = (major / 5).max(1);
        let begin = (((offset - INSET).max(0.0) / scale) * US_PER_SECOND) as i64;
        let mut tick = begin / major * major;
        cr.select_font_face(
            "monospace",
            cairo::FontSlant::Normal,
            cairo::FontWeight::Normal,
        );
        cr.set_font_size(10.0);
        while x_at(tick) <= width {
            let x = x_at(tick);
            paint(cr, grid);
            line(cr, x, 18.0, height);
            paint(cr, text);
            let label = timecode(seconds(tick));
            let label = if major >= 1_000_000 {
                label.split('.').next().unwrap_or(&label)
            } else {
                &label
            };
            cr.move_to(x + 4.0, 14.0);
            let _ = cr.show_text(label);
            paint(cr, grid);
            for index in 1..5 {
                line(
                    cr,
                    x_at(tick.saturating_add(minor.saturating_mul(index))),
                    24.0,
                    RULER,
                );
            }
            let Some(next) = tick.checked_add(major) else {
                break;
            };
            tick = next;
        }
    } else if !view.clips.is_empty() {
        paint(cr, text);
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        cr.set_font_size(11.0);
        cr.move_to(INSET, 18.0);
        let _ = cr.show_text("Timeline timing unavailable");
    }

    let first = view.clips.partition_point(|clip| x_at(clip.end) < 0.0);
    for (index, clip) in view.clips.iter().enumerate().skip(first) {
        let left = x_at(clip.start);
        if left > width {
            break;
        }
        if matches!(view.draft, Some(Draft::Trim { clip: active, .. }) if index == active) {
            continue;
        }
        draw_clip(
            cr,
            clip,
            left,
            x_at(clip.end),
            view.selected == Some(index),
            view.timed,
        );
    }
    if let Some(draft) = view.draft {
        let accent = (0.25, 0.62, 1.0);
        paint(cr, accent);
        cr.set_line_width(2.0);
        match draft {
            Draft::Trim { clip, start, end } => {
                draw_clip(
                    cr,
                    &view.clips[clip],
                    x_at(start),
                    x_at(end),
                    true,
                    view.timed,
                );
            }
            Draft::Move { slot, clip } => {
                if let Some((index, start)) = clip {
                    let clip = &view.clips[index];
                    cr.push_group();
                    draw_clip(
                        cr,
                        clip,
                        x_at(start),
                        x_at(start.saturating_add(clip.end - clip.start)),
                        true,
                        view.timed,
                    );
                    let _ = cr.pop_group_to_source();
                    let _ = cr.paint_with_alpha(0.82);
                }
                let insertion = view
                    .clips
                    .get(slot)
                    .map_or_else(|| view.duration(), |clip| clip.start);
                paint(cr, accent);
                line(
                    cr,
                    x_at(insertion),
                    CLIP_TOP - 3.0,
                    CLIP_TOP + CLIP_HEIGHT + 3.0,
                );
            }
        }
    }

    if view.timed && !view.clips.is_empty() {
        let x = x_at(view.playhead);
        paint(cr, (0.87, 0.30, 0.12));
        cr.set_line_width(1.8);
        line(cr, x, 6.0, height);
        cr.move_to(x - 6.0, 0.0);
        cr.line_to(x + 6.0, 0.0);
        cr.line_to(x + 6.0, 7.0);
        cr.line_to(x, 13.0);
        cr.line_to(x - 6.0, 7.0);
        cr.close_path();
        let _ = cr.fill();
    }
}

fn draw_clip(
    cr: &cairo::Context,
    clip: &ClipVisual,
    left: f64,
    right: f64,
    selected: bool,
    timed: bool,
) {
    let width = right - left;
    let _ = cr.save();
    cr.rectangle(left, CLIP_TOP, width, CLIP_HEIGHT);
    cr.clip();
    let colors = [
        (0.16, 0.33, 0.59),
        (0.13, 0.42, 0.38),
        (0.39, 0.29, 0.57),
        (0.50, 0.34, 0.21),
    ];
    paint(cr, colors[clip.color % colors.len()]);
    let _ = cr.paint();

    if selected {
        paint(cr, (0.48, 0.76, 1.0));
        cr.rectangle(
            left + 1.0,
            CLIP_TOP + 1.0,
            (width - 2.0).max(0.0),
            CLIP_HEIGHT - 2.0,
        );
        cr.set_line_width(2.0);
        let _ = cr.stroke();
    }
    if width >= 18.0 {
        cr.set_source_rgba(1.0, 1.0, 1.0, if selected { 0.36 } else { 0.17 });
        let handle = HANDLE.min(width / 3.0);
        cr.rectangle(left + 2.0, CLIP_TOP + 9.0, handle - 4.0, CLIP_HEIGHT - 18.0);
        cr.rectangle(
            right - handle + 2.0,
            CLIP_TOP + 9.0,
            handle - 4.0,
            CLIP_HEIGHT - 18.0,
        );
        let _ = cr.fill();
    }
    if width >= 26.0 {
        paint(cr, (1.0, 1.0, 1.0));
        let text_x = left + 11.0;
        let mut name = clip.name.replace('\0', "�");
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        cr.set_font_size(12.0);
        if cr
            .text_extents(&name)
            .is_ok_and(|ext| ext.x_advance() > width - 22.0)
        {
            while !name.is_empty()
                && cr
                    .text_extents(&format!("{name}…"))
                    .is_ok_and(|ext| ext.x_advance() > width - 22.0)
            {
                name.pop();
            }
            name.push('…');
        }
        cr.move_to(text_x, CLIP_TOP + 24.0);
        let _ = cr.show_text(&name);
        if width >= 81.0 {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
            cr.select_font_face(
                "monospace",
                cairo::FontSlant::Normal,
                cairo::FontWeight::Normal,
            );
            cr.set_font_size(10.0);
            let duration = if timed {
                timecode(seconds(clip.end - clip.start))
            } else {
                "Unavailable".into()
            };
            cr.move_to(text_x, CLIP_TOP + 43.0);
            let _ = cr.show_text(&duration);
        }
    }
    let _ = cr.restore();
}
