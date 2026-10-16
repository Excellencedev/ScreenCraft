fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        // The recorder bar: borderless, transparent, always on top, bottom-centre of the display.
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ScreenCraft")
            .with_inner_size([screencraft::recorder::BAR_W, screencraft::recorder::BAR_H])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_has_shadow(false)
            .with_always_on_top(),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native("ScreenCraft", options, Box::new(|cc| Ok(Box::new(screencraft::app::App::new(cc)))))
}
