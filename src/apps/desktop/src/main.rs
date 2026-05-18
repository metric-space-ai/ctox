use anyhow::Result;
use ctox_desktop::app::CtoxDesktopApp;
use eframe::egui::IconData;
use image::ImageReader;

fn main() -> Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CTOX Desktop")
            .with_inner_size([1600.0, 980.0])
            .with_min_inner_size([1200.0, 760.0])
            .with_icon(load_icon(include_bytes!("../CTOX_app_icon.png"))),
        ..Default::default()
    };

    eframe::run_native(
        "CTOX Desktop",
        native_options,
        Box::new(|cc| Ok(Box::new(CtoxDesktopApp::new(cc)?))),
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))
}

fn load_icon(bytes: &[u8]) -> IconData {
    let image = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .expect("icon format")
        .decode()
        .expect("icon decode")
        .to_rgba8();
    let width = image.width();
    let height = image.height();
    IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}
