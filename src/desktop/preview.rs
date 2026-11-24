use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::Path,
    ptr::{self, NonNull},
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result, bail, ensure};
use libmpv2_sys as mpv;

#[derive(Debug)]
pub enum Event {
    Started(i64),
    Ready,
    Ended { id: i64, eof: bool },
    Failed { id: i64, message: String },
}

pub struct Preview {
    player: NonNull<mpv::mpv_handle>,
    render: Option<NonNull<mpv::mpv_render_context>>,
    redraw: Box<AtomicBool>,
    // Keep the provider loaded for the lifetime of the OpenGL render context.
    _egl: libloading::Library,
    lookup: Box<GetProcAddress>,
    #[cfg(test)]
    pub last_pixel: std::cell::Cell<[u8; 4]>,
}

type GetProcAddress = unsafe extern "C" fn(*const c_char) -> *mut c_void;

impl Preview {
    // The caller keeps this player on GTK's main thread and makes its GLArea
    // context current when creating, drawing, or dropping it.
    pub fn new() -> Result<Self> {
        // SAFETY: These are the system EGL library and libmpv instance constructor.
        let egl = unsafe { libloading::Library::new("libEGL.so.1") }.context("loading EGL")?;
        // SAFETY: The system EGL library exposes this function with the EGL ABI.
        let lookup = Box::new(unsafe { *egl.get::<GetProcAddress>(b"eglGetProcAddress\0")? });
        let player =
            NonNull::new(unsafe { mpv::mpv_create() }).context("creating libmpv player")?;
        let mut this = Self {
            player,
            render: None,
            redraw: Box::new(AtomicBool::new(false)),
            _egl: egl,
            lookup,
            #[cfg(test)]
            last_pixel: Default::default(),
        };
        for (name, value) in [
            (c"vo", c"libmpv"),
            (c"config", c"no"),
            (c"osc", c"no"),
            (c"load-scripts", c"no"),
            (c"sub-auto", c"no"),
            (c"audio-file-auto", c"no"),
            (c"rebase-start-time", c"no"),
            (c"hr-seek", c"yes"),
            (c"osd-level", c"0"),
            (c"pause", c"yes"),
        ] {
            // SAFETY: The player and both null-terminated strings are valid.
            check(unsafe {
                mpv::mpv_set_option_string(player.as_ptr(), name.as_ptr(), value.as_ptr())
            })?;
        }
        #[cfg(test)]
        // SAFETY: The test uses mpv's timed null sink in place of a sound card.
        check(unsafe {
            mpv::mpv_set_option_string(player.as_ptr(), c"ao".as_ptr(), c"null".as_ptr())
        })?;
        // SAFETY: The instance is initialized once, after setting its options.
        check(unsafe { mpv::mpv_initialize(player.as_ptr()) })?;
        this.init_render()?;
        Ok(this)
    }

    fn init_render(&mut self) -> Result<()> {
        gl::load_with(|name| {
            let name = CString::new(name).expect("GL function names contain no null bytes");
            // SAFETY: EGL receives a valid null-terminated OpenGL symbol name.
            unsafe { (self.lookup)(name.as_ptr()) }
        });
        ensure!(
            gl::GetIntegerv::is_loaded(),
            "OpenGL functions are unavailable"
        );
        let mut init = mpv::mpv_opengl_init_params {
            get_proc_address: Some(gl_proc),
            get_proc_address_ctx: ptr::from_mut(self.lookup.as_mut()).cast(),
        };
        let mut params = [
            mpv::mpv_render_param {
                type_: mpv::mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                data: c"opengl".as_ptr().cast_mut().cast(),
            },
            parameter(
                mpv::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                &mut init,
            ),
            parameter_end(),
        ];
        let mut context = ptr::null_mut();
        // SAFETY: The GL context is current, the lookup outlives the render
        // context, and the parameter array remains live throughout this call.
        check(unsafe {
            mpv::mpv_render_context_create(&mut context, self.player.as_ptr(), params.as_mut_ptr())
        })?;
        let render = NonNull::new(context).context("creating libmpv render context")?;
        self.render = Some(render);
        // SAFETY: The boxed flag outlives the callback, which is detached in Drop.
        unsafe {
            mpv::mpv_render_context_set_update_callback(
                render.as_ptr(),
                Some(redraw),
                ptr::from_mut(self.redraw.as_mut()).cast(),
            );
        }
        Ok(())
    }

    pub fn wants_frame(&self) -> bool {
        self.redraw.swap(false, Ordering::Relaxed)
    }

    pub fn draw(&self) -> Result<()> {
        let context = self.render.context("Preview renderer is unavailable")?;
        let mut framebuffer = 0;
        let mut read_framebuffer = 0;
        let mut viewport = [0i32; 4];
        // SAFETY: GTK has bound its framebuffer and made the GL context current.
        unsafe {
            gl::GetIntegerv(gl::DRAW_FRAMEBUFFER_BINDING, &mut framebuffer);
            gl::GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut read_framebuffer);
            gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());
        }
        if viewport[2] <= 0 || viewport[3] <= 0 {
            return Ok(());
        }
        let mut target = mpv::mpv_opengl_fbo {
            fbo: framebuffer,
            w: viewport[2],
            h: viewport[3],
            internal_format: 0,
        };
        let mut flip = 1i32;
        let mut block = 0i32;
        let mut params = [
            parameter(
                mpv::mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                &mut target,
            ),
            parameter(
                mpv::mpv_render_param_type_MPV_RENDER_PARAM_FLIP_Y,
                &mut flip,
            ),
            parameter(
                mpv::mpv_render_param_type_MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME,
                &mut block,
            ),
            parameter_end(),
        ];
        // SAFETY: The FBO belongs to the current GL context. Parameters are live
        // until the synchronous render call returns.
        check(unsafe {
            gl::Disable(gl::SCISSOR_TEST);
            gl::ColorMask(gl::TRUE, gl::TRUE, gl::TRUE, gl::TRUE);
            mpv::mpv_render_context_update(context.as_ptr());
            let code = mpv::mpv_render_context_render(context.as_ptr(), params.as_mut_ptr());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, framebuffer as u32);
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, read_framebuffer as u32);
            code
        })?;
        #[cfg(test)]
        {
            // Read back the frame GTK received so the smoke test checks pixels
            // from the real render callback.
            let mut pixel = [0u8; 4];
            let mut previous = 0;
            // SAFETY: The GLArea framebuffer is current and RGBA fits four bytes.
            unsafe {
                gl::GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut previous);
                gl::BindFramebuffer(gl::READ_FRAMEBUFFER, framebuffer as u32);
                gl::BindBuffer(gl::PIXEL_PACK_BUFFER, 0);
                gl::PixelStorei(gl::PACK_SKIP_PIXELS, 0);
                gl::PixelStorei(gl::PACK_SKIP_ROWS, 0);
                gl::ReadPixels(
                    viewport[2] / 2,
                    viewport[3] / 2,
                    1,
                    1,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    pixel.as_mut_ptr().cast(),
                );
                gl::BindFramebuffer(gl::READ_FRAMEBUFFER, previous as u32);
            }
            self.last_pixel.set(pixel);
        }
        Ok(())
    }

    pub fn load(&self, path: &Path, start: f64, end: f64, playing: bool) -> Result<i64> {
        self.pause(!playing)?;
        let options = format!("start={},end={}", mpv_time(start), mpv_time(end));
        self.command(&[
            b"loadfile",
            path.as_os_str().as_encoded_bytes(),
            b"replace",
            b"-1",
            options.as_bytes(),
        ])?;
        self.property(c"playlist/0/id", mpv::mpv_format_MPV_FORMAT_INT64)
    }

    pub fn seek(&self, seconds: f64, playing: bool) -> Result<()> {
        self.pause(!playing)?;
        let timestamp = format!("{seconds:.9}");
        self.command(&[b"seek", timestamp.as_bytes(), b"absolute+exact"])
    }

    pub fn stop(&self) {
        let _ = self.command(&[b"stop"]);
    }

    pub fn position(&self) -> Option<f64> {
        self.property(c"time-pos", mpv::mpv_format_MPV_FORMAT_DOUBLE)
            .ok()
    }

    fn pause(&self, paused: bool) -> Result<()> {
        let mut flag = i32::from(paused);
        // SAFETY: MPV_FORMAT_FLAG requires a pointer to a C int.
        check(unsafe {
            mpv::mpv_set_property(
                self.player.as_ptr(),
                c"pause".as_ptr(),
                mpv::mpv_format_MPV_FORMAT_FLAG,
                ptr::from_mut(&mut flag).cast(),
            )
        })
    }

    fn command(&self, values: &[&[u8]]) -> Result<()> {
        let values = values
            .iter()
            .map(|&value| CString::new(value))
            .collect::<Result<Vec<_>, _>>()?;
        let mut args: Vec<_> = values.iter().map(|value| value.as_ptr()).collect();
        args.push(ptr::null());
        // SAFETY: All arguments live until the synchronous command returns.
        check(unsafe { mpv::mpv_command(self.player.as_ptr(), args.as_mut_ptr()) })
    }

    fn property<T: Default>(&self, name: &CStr, format: mpv::mpv_format) -> Result<T> {
        let mut value = T::default();
        // SAFETY: Callers here use MPV_FORMAT_DOUBLE with f64 and INT64 with i64.
        check(unsafe {
            mpv::mpv_get_property(
                self.player.as_ptr(),
                name.as_ptr(),
                format,
                ptr::from_mut(&mut value).cast(),
            )
        })?;
        Ok(value)
    }

    pub fn events(&self) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            // SAFETY: libmpv always returns an event, valid until the next wait.
            // Data is read in this iteration only, based on the event's ID.
            let event = unsafe { &*mpv::mpv_wait_event(self.player.as_ptr(), 0.0) };
            match event.event_id {
                mpv::mpv_event_id_MPV_EVENT_NONE => break,
                mpv::mpv_event_id_MPV_EVENT_START_FILE => {
                    let data = unsafe { &*event.data.cast::<mpv::mpv_event_start_file>() };
                    events.push(Event::Started(data.playlist_entry_id));
                }
                mpv::mpv_event_id_MPV_EVENT_PLAYBACK_RESTART => events.push(Event::Ready),
                mpv::mpv_event_id_MPV_EVENT_END_FILE => {
                    let data = unsafe { &*event.data.cast::<mpv::mpv_event_end_file>() };
                    let id = data.playlist_entry_id;
                    if let Err(error) = check(data.error) {
                        events.push(Event::Failed {
                            id,
                            message: error.to_string(),
                        });
                    } else {
                        events.push(Event::Ended {
                            id,
                            eof: data.reason == mpv::mpv_end_file_reason_MPV_END_FILE_REASON_EOF,
                        });
                    }
                }
                _ => {}
            }
        }
        events
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        // SAFETY: The render context is still current and callbacks are detached
        // before freeing the state they reference. The instance is uniquely owned.
        unsafe {
            if let Some(context) = self.render {
                mpv::mpv_render_context_set_update_callback(
                    context.as_ptr(),
                    None,
                    ptr::null_mut(),
                );
                mpv::mpv_render_context_free(context.as_ptr());
            }
            mpv::mpv_terminate_destroy(self.player.as_ptr());
        }
    }
}

unsafe extern "C" fn gl_proc(context: *mut c_void, name: *const c_char) -> *mut c_void {
    // SAFETY: Preview owns the lookup until after the render context is freed.
    unsafe { (*context.cast::<GetProcAddress>())(name) }
}

unsafe extern "C" fn redraw(context: *mut c_void) {
    // SAFETY: Preview owns a stable boxed atomic until the callback is detached.
    unsafe { &*context.cast::<AtomicBool>() }.store(true, Ordering::Relaxed);
}

fn parameter<T>(type_: mpv::mpv_render_param_type, value: &mut T) -> mpv::mpv_render_param {
    mpv::mpv_render_param {
        type_,
        data: ptr::from_mut(value).cast(),
    }
}

fn parameter_end() -> mpv::mpv_render_param {
    mpv::mpv_render_param {
        type_: 0,
        data: ptr::null_mut(),
    }
}

fn check(code: i32) -> Result<()> {
    if code < 0 {
        // SAFETY: libmpv returns a static error string for any status code.
        let reason = unsafe { CStr::from_ptr(mpv::mpv_error_string(code)) };
        bail!("libmpv: {}", reason.to_string_lossy());
    }
    Ok(())
}

fn mpv_time(seconds: f64) -> String {
    // mpv interprets a bare minus sign as time relative to the end of the file.
    if seconds < 0.0 {
        format!(" {seconds:.9}")
    } else {
        format!("{seconds:.9}")
    }
}
