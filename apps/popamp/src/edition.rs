//! Which Popamp this is: the player alone, or Popamp Plus, built with
//! plug-ins. The two are packaged under these names and replace each
//! other; both install `popamp` and share its settings.

/// The package, and the first word of `popamp /VERSION`.
pub const PACKAGE: &str = if cfg!(feature = "plugins") { "popamp-plus" } else { "popamp" };
/// The name the user sees.
pub const NAME: &str = if cfg!(feature = "plugins") { "Popamp Plus" } else { "Popamp" };

/// What `/VERSION` prints; packaging reads the package name from it.
pub fn version_line() -> String {
    format!("{PACKAGE} {}", env!("CARGO_PKG_VERSION"))
}

/// The name and version, as the About box and the title display show it.
pub fn title() -> String {
    format!("{NAME} {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_with_plugins_is_popamp_plus() {
        let (package, version) = version_line().split_once(' ').map(|(p, v)| (p.to_string(), v.to_string())).unwrap();
        assert_eq!(package == "popamp-plus", cfg!(feature = "plugins"));
        assert_eq!(package == "popamp", !cfg!(feature = "plugins"));
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
        assert_eq!(title().starts_with("Popamp Plus "), cfg!(feature = "plugins"));
    }
}
