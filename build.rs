fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        // If you have an icon.ico file in the root directory, uncomment the line below:
        // res.set_icon("icon.ico");
        res.compile().unwrap();
    }
}
