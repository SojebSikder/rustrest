//! Safe wrappers over the small set of host functions a plugin can call
//! into (linked by `rustrest-plugin-host::state::link_host_functions`).

unsafe extern "C" {
    fn host_log(ptr: u32, len: u32);
}

/// logs a line, attributed to this plugin, into the app's console panel.
pub fn log(message: &str) {
    let bytes = message.as_bytes();
    unsafe {
        host_log(bytes.as_ptr() as u32, bytes.len() as u32);
    }
}
