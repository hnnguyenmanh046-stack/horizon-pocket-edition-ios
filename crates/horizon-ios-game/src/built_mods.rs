//! The code mods this app was built with. iOS runs no code an app did not ship with, so where
//! the desktop's mod host opens the libraries beside its program and Starling's Android library
//! the ones in its private files (android_lib.rs `native_libraries`), a mod here is compiled
//! into the program: the build writes this list (docs/ios.md). An app built without mods has
//! none.

/// The mods' entries and typed listeners in the order they were built in, and their packages,
/// for `horizon_mod_host::load_code`.
pub fn built() -> (
    Vec<mod_platform::ModEntry>,
    Vec<horizon_mod_host::SlotListener>,
    horizon_mod_package::runtime::Runtime,
) {
    Default::default()
}
