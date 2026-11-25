#[cfg(target_os = "linux")]
#[path = "graphics/egl.rs"]
mod platform;
#[cfg(windows)]
#[path = "graphics/wgl.rs"]
mod platform;

#[cfg(any(target_os = "linux", windows))]
pub use platform::Resolver;

#[cfg(not(any(target_os = "linux", windows)))]
pub struct Resolver;

#[cfg(not(any(target_os = "linux", windows)))]
impl Resolver {
    pub fn new() -> anyhow::Result<Self> {
        anyhow::bail!("OpenGL preview is unavailable on this platform")
    }

    pub fn lookup(&self, _: &std::ffi::CStr) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
}

pub fn prefer_opengl() -> bool {
    match std::env::var("SPOOLCUT_PREVIEW").as_deref() {
        Ok("software") => false,
        Ok("opengl") => true,
        _ => cfg!(target_os = "linux"),
    }
}
