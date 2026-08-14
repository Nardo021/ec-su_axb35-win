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

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/ec-su_axb35-win.ico");
        if std::env::var("PROFILE").unwrap_or_default() == "release" {
            res.set_manifest(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#,
            );
        }
        res.compile().expect("embed Windows resources");
        println!("cargo:rustc-link-arg=/DEBUG:NONE");
    }
}
