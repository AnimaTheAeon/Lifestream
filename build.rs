use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=cpp/lt_bridge.cpp");
    println!("cargo:rerun-if-changed=cpp/lt_bridge.h");
    println!("cargo:rerun-if-changed=src/engine/bridge.rs");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=LIFESTREAM_VCPKG_TRIPLET");

    println!("cargo:rerun-if-env-changed=LIFESTREAM_CHECK_ONLY");
    if std::env::var_os("LIFESTREAM_CHECK_ONLY").is_some() {
        // `cargo check` for another OS: skip the C++ side entirely.
        return;
    }
    let mut build = cxx_build::bridge("src/engine/bridge.rs");
    build.std("c++17");
    build.file("cpp/lt_bridge.cpp");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        windows_libtorrent(&mut build);
    } else {
        let lib = pkg_config::Config::new()
            .probe("libtorrent-rasterbar")
            .expect("libtorrent-rasterbar not found (install libtorrent-rasterbar-dev)");
        for inc in &lib.include_paths {
            build.include(inc);
        }
        for (k, v) in &lib.defines {
            build.define(k, v.as_deref());
        }
        build.flag_if_supported("-fexceptions");
        build.flag_if_supported("-Wno-deprecated-declarations");
    }
    build.compile("lifestream_lt");

    if target_os == "windows" {
        println!("cargo:rerun-if-changed=resources/lifestream.rc");
        embed_resource::compile("resources/lifestream.rc", embed_resource::NONE)
            .manifest_optional()
            .ok();
    }
}

/// Link against libtorrent built by vcpkg (static, /MD CRT).
fn windows_libtorrent(build: &mut cc::Build) {
    let root = PathBuf::from(
        std::env::var("VCPKG_ROOT").expect("Set VCPKG_ROOT to a vcpkg install that has libtorrent"),
    );
    let triplet =
        std::env::var("LIFESTREAM_VCPKG_TRIPLET").unwrap_or_else(|_| "x64-windows-static-md".into());
    let installed = root.join("installed").join(&triplet);
    build.include(installed.join("include"));
    build.flag("/EHsc");
    build.flag("/bigobj");
    build.flag("/permissive-");

    for def in vcpkg_defines(&installed) {
        match def.split_once('=') {
            Some((k, v)) => build.define(k, Some(v)),
            None => build.define(&def, None),
        };
    }

    let lib_dir = installed.join("lib");
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    if let Ok(entries) = std::fs::read_dir(&lib_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lib") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    println!("cargo:rustc-link-lib=static={stem}");
                }
            }
        }
    }
    for sys in [
        "ws2_32", "iphlpapi", "mswsock", "wsock32", "crypt32", "bcrypt", "advapi32", "user32",
        "dbghelp", "ole32", "shell32",
    ] {
        println!("cargo:rustc-link-lib={sys}");
    }
}

/// Compile definitions libtorrent was built with, read from its CMake export.
fn vcpkg_defines(installed: &Path) -> Vec<String> {
    let mut defs: Vec<String> = Vec::new();
    let share = installed.join("share");
    let mut files = Vec::new();
    for dir in ["LibtorrentRasterbar", "libtorrent", "libtorrent-rasterbar"] {
        if let Ok(entries) = std::fs::read_dir(share.join(dir)) {
            for e in entries.flatten() {
                let p = e.path();
                if p.to_string_lossy().ends_with("Targets.cmake") {
                    files.push(p);
                }
            }
        }
    }
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        for line in text.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("INTERFACE_COMPILE_DEFINITIONS") else { continue };
            let rest = rest.trim().trim_matches('"');
            for d in rest.split(';') {
                let d = d.trim();
                if d.is_empty() || d.contains('$') || d.contains('<') || d == "TORRENT_LINKING_SHARED" {
                    continue;
                }
                if !defs.iter().any(|x| x == d) {
                    defs.push(d.to_string());
                }
            }
        }
    }
    if defs.is_empty() {
        // Sensible fallback matching vcpkg's default libtorrent build.
        defs = [
            "BOOST_ASIO_ENABLE_CANCELIO",
            "BOOST_ASIO_NO_DEPRECATED",
            "TORRENT_USE_OPENSSL",
            "TORRENT_USE_LIBCRYPTO",
            "TORRENT_SSL_PEERS",
            "OPENSSL_NO_SSL2",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
    }
    for d in ["WIN32_LEAN_AND_MEAN", "_WIN32_WINNT=0x0A00", "NOMINMAX", "_SCL_SECURE_NO_DEPRECATE"] {
        let key = d.split('=').next().unwrap();
        if !defs.iter().any(|x| x.split('=').next() == Some(key)) {
            defs.push(d.to_string());
        }
    }
    defs
}
