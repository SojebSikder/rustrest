//! Id generation for exported Insomnia resources.
//!
//! Plugins run in a sandboxed wasm module with no OS RNG or clock host
//! function available (see `rustrest-plugin-host::state::link_host_functions`
//! - only `host_log` is linked), so ids can't be truly random or time-based.
//!
//! A monotonically increasing in-process counter is good enough: Insomnia
//! only needs ids to be unique *within one export file*, not globally.

use std::cell::Cell;

thread_local! {
    static COUNTER: Cell<u64> = const { Cell::new(1) };
}

/// Generates an Insomnia-shaped resource id, e.g. `req_0000...0001`.
pub fn next_id(prefix: &str) -> String {
    let n = COUNTER.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    });
    format!("{prefix}_{n:032x}")
}
