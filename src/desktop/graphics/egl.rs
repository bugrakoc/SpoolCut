use std::ffi::{CStr, c_char, c_void};

use anyhow::{Context, Result};

type GetProcAddress = unsafe extern "C" fn(*const c_char) -> *mut c_void;

pub struct Resolver {
    _library: libloading::Library,
    get_proc: GetProcAddress,
}

impl Resolver {
    pub fn new() -> Result<Self> {
        // SAFETY: Load the system EGL library and use the EGL symbol's ABI.
        let library = unsafe { libloading::Library::new("libEGL.so.1") }.context("loading EGL")?;
        let get_proc = unsafe { *library.get::<GetProcAddress>(b"eglGetProcAddress\0")? };
        Ok(Self {
            _library: library,
            get_proc,
        })
    }

    pub fn lookup(&self, name: &CStr) -> *mut c_void {
        // SAFETY: EGL receives a valid null-terminated OpenGL symbol name.
        unsafe { (self.get_proc)(name.as_ptr()) }
    }
}
