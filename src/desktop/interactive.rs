use super::*;

impl App {
    pub(super) fn connect_timeline(self: &Rc<Self>) {
        let area = &self.ui.timeline.area;
        let drag = gtk::GestureDrag::builder().button(1).build();
        let weak = Rc::downgrade(self);
        drag.connect_drag_begin(move |_, x, y| {
            if let Some(app) = weak.upgrade() {
                app.begin_timeline_drag(x, y);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_update(move |_, dx, _| {
            if let Some(app) = weak.upgrade() {
                app.update_timeline_drag(dx);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_drag_end(move |_, dx, _| {
            if let Some(app) = weak.upgrade() {
                app.finish_timeline_drag(dx);
            }
        });
        let weak = Rc::downgrade(self);
        drag.connect_cancel(move |_, _| {
            if let Some(app) = weak.upgrade() {
                app.cancel_timeline_drag();
                app.controls();
            }
        });
        area.add_controller(drag);

        let source_drop = gtk::DropTarget::new(String::static_type(), gdk::DragAction::COPY);
        let weak = Rc::downgrade(self);
        source_drop.connect_motion(move |_, x, _| {
            let Some(app) = weak.upgrade() else {
                return gdk::DragAction::empty();
            };
            if app.state.borrow().job.is_some() {
                return gdk::DragAction::empty();
            }
            app.ui.timeline.draft(Some(Draft::Move {
                slot: app.ui.timeline.slot_at(x),
                clip: None,
            }));
            gdk::DragAction::COPY
        });
        let weak = Rc::downgrade(self);
        source_drop.connect_leave(move |_| {
            if let Some(app) = weak.upgrade() {
                app.ui.timeline.draft(None);
            }
        });
        let weak = Rc::downgrade(self);
        source_drop.connect_drop(move |_, value, x, _| {
            let Some(app) = weak.upgrade() else {
                return false;
            };
            app.ui.timeline.draft(None);
            value
                .get::<String>()
                .ok()
                .is_some_and(|id| app.insert_source(&id, app.ui.timeline.slot_at(x)))
        });
        area.add_controller(source_drop);

        let motion = gtk::EventControllerMotion::new();
        let weak = Rc::downgrade(self);
        motion.connect_motion(move |_, x, y| {
            if let Some(app) = weak.upgrade() {
                let cursor = match app.ui.timeline.hit(x, y) {
                    Hit::Ruler => "crosshair",
                    Hit::Clip(_, Some(_)) => "ew-resize",
                    Hit::Clip(_, None) => "grab",
                    Hit::Empty => "default",
                };
                app.ui.timeline.area.set_cursor_from_name(Some(cursor));
            }
        });
        area.add_controller(motion);

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        let weak = Rc::downgrade(self);
        scroll.connect_scroll(move |controller, dx, dy| {
            if let Some(app) = weak.upgrade()
                && app.timeline_drag.borrow().is_none()
            {
                let amount = if dy.abs() >= dx.abs() { dy } else { dx };
                let wheel = controller.unit() == gdk::ScrollUnit::Wheel;
                if controller
                    .current_event_state()
                    .contains(gdk::ModifierType::CONTROL_MASK)
                {
                    let anchor = controller
                        .current_event()
                        .and_then(|event| event.position())
                        .and_then(|(x, y)| {
                            app.ui.window.compute_point(
                                &app.ui.timeline.area,
                                &gtk::graphene::Point::new(x as f32, y as f32),
                            )
                        })
                        .map(|point| f64::from(point.x()));
                    let steps = if wheel { amount } else { amount / 45.0 };
                    app.ui.timeline.zoom(1.35f64.powf(-steps), anchor);
                } else {
                    app.ui
                        .timeline
                        .pan(if wheel { amount * 65.0 } else { amount });
                }
            }
            glib::Propagation::Stop
        });
        area.add_controller(scroll);
        self.connect_shortcuts();
    }

    fn connect_shortcuts(self: &Rc<Self>) {
        let keys = gtk::EventControllerKey::new();
        keys.set_name(Some("timeline-shortcuts"));
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        keys.connect_key_pressed(move |_, key, _, modifiers| {
            let Some(app) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if GtkWindowExt::focus(&app.ui.window).is_some_and(|focus| focus.is::<gtk::Editable>())
                || modifiers.intersects(
                    gdk::ModifierType::ALT_MASK
                        | gdk::ModifierType::SUPER_MASK
                        | gdk::ModifierType::META_MASK
                        | gdk::ModifierType::HYPER_MASK,
                )
            {
                return glib::Propagation::Proceed;
            }
            let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
            let dragging = app.timeline_drag.borrow().is_some();
            let can_edit = app.state.borrow().job.is_none() && !dragging;
            if control {
                if can_edit {
                    match key.to_lower() {
                        gdk::Key::x if !shift => app.copy_clip(true),
                        gdk::Key::c if !shift => app.copy_clip(false),
                        gdk::Key::v if !shift => app.paste_clip(),
                        gdk::Key::z => app.edit_history(shift),
                        gdk::Key::Left | gdk::Key::Right if !shift => {
                            let selected = app.state.borrow().selected_clip;
                            if let Some(clip) = selected {
                                let slot = if key == gdk::Key::Left {
                                    clip.saturating_sub(1)
                                } else {
                                    clip + 2
                                };
                                app.reorder_clip(clip, slot);
                            }
                        }
                        _ => return glib::Propagation::Proceed,
                    }
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }
            match key {
                gdk::Key::Escape if dragging => {
                    app.cancel_timeline_drag();
                    app.controls();
                }
                gdk::Key::space if !dragging && !shift => app.toggle_play(),
                gdk::Key::Left | gdk::Key::Right if !dragging && !shift => {
                    app.seek_keyframe(key == gdk::Key::Right);
                }
                gdk::Key::s | gdk::Key::S if can_edit && !shift => app.split(),
                gdk::Key::Delete if can_edit && !shift => app.remove_clip(),
                gdk::Key::plus | gdk::Key::equal => app.ui.timeline.zoom(1.5, None),
                gdk::Key::minus => app.ui.timeline.zoom(1.0 / 1.5, None),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        self.ui.window.add_controller(keys);
    }

    fn seek_keyframe(&self, forward: bool) {
        let position = self.state.borrow().adjacent_keyframe(forward);
        if let Some(position) = position {
            self.request_preview(position, false);
            self.ui.timeline.playhead(position, true);
        }
    }

    pub(super) fn toggle_play(&self) {
        let (position, playing) = {
            let state = self.state.borrow();
            if state.segments.is_empty() || self.preview.borrow().is_none() {
                return;
            }
            let position = if state.position >= state.total() {
                0
            } else {
                state.position
            };
            (position, !state.playing)
        };
        self.request_preview(position, playing);
    }

    pub(super) fn begin_timeline_drag(&self, x: f64, y: f64) {
        let time = self.ui.timeline.time_at(x);
        let mode = match self.ui.timeline.hit(x, y) {
            Hit::Ruler => DragMode::Scrub { snap_click: true },
            Hit::Empty => DragMode::Scrub { snap_click: false },
            Hit::Clip(clip, edge) => {
                let mut state = self.state.borrow_mut();
                if state.job.is_some() {
                    return;
                }
                state.selected_clip = Some(clip);
                self.trim_drag(&state, clip, edge)
                    .unwrap_or(DragMode::Reorder { clip, slot: clip })
            }
        };
        let playing_at = {
            let state = self.state.borrow();
            state.playing.then_some(state.position)
        };
        if let Some(position) = playing_at {
            self.request_preview(position, false);
        }
        self.ui.timeline.area.grab_focus();
        *self.timeline_drag.borrow_mut() = Some(TimelineDrag {
            origin_x: x,
            origin_time: time,
            pointer_x: x,
            moved: false,
            mode,
        });
        self.controls();
    }

    fn trim_drag(&self, state: &State, clip: usize, edge: Option<Edge>) -> Option<DragMode> {
        let edge = edge?;
        let segment = state.segments.get(clip)?;
        let source = &state.editor.project.clips[clip].source;
        let base = state.editor.source(source).ok()?.1.info.video_time_base;
        let original_time = match edge {
            Edge::In => segment.source_start,
            Edge::Out => segment.source_end,
        };
        let cuts = state
            .editor
            .cut_points(clip, edge == Edge::In)
            .ok()?
            .into_iter()
            .map(|pts| (base.seconds(pts), pts))
            .collect();
        Some(DragMode::Trim {
            clip,
            edge,
            original_time,
            cuts,
            snapped_pts: None,
        })
    }

    pub(super) fn update_timeline_drag(&self, dx: f64) {
        let mut active = self.timeline_drag.borrow_mut();
        let Some(drag) = active.as_mut() else { return };
        drag.pointer_x = drag.origin_x + dx;
        drag.moved |= dx.abs() >= 3.0;
        let time = self.ui.timeline.time_at(drag.pointer_x);
        match &mut drag.mode {
            DragMode::Scrub { .. } if drag.moved => self.request_preview(time, false),
            DragMode::Reorder { clip, slot } if drag.moved => {
                *slot = self.ui.timeline.slot_at(drag.pointer_x);
                let moving = self.ui.timeline.clip_start(*clip).map(|start| {
                    (
                        *clip,
                        start.saturating_add(time.saturating_sub(drag.origin_time)),
                    )
                });
                self.ui.timeline.draft(Some(Draft::Move {
                    slot: *slot,
                    clip: moving,
                }));
            }
            DragMode::Trim {
                clip,
                edge,
                original_time,
                cuts,
                snapped_pts,
            } if drag.moved => {
                let desired = *original_time + seconds(time.saturating_sub(drag.origin_time));
                let Some((target, pts)) = nearest_cut(cuts, desired) else {
                    return;
                };
                *snapped_pts = Some(pts);
                let state = self.state.borrow();
                let Some(segment) = state.segments.get(*clip) else {
                    return;
                };
                let delta = ((target - *original_time) * 1_000_000.0).round() as i64;
                let (start, end, label) = match edge {
                    Edge::In => (segment.start_us.saturating_add(delta), segment.end_us, "In"),
                    Edge::Out => (
                        segment.start_us,
                        segment.end_us.saturating_add(delta),
                        "Out",
                    ),
                };
                self.ui.timeline.draft(Some(Draft::Trim {
                    clip: *clip,
                    start,
                    end,
                }));
                self.ui
                    .clip_details
                    .set_text(&format!("{label} {}", timecode(target)));
            }
            _ => {}
        }
    }

    pub(super) fn finish_timeline_drag(self: &Rc<Self>, dx: f64) {
        self.update_timeline_drag(dx);
        let drag = self.timeline_drag.borrow_mut().take();
        self.ui.timeline.draft(None);
        let Some(drag) = drag else { return };
        match drag.mode {
            DragMode::Scrub { snap_click } if !drag.moved => {
                let position = if snap_click {
                    self.state
                        .borrow()
                        .nearest_keyframe(drag.origin_time)
                        .unwrap_or(drag.origin_time)
                } else {
                    drag.origin_time
                };
                self.request_preview(position, false);
            }
            DragMode::Reorder { clip, slot } if drag.moved => self.reorder_clip(clip, slot),
            DragMode::Trim {
                clip,
                edge,
                snapped_pts: Some(pts),
                ..
            } if drag.moved => {
                let result =
                    self.state
                        .borrow_mut()
                        .editor
                        .set_boundary(clip, edge == Edge::In, pts);
                match result {
                    Ok(()) => {
                        self.refresh();
                        let position =
                            self.state
                                .borrow()
                                .segments
                                .get(clip)
                                .map(|segment| match edge {
                                    Edge::In => segment.start_us,
                                    Edge::Out => segment.end_us.saturating_sub(1),
                                });
                        if let Some(position) = position {
                            self.request_preview(position, false);
                        }
                    }
                    Err(error) => self
                        .ui
                        .message(&format!("Could not trim at that cut. {error:#}")),
                }
            }
            DragMode::Reorder { .. } | DragMode::Trim { .. } if !drag.moved => {
                self.request_preview(self.ui.timeline.time_at(drag.pointer_x), false);
            }
            _ => {}
        }
        self.controls();
    }

    pub(super) fn cancel_timeline_drag(&self) {
        self.timeline_drag.borrow_mut().take();
        self.ui.timeline.draft(None);
    }

    pub(super) fn pan_during_drag(&self) {
        let pointer = self
            .timeline_drag
            .borrow()
            .as_ref()
            .filter(|drag| drag.moved)
            .map(|drag| (drag.pointer_x, drag.origin_x));
        if let Some((x, origin)) = pointer {
            let limit = f64::from(self.ui.timeline.area.width()) - 24.0;
            let speed = if x > limit {
                ((x - limit) / 2.0).min(36.0)
            } else if x < 24.0 {
                -((24.0 - x) / 2.0).min(36.0)
            } else {
                0.0
            };
            if speed != 0.0 && self.ui.timeline.pan(speed) {
                self.update_timeline_drag(x - origin);
            }
        }
    }

    fn reorder_clip(self: &Rc<Self>, clip: usize, slot: usize) {
        let changed = {
            let mut state = self.state.borrow_mut();
            let count = state.editor.project.clips.len();
            if clip >= count {
                return;
            }
            let slot = slot.min(count);
            let target = slot - usize::from(slot > clip);
            if target == clip {
                return;
            }
            state.editor.change(|project| {
                let item = project.clips.remove(clip);
                project.clips.insert(target, item);
            });
            state.selected_clip = Some(target);
            target
        };
        self.refresh();
        self.select_clip(changed);
    }
}

impl State {
    fn adjacent_keyframe(&self, forward: bool) -> Option<i64> {
        let index = self.at(self.position)?;
        if forward {
            self.segments[index..].iter().find_map(|segment| {
                let points = &segment.keyframes;
                let next = points.partition_point(|&time| time <= self.position);
                points.get(next).copied()
            })
        } else {
            self.segments[..=index].iter().rev().find_map(|segment| {
                let points = &segment.keyframes;
                let before = points.partition_point(|&time| time < self.position);
                before.checked_sub(1).map(|i| points[i])
            })
        }
    }

    fn nearest_keyframe(&self, position: i64) -> Option<i64> {
        let index = self.at(position)?;
        let segment = &self.segments[index];
        let clip = &self.editor.project.clips[index];
        let (_, media) = self.editor.source(&clip.source).ok()?;
        let base = media.info.video_time_base;
        let points = &media.info.safe_in_pts;
        let to_timeline = |pts| {
            let offset = ((base.seconds(pts) - segment.source_start) * 1_000_000.0).round();
            segment.start_us.saturating_add(offset as i64)
        };
        let insertion = points.partition_point(|&pts| to_timeline(pts) <= position);
        let before = insertion.checked_sub(1).and_then(|index| points.get(index));
        let after = points.get(insertion);
        before
            .into_iter()
            .chain(after)
            .copied()
            .filter(|&pts| (clip.in_pts..clip.out_pts).contains(&pts))
            .map(to_timeline)
            .chain(self.segments.get(index + 1).map(|next| next.start_us))
            .min_by_key(|&time| time.abs_diff(position))
    }
}

fn nearest_cut(points: &[(f64, i64)], target: f64) -> Option<(f64, i64)> {
    let index = points.partition_point(|&(seconds, _)| seconds <= target);
    let before = index.checked_sub(1).and_then(|index| points.get(index));
    let after = points.get(index);
    before
        .into_iter()
        .chain(after)
        .min_by(|a, b| (a.0 - target).abs().total_cmp(&(b.0 - target).abs()))
        .copied()
}
