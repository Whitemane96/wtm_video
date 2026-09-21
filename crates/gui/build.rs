//! Embeds assets/icon.ico into the Windows executable so Explorer and the
//! taskbar show it. Does nothing on other platforms.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        res.compile().expect("failed to embed the Windows icon");
    }
}
