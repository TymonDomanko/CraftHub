// Embeds the icon and version details into the Windows .exe.

fn main() {
    println!("cargo:rerun-if-changed=assets/icons/crafthub.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/crafthub.ico")
            .set("ProductName", "Craft Hub")
            .set("FileDescription", "Craft Hub")
            .set("CompanyName", "Tymon Domanko")
            .set("LegalCopyright", "MIT License");
        if let Err(e) = res.compile() {
            println!("cargo:warning=couldn't embed the Windows icon: {e}");
        }
    }
}
