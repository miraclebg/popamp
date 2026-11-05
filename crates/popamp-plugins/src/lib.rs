//! The plug-in host: finds plug-ins, checks their manifests and runs them
//! as Extism WebAssembly modules. See `docs/plugins.md`.

pub mod host;
pub mod installed;
pub mod manifest;
