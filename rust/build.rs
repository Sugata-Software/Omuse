// Windows: embed the application icon and version information in omuse.exe.
// Other platforms have nothing to build.
fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/windows/omuse.rc");
        println!("cargo:rerun-if-changed=assets/windows/omuse.ico");
        let version = std::env::var("CARGO_PKG_VERSION").unwrap();
        let quoted = format!("\"{version}\"");
        let [major, minor, patch] = ["MAJOR", "MINOR", "PATCH"]
            .map(|part| std::env::var(format!("CARGO_PKG_VERSION_{part}")).unwrap());
        embed_resource::compile(
            "assets/windows/omuse.rc",
            [
                format!("OMUSE_VERSION={quoted}"),
                format!("OMUSE_VERSION_MAJOR={major}"),
                format!("OMUSE_VERSION_MINOR={minor}"),
                format!("OMUSE_VERSION_PATCH={patch}"),
            ],
        )
        .manifest_optional()
        .unwrap();
    }
}
