//! Sets `linux_like` for Linux and the other Unix systems with ALSA, X11
//! and the freedesktop services: every Unix but macOS, which is a Unix too
//! but has none of them.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(linux_like)");
    let unix = std::env::var("CARGO_CFG_TARGET_FAMILY").is_ok_and(|f| f.split(',').any(|f| f == "unix"));
    if unix && std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        println!("cargo::rustc-cfg=linux_like");
    }
}
