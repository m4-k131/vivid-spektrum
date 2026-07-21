#[cfg(target_os = "windows")]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_icon("assets/icon.ico");
    res.set("FileDescription", "vividspektrum");
    res.set("ProductName", "vividspektrum");
    res.set("OriginalFilename", "vividspektrum.exe");
    if let Err(e) = res.compile() {
        eprintln!("warning: failed to embed Windows icon: {e}");
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {}
