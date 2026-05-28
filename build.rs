fn main() {
    slint_build::compile("src/app.slint").unwrap();

    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/logo.ico");
        res.compile().expect("Failed to compile Windows resource");
    }
}
