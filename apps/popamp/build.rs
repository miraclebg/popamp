//! Gives popamp.exe its icon on Windows, for Explorer, shortcuts and the
//! taskbar.

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=../../assets/logo/popamp.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/logo/popamp.ico");
        res.compile().expect("compile the Windows resources");
    }
}
