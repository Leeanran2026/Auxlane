fn main() {
    let config = slint_build::CompilerConfiguration::new().with_style("material".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("failed to compile Slint UI");

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/auxlane.ico");
    resource.set("ProductName", "Auxlane");
    resource.set("FileDescription", "Auxlane Bluetooth audio receiver");
    resource.set("LegalCopyright", "GPL-3.0-only");
    resource.set("FileVersion", "0.1.0.0");
    resource.set("ProductVersion", "0.1.0");
    if let Err(error) = resource.compile() {
        println!("cargo:warning=could not embed Windows resources: {error}");
    }
}
