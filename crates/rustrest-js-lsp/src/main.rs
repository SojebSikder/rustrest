//! `rustrest-js-lsp`: speaks LSP over stdio.

fn main() {
    let (connection, io_threads) = lsp_server::Connection::stdio();
    let clean = match rustrest_js_lsp::server::run(connection) {
        Ok(clean) => clean,
        Err(e) => {
            eprintln!("rustrest-js-lsp: {e}");
            false
        }
    };
    if let Err(e) = io_threads.join() {
        eprintln!("rustrest-js-lsp: io error: {e}");
    }
    std::process::exit(if clean { 0 } else { 1 });
}
