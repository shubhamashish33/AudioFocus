use std::path::Path;
use image::imageops::FilterType;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let icon_path = Path::new("assets/app_icon.ico");
        
        // Only convert if the png exists and ico doesn't
        if Path::new("assets/app_icon.png").exists() && !icon_path.exists() {
            generate_ico("assets/app_icon.png", "assets/app_icon.ico");
        }

        let mut res = winres::WindowsResource::new();
        if icon_path.exists() {
            res.set_icon("assets/app_icon.ico");
        }
        res.compile().unwrap();
    }
}

fn generate_ico(input: &str, output: &str) {
    let img = image::open(input).expect("Failed to open input PNG");
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);

    // Standard Windows icon sizes
    let sizes = [16, 32, 48, 64, 256];

    for &size in &sizes {
        let resized = img.resize_exact(size, size, FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let icon_image = ico::IconImage::from_rgba_data(size, size, rgba.into_raw());
        
        // Correctly encode the icon image into an entry
        let entry = ico::IconDirEntry::encode(&icon_image).expect("Failed to encode icon entry");
        icon_dir.add_entry(entry);
    }

    let file = std::fs::File::create(output).expect("Failed to create .ico file");
    icon_dir.write(file).expect("Failed to write .ico file");
}
