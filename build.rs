fn main() {
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/windows.rc");
        println!("cargo:rerun-if-changed=assets/asciiflow.ico");
        embed_resource::compile("assets/windows.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("could not embed the ASCIIFlow icon");
    }
}
