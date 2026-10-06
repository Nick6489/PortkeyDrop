//! Embed the Windows application manifest.
//!
//! This is load-bearing, not cosmetic: wxWidgets imports `GetWindowSubclass`,
//! which only Common Controls 6 exports. Without the manifest Windows resolves
//! comctl32 to the 5.82 copy in System32 and the process dies with
//! STATUS_ENTRYPOINT_NOT_FOUND before `main` runs.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=portkeydrop.manifest");
    link_static_prism_dependencies();

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

/// CMake's private dependencies do not propagate through a static archive.
/// Supply the platform libraries used by Prismer's bundled Prism here.
fn link_static_prism_dependencies() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("macos") => {
            for framework in [
                "Foundation",
                "AVFoundation",
                "AppKit",
                "IOKit",
                "CoreFoundation",
            ] {
                println!("cargo:rustc-link-lib=framework={framework}");
            }
        }
        Ok("linux") => {
            // Match Prism's optional backend detection. Speech-dispatcher
            // loads at runtime, while Orca and Spiel link GLib libraries.
            for package in ["glibmm-2.68", "giomm-2.68"] {
                let _ = pkg_config::Config::new()
                    .atleast_version("2.68.0")
                    .probe(package);
            }
            let _ = pkg_config::Config::new().probe("gio-2.0");
        }
        _ => {}
    }
}
