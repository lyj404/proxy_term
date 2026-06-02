fn main() {
    slint_build::compile("src/app.slint").unwrap();

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let icon_path = std::path::Path::new(&manifest_dir).join("assets/logo.ico");

    let icon_bytes = std::fs::read(&icon_path).expect("无法读取 assets/logo.ico");
    let img = image::load_from_memory(&icon_bytes)
        .expect("无法解码 assets/logo.ico")
        .into_rgba8();
    let (w, h) = img.dimensions();
    let pixels = img.into_raw();

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = std::path::Path::new(&out_dir).join("icon.rs");

    let mut buf = String::new();
    buf.push_str(&format!(
        "pub const ICON_WIDTH: u32 = {};\n",
        w
    ));
    buf.push_str(&format!(
        "pub const ICON_HEIGHT: u32 = {};\n",
        h
    ));
    buf.push_str("pub const ICON_RGBA: &[u8] = &[\n");
    for chunk in pixels.chunks(32) {
        buf.push_str("    ");
        for &p in chunk {
            buf.push_str(&format!("{},", p));
        }
        buf.push('\n');
    }
    buf.push_str("];\n");

    std::fs::write(&dest, buf).unwrap();

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/logo.ico");
        res.compile().expect("Failed to compile Windows resource");
    }
}
