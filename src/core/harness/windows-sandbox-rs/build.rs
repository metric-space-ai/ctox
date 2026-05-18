fn main() {
    let mut res = winres::WindowsResource::new();
    res.set_manifest_file("ctox-windows-sandbox-setup.manifest");
    let _ = res.compile();
}
