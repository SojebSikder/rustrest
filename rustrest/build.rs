#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("../assets/images/app-icon.ico");

    // executable file metadata details in Windows Properties
    res.set("ProductName", "Rustrest");
    res.set("FileDescription", "API Testing Platform");
    res.set("LegalCopyright", "Copyright © 2026");
    res.set("CompanyName", "sojebsikder");

    if let Err(e) = res.compile() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    // Non-windows targets ignore this step
}
