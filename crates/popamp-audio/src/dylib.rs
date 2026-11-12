//! Loads the libraries Popamp opens at run time (libopus, libopenmpt) by
//! name. On macOS they are looked for in the app bundle, then Homebrew,
//! which the system's loader does not search.

#[cfg(any(target_os = "macos", test))]
use std::path::{Path, PathBuf};

/// Loads the library `name`, or says where it was looked for.
#[cfg(not(target_os = "macos"))]
pub(crate) fn load(name: &str) -> Result<libloading::Library, String> {
    // SAFETY: loading runs the library's initialisers; libopus and
    // libopenmpt have none that touch this program.
    unsafe { libloading::Library::new(name) }.map_err(|e| format!("cannot load {name}: {e}"))
}

/// Loads the library `name` from the first of [`macos_candidates`] that
/// has it, or says every place that was tried.
#[cfg(target_os = "macos")]
pub(crate) fn load(name: &str) -> Result<libloading::Library, String> {
    let exe = std::env::current_exe().ok();
    let places = macos_candidates(name, exe.as_deref());
    let mut last = String::new();
    for place in &places {
        // SAFETY: as above.
        match unsafe { libloading::Library::new(place) } {
            Ok(lib) => return Ok(lib),
            Err(e) => last = e.to_string(),
        }
    }
    let tried: Vec<String> = places.iter().map(|p| p.display().to_string()).collect();
    Err(format!("cannot load {name} (tried {}): {last}", tried.join(", ")))
}

/// The places to load the library `name` from on macOS, best first: the
/// `Frameworks` folder of the app bundle `exe` is in, then Homebrew's
/// folders (Apple Silicon, then Intel). Only whole paths: given a bare
/// name, the system's loader also looks in the current folder, where
/// anyone could have left a library of that name.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn macos_candidates(name: &str, exe: Option<&Path>) -> Vec<PathBuf> {
    let bundle = exe.and_then(Path::parent).and_then(Path::parent).map(|contents| contents.join("Frameworks").join(name));
    let homebrew = ["/opt/homebrew/lib", "/usr/local/lib"].map(|dir| Path::new(dir).join(name));
    bundle.into_iter().chain(homebrew).filter(|p| p.is_absolute()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_looks_in_the_bundle_then_homebrew() {
        let exe = Path::new("/Applications/Popamp.app/Contents/MacOS/popamp");
        assert_eq!(
            macos_candidates("libopus.0.dylib", Some(exe)),
            [
                PathBuf::from("/Applications/Popamp.app/Contents/Frameworks/libopus.0.dylib"),
                PathBuf::from("/opt/homebrew/lib/libopus.0.dylib"),
                PathBuf::from("/usr/local/lib/libopus.0.dylib"),
            ]
        );
    }

    #[test]
    fn without_the_program_path_there_is_no_bundle_to_look_in() {
        assert_eq!(
            macos_candidates("x.dylib", None),
            [PathBuf::from("/opt/homebrew/lib/x.dylib"), PathBuf::from("/usr/local/lib/x.dylib")]
        );
    }

    // A bare name makes the system's loader look in the current folder too,
    // where anyone could have left a library of that name.
    #[test]
    fn macos_never_loads_by_bare_name() {
        assert!(macos_candidates("x.dylib", Some(Path::new("popamp"))).iter().all(|p| p.is_absolute()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_missing_library_names_every_place_tried() {
        let e = load("libpopamp-no-such.0.dylib").unwrap_err();
        let exe = std::env::current_exe().ok();
        for place in macos_candidates("libpopamp-no-such.0.dylib", exe.as_deref()) {
            assert!(e.contains(&*place.to_string_lossy()), "{e}");
        }
    }
}
