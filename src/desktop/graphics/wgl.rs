use std::ffi::{CStr, c_char, c_void};

use anyhow::{Context, Result, ensure};
use libloading::os::windows::Library;

type GetProcAddress = unsafe extern "system" fn(*const c_char) -> *mut c_void;
type GetCurrentContext = unsafe extern "system" fn() -> *mut c_void;

pub struct Resolver {
    library: Library,
    get_proc: GetProcAddress,
}

impl Resolver {
    pub fn new() -> Result<Self> {
        use windows_sys::Win32::System::LibraryLoader::LOAD_LIBRARY_SEARCH_SYSTEM32;

        // SAFETY: Use the system OpenGL DLL and the published WGL signatures.
        let library =
            unsafe { Library::load_with_flags("opengl32.dll", LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .context("loading Windows OpenGL")?;
        let current = unsafe { *library.get::<GetCurrentContext>(b"wglGetCurrentContext\0")? };
        let get_proc = unsafe { *library.get::<GetProcAddress>(b"wglGetProcAddress\0")? };
        // SAFETY: This only reads the current thread's WGL context.
        ensure!(
            !unsafe { current() }.is_null(),
            "GTK did not create a WGL context"
        );
        Ok(Self { library, get_proc })
    }

    pub fn lookup(&self, name: &CStr) -> *mut c_void {
        // SAFETY: mpv and GTK query symbols with their GL context current.
        let address = unsafe { (self.get_proc)(name.as_ptr()) };
        // Some drivers return a sentinel for unavailable symbols. OpenGL 1.1
        // functions such as glGetIntegerv are exported directly by opengl32.
        if !matches!(address as usize, 0..=3 | usize::MAX) {
            return address;
        }
        // SAFETY: The caller casts the symbol to its OpenGL signature.
        unsafe { self.library.get::<*mut c_void>(name.to_bytes_with_nul()) }
            .map_or(std::ptr::null_mut(), |symbol| *symbol)
    }
}
