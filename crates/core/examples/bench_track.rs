//! Times `compute_track` on a long synthetic recording.
use screencraft_core::input::{Button, ClickEvent, CursorKind, InputLog, MoveEvent};
use screencraft_core::project::{Media, Project, Rect};
use screencraft_core::track::compute_track;
use screencraft_core::zoom::{generate, AutoZoomOptions};

fn main() {
    let minutes = 20.0;
    let dur = minutes * 60_000.0;
    let mut log = InputLog { bounds: Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 }, ..Default::default() };
    let mut t = 0.0;
    while t < dur {
        log.moves.push(MoveEvent { t, x: 960.0 + 800.0 * (t / 1500.0).sin(), y: 540.0 + 400.0 * (t / 900.0).cos(), cursor: CursorKind::Arrow });
        t += 8.0;
    }
    let mut c = 0.0;
    while c < dur {
        log.clicks.push(ClickEvent { t: c, x: 100.0, y: 100.0, button: Button::Left, down: true });
        c += 4000.0;
    }
    let mut p = Project::from_recording("bench", Media { duration_ms: dur, width: 1920, height: 1080, ..Default::default() });
    let t0 = std::time::Instant::now();
    p.scene.zooms = generate(&log, dur, &AutoZoomOptions::default());
    let t1 = t0.elapsed();
    let tr = compute_track(&p, &log, 60.0);
    println!("{} min: {} frames, {} zoom regions; zoom gen {:?}, track {:?}", minutes, tr.count, p.scene.zooms.len(), t1, t0.elapsed() - t1);
}
