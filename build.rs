fn main() {
    println!("cargo:rerun-if-changed=assets/icons/codex-weather-widget.ico");
    println!("cargo:rerun-if-changed=assets/icons/codex-weather-widget-32.png");

    #[cfg(target_os = "windows")]
    {
        let mut resource = winres::WindowsResource::new();
        resource
            .set_icon("assets/icons/codex-weather-widget.ico")
            .set("ProductName", "Codex Weather Widget")
            .set("FileDescription", "Codex quota and weather desktop widget")
            .set("LegalCopyright", "Copyright (c) adoin");
        resource
            .compile()
            .expect("compile Windows application resources");
    }
}
