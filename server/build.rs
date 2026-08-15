use std::env;
use std::path::Path;

fn main() {
    let module = Path::new("vendor/pawnio/LpcACPIEC.bin");
    if !module.exists() {
        panic!(
            "Missing official LpcACPIEC.bin. See server/vendor/pawnio/README.md \
             and download PawnIO.Modules 0.2.10."
        );
    }
    let metadata = std::fs::metadata(module).expect("stat LpcACPIEC.bin");
    if metadata.len() != 2612 {
        panic!(
            "LpcACPIEC.bin size {} does not match official 0.2.10 blob (2612 bytes)",
            metadata.len()
        );
    }
    println!("cargo:rerun-if-changed=vendor/pawnio/LpcACPIEC.bin");
    println!("cargo:rerun-if-changed=assets/ec-su_axb35-win.ico");
    println!("cargo:rerun-if-changed=assets/app.manifest");

    #[cfg(windows)]
    {
        let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/ec-su_axb35-win.ico");
        res.set("ProductName", "ec-su_axb35-win");
        res.set("FileDescription", "SU_AXB35 EC control");
        res.set("CompanyName", "Nardo021");
        res.set("LegalCopyright", "Copyright (c) deseven, Nardo021");
        res.set("FileVersion", &version);
        res.set("ProductVersion", &version);
        res.compile().expect("embed Windows resources");

        let manifest =
            Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets/app.manifest");
        // Embed UAC + DPI for the app/CLI only. Test binaries stay unelevated.
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
        println!("cargo:rustc-link-arg=/DEBUG:NONE");
    }
}
