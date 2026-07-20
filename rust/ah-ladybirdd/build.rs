use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

fn main() {
    emit_astrohacker_cli_version();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    println!("cargo:rerun-if-changed=../proto/termsurf.proto");

    let abi_build = manifest_dir.join("libtermsurf_ladybird/build");
    let render_channel_dir = manifest_dir.join("../render-channel");

    println!(
        "cargo:rerun-if-changed={}",
        render_channel_dir
            .join("termsurf_render_channel.c")
            .display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        render_channel_dir
            .join("termsurf_render_channel.h")
            .display()
    );

    cc::Build::new()
        .file(render_channel_dir.join("termsurf_render_channel.c"))
        .include(&render_channel_dir)
        .flag_if_supported("-std=c99")
        .compile("termsurf_render_channel");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=IOSurface");
    }

    println!("cargo:rustc-link-search=native={}", abi_build.display());
    println!("cargo:rustc-link-lib=dylib=termsurf_ladybird");
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path/.");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", abi_build.display());

    prost_build::Config::new()
        .compile_protos(&["../proto/termsurf.proto"], &["../proto/"])
        .unwrap();

    stage_pdfjs_resources(&manifest_dir);
}

fn emit_astrohacker_cli_version() {
    println!("cargo:rerun-if-env-changed=ASTROHACKER_VERSION");
    let version =
        env::var("ASTROHACKER_VERSION").unwrap_or_else(|_| env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=ASTROHACKER_CLI_VERSION={version}");
}

fn stage_pdfjs_resources(manifest_dir: &Path) {
    println!("cargo:rerun-if-changed=../../forks/ladybird/Base/res/ladybird/pdfjs");
    println!(
        "cargo:rerun-if-changed=../../forks/ladybird/Build/debug/vcpkg_installed/arm64-osx-dynamic/share/pdfjs"
    );
    println!(
        "cargo:rerun-if-changed=../../forks/ladybird/Build/release/vcpkg_installed/arm64-osx-dynamic/share/pdfjs"
    );

    let Some(target_profile_dir) = target_profile_dir() else {
        println!("cargo:warning=unable to resolve target profile directory for pdf.js staging");
        return;
    };
    let Some(target_root) = target_profile_dir.parent() else {
        println!("cargo:warning=unable to resolve target root for pdf.js staging");
        return;
    };

    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let vendor_dir = manifest_dir.join("../../forks/ladybird");
    let preferred_build = if profile == "release" {
        "release"
    } else {
        "debug"
    };
    let mut pdfjs_source = vendor_dir.join(format!(
        "Build/{preferred_build}/vcpkg_installed/arm64-osx-dynamic/share/pdfjs"
    ));
    if !pdfjs_source.join("web/viewer.html").is_file() {
        let fallback = vendor_dir.join("Build/debug/vcpkg_installed/arm64-osx-dynamic/share/pdfjs");
        if fallback.join("web/viewer.html").is_file() {
            pdfjs_source = fallback;
        }
    }
    if !pdfjs_source.join("web/viewer.html").is_file() {
        println!(
            "cargo:warning=pdf.js vcpkg assets not staged; missing {}",
            pdfjs_source.join("web/viewer.html").display()
        );
        return;
    }

    let transport_source = vendor_dir.join("Base/res/ladybird/pdfjs/pdfjs-ladybird-transport.mjs");
    let mut destinations = vec![target_root.join("Resources/ladybird/pdfjs")];
    let ladybird_app_resources = vendor_dir.join(format!(
        "Build/{preferred_build}/bin/Ladybird.app/Contents/Resources/ladybird/pdfjs"
    ));
    if ladybird_app_resources.parent().is_some_and(Path::exists) {
        destinations.push(ladybird_app_resources);
    }

    for pdfjs_dest in destinations {
        if let Err(error) = copy_tree(&pdfjs_source.join("build"), &pdfjs_dest.join("build")) {
            println!(
                "cargo:warning=failed to stage pdf.js build assets to {}: {error}",
                pdfjs_dest.display()
            );
            continue;
        }
        if let Err(error) = copy_tree(&pdfjs_source.join("web"), &pdfjs_dest.join("web")) {
            println!(
                "cargo:warning=failed to stage pdf.js web assets to {}: {error}",
                pdfjs_dest.display()
            );
            continue;
        }

        let transport_dest = pdfjs_dest.join("web/pdfjs-ladybird-transport.mjs");
        if let Err(error) = fs::copy(&transport_source, &transport_dest) {
            println!(
                "cargo:warning=failed to stage Ladybird pdf.js transport {} -> {}: {error}",
                transport_source.display(),
                transport_dest.display()
            );
        }
    }
}

fn target_profile_dir() -> Option<PathBuf> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").ok()?);
    out_dir.ancestors().nth(3).map(Path::to_path_buf)
}

fn copy_tree(source: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let child_dest = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &child_dest)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), child_dest)?;
        }
    }
    Ok(())
}
