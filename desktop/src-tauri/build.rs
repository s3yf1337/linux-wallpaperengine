fn main() {
    tauri_build::build();

    // After tauri_build: ensure libsteam_api.so lands next to the dev binary
    // so the rpath $ORIGIN in .cargo/config.toml can find it.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let sdk_lib = std::path::Path::new(&manifest)
        .join("steamworks-sys")
        .join("lib")
        .join("steam")
        .join("redistributable_bin")
        .join("linux64")
        .join("libsteam_api.so");
    if sdk_lib.exists() {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        // OUT_DIR = .../target/<profile>/build/<pkg-hash>/out
        // ancestors: 0=out 1=<pkg> 2=build 3=<profile> 4=target
        let target_profile = std::path::Path::new(&out_dir)
            .ancestors()
            .nth(3)
            .expect("OUT_DIR too shallow for target/<profile>");
        let dest = target_profile.join("libsteam_api.so");
        if let Err(e) = std::fs::copy(&sdk_lib, &dest) {
            println!("cargo:warning=failed to stage libsteam_api.so: {e}");
        } else {
            println!("cargo:warning=staged libsteam_api.so -> {}", dest.display());
        }
    } else {
        println!(
            "cargo:warning=libsteam_api.so not found at {} \
             — copy Steamworks SDK linux64/libsteam_api.so there",
            sdk_lib.display()
        );
    }
}
