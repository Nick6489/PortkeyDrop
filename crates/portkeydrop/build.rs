//! Embed the Windows application manifest.
//!
//! This is load-bearing, not cosmetic: wxWidgets imports `GetWindowSubclass`,
//! which only Common Controls 6 exports. Without the manifest Windows resolves
//! comctl32 to the 5.82 copy in System32 and the process dies with
//! STATUS_ENTRYPOINT_NOT_FOUND before `main` runs.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=portkeydrop.manifest");
    delay_load_screen_reader_dlls();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Only the MSVC linker understands these flags; the GNU toolchain embeds
    // manifests through a resource file instead.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }

    let manifest =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("portkeydrop.manifest");
    if !manifest.is_file() {
        println!("cargo:warning=portkeydrop.manifest is missing; the app will not start");
        return;
    }

    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
        manifest.display()
    );
    // Without this the linker also merges its own default manifest, which can
    // conflict with the one supplied above.
    println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:level='asInvoker' uiAccess='false'");
}

/// Delay-load the screen-reader DLLs a static Prism imports.
///
/// Prismer publishes the list because Cargo will not carry link arguments up
/// from the library crate. Without the delay load, a machine that does not
/// have every one of those DLLs installed will not start.
fn delay_load_screen_reader_dlls() {
    let Ok(dlls) = std::env::var("DEP_PRISMER_DELAY_LOAD_DLLS") else {
        return;
    };
    println!("cargo:rerun-if-env-changed=DEP_PRISMER_DELAY_LOAD_DLLS");
    println!("cargo:rustc-link-lib=delayimp");
    // Prism's list includes bridge DLLs this platform does not import.
    // The linker ignores those and otherwise warns, which fails a build
    // that denies warnings.
    println!("cargo:rustc-link-arg=/IGNORE:4199");
    for dll in dlls.split(';').filter(|dll| !dll.is_empty()) {
        println!("cargo:rustc-link-arg=/DELAYLOAD:{dll}");
    }
}
