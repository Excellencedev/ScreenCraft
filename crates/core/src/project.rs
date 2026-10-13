//! ScreenCraft project document. Serialised as `project.json` inside a
//! `.screencraft` package directory next to the recorded media.

use crate::spring::SpringConfig;
use serde::{Deserialize, Serialize};

pub const PROJECT_VERSION: &str = "1.0.0";

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const FULL: Rect = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
}

impl Default for Rect {
    fn default() -> Self {
        Rect::FULL
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Media {
    /// Screen video, relative to the package directory.
    pub screen: String,
    /// Optional camera video.
    pub camera: Option<String>,
    /// Cursor / click / key log produced by the input tracker.
    pub input: String,
    pub duration_ms: f64,
    /// Pixel size of the screen video.
    pub width: u32,
    pub height: u32,
    /// Captured area in global points.
    pub bounds: Rect,
    /// Seconds to add to the camera clock to line it up with the screen.
    pub camera_offset_ms: f64,
    pub has_system_audio: bool,
    pub has_microphone: bool,
    /// Separately recorded microphone track (so its volume can differ from system audio).
    pub mic: Option<String>,
    pub mic_offset_ms: f64,
}

impl Default for Media {
    fn default() -> Self {
        Self {
            screen: "screen.mp4".into(),
            camera: None,
            input: "input.json".into(),
            duration_ms: 0.0,
            width: 1920,
            height: 1080,
            bounds: Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 },
            camera_offset_ms: 0.0,
            has_system_audio: false,
            has_microphone: false,
            mic: None,
            mic_offset_ms: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Slice {
    pub id: String,
    pub source_start_ms: f64,
    pub source_end_ms: f64,
    pub time_scale: f64,
    pub volume: f64,
    pub hide_cursor: bool,
    pub disable_smooth_mouse: bool,
    /// A held single frame (made by "Freeze frame"); its source range is ~1 frame stretched in time.
    pub freeze: bool,
    /// Transition played across the cut between the previous slice and this one.
    pub transition: Option<Transition>,
}

impl Default for Slice {
    fn default() -> Self {
        Self {
            id: "slice-0".into(),
            source_start_ms: 0.0,
            source_end_ms: 0.0,
            time_scale: 1.0,
            volume: 1.0,
            hide_cursor: false,
            disable_smooth_mouse: false,
            freeze: false,
            transition: None,
        }
    }
}

/// A transition centred on the cut at the start of a slice. `kind`: fade | flash | blur | slide | zoom.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Transition {
    pub kind: String,
    pub duration_ms: f64,
}

impl Default for Transition {
    fn default() -> Self {
        Self { kind: "fade".into(), duration_ms: 500.0 }
    }
}

/// Something drawn over the video for a stretch of the recording.
/// `kind`: text | box | ellipse | arrow | highlight | blur | pixelate | spotlight.
/// `space`: "content" is pinned to the recording (moves and scales with the camera),
/// "screen" is fixed on the output frame. Coordinates are 0..1 of that space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Annotation {
    pub id: String,
    pub kind: String,
    pub space: String,
    pub source_start_ms: f64,
    pub source_end_ms: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Arrow end point.
    pub x2: f64,
    pub y2: f64,
    pub text: String,
    pub color: String,
    /// Background fill for text; empty for none.
    pub fill: String,
    /// Text size in px at 1080p.
    pub font_size: f64,
    pub thickness: f64,
    /// Blur / pixelate strength 0..1, spotlight dim 0..1.
    pub strength: f64,
    /// fade | pop | none
    pub anim: String,
    pub anim_ms: f64,
}

impl Default for Annotation {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: "text".into(),
            space: "screen".into(),
            source_start_ms: 0.0,
            source_end_ms: 0.0,
            x: 0.5,
            y: 0.5,
            w: 0.3,
            h: 0.12,
            x2: 0.6,
            y2: 0.6,
            text: "Text".into(),
            color: "#ffffff".into(),
            fill: "#000000aa".into(),
            font_size: 44.0,
            thickness: 5.0,
            strength: 0.6,
            anim: "fade".into(),
            anim_ms: 250.0,
        }
    }
}

/// What the camera looks like for a stretch of the recording.
/// `layout`: bubble | hidden | large | camera | side-right | side-left.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CameraLayoutRegion {
    pub id: String,
    pub source_start_ms: f64,
    pub source_end_ms: f64,
    pub layout: String,
}

impl Default for CameraLayoutRegion {
    fn default() -> Self {
        Self { id: String::new(), source_start_ms: 0.0, source_end_ms: 0.0, layout: "bubble".into() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ZoomKind {
    /// Camera keeps the cursor in view.
    FollowCursor,
    /// Camera stays on `manual_target_point`.
    Manual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ZoomRegion {
    pub id: String,
    pub zoom: f64,
    pub kind: ZoomKind,
    pub manual_target_point: Point,
    pub is_disabled: bool,
    pub source_start_ms: f64,
    pub source_end_ms: f64,
    pub instant: bool,
    /// Per-region ease durations; `None` uses the project default.
    pub ease_in_ms: Option<f64>,
    pub ease_out_ms: Option<f64>,
}

impl Default for ZoomRegion {
    fn default() -> Self {
        Self {
            id: String::new(),
            zoom: 2.0,
            kind: ZoomKind::FollowCursor,
            manual_target_point: Point { x: 0.5, y: 0.5 },
            is_disabled: false,
            source_start_ms: 0.0,
            source_end_ms: 0.0,
            instant: false,
            ease_in_ms: None,
            ease_out_ms: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Scene {
    pub slices: Vec<Slice>,
    pub zooms: Vec<ZoomRegion>,
    /// Legacy single music track (migrated into `music_clips` when a project opens).
    pub music: Option<MusicTrack>,
    /// Background music clips laid under the edit, placed on the output timeline.
    pub music_clips: Vec<MusicClip>,
    pub annotations: Vec<Annotation>,
    pub camera_layouts: Vec<CameraLayoutRegion>,
}

/// A trimmed piece of an audio file in the project folder.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MusicClip {
    pub id: String,
    pub file: String,
    pub name: String,
    /// Where this piece starts on the output timeline.
    pub offset_ms: f64,
    /// Start within the file.
    pub in_ms: f64,
    /// Length of the piece.
    pub dur_ms: f64,
    /// Full length of the file.
    pub file_dur_ms: f64,
    pub fade_in_ms: f64,
    pub fade_out_ms: f64,
}

/// An audio file in the project folder, positioned on the edited timeline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MusicTrack {
    pub file: String,
    pub name: String,
    /// Where the music starts on the output timeline.
    pub offset_ms: f64,
    pub duration_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClickEffect {
    None,
    Circle,
    Ripple,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CursorConfig {
    pub hide: bool,
    /// Cursor height as a fraction of the output frame height... in px at 1080p.
    pub size: f64,
    pub style: String,
    pub always_use_pointer: bool,
    pub hide_not_moving_after_ms: Option<f64>,
    pub loop_position_before_end_ms: Option<f64>,
    pub click_effect: ClickEffect,
    pub click_effect_color: String,
    pub click_sound: Option<String>,
    pub click_sound_volume: f64,
    /// Tilt the cursor in the direction of movement.
    pub rotate_with_velocity: bool,
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            hide: false,
            size: 28.0,
            style: "macos".into(),
            always_use_pointer: false,
            hide_not_moving_after_ms: None,
            loop_position_before_end_ms: None,
            click_effect: ClickEffect::None,
            click_effect_color: "#ffffff".into(),
            click_sound: None,
            click_sound_volume: 0.25,
            rotate_with_velocity: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CameraConfig {
    pub hide: bool,
    pub mirror: bool,
    /// width / height
    pub aspect_ratio: f64,
    /// Camera size as a fraction of the shorter output side.
    pub size_ratio: f64,
    /// Position of the camera in the frame, 0..1 on each axis.
    pub position: Point,
    pub padding_ratio: f64,
    pub corner_radius_ratio: f64,
    /// Scale applied to the camera while zoomed in.
    pub zoomed_scale: f64,
    pub shadow: bool,
    pub border: bool,
    /// Border colour (hex) used when `border` is on.
    pub border_color: String,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            hide: false,
            mirror: true,
            aspect_ratio: 1.0,
            size_ratio: 0.30,
            position: Point { x: 1.0, y: 1.0 },
            padding_ratio: 0.5,
            corner_radius_ratio: 0.25,
            zoomed_scale: 0.7,
            shadow: true,
            border: false,
            border_color: "#ffffff".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Shadow {
    pub intensity: f64,
    pub distance: f64,
    pub angle: f64,
    pub blur: f64,
    pub directional: bool,
}

impl Default for Shadow {
    fn default() -> Self {
        Self { intensity: 0.3, distance: 25.0, angle: 45.0, blur: 20.0, directional: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Inset {
    pub enabled: bool,
    pub size: f64,
    pub color: String,
    pub alpha: f64,
}

impl Default for Inset {
    fn default() -> Self {
        Self { enabled: false, size: 6.0, color: "#000000".into(), alpha: 0.5 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Background {
    /// "wallpaper" | "gradient" | "color" | "image"
    pub kind: String,
    pub wallpaper: String,
    pub color: String,
    pub gradient_from: String,
    pub gradient_to: String,
    pub gradient_angle: f64,
    /// Path or data URL of a user supplied image.
    pub image: Option<String>,
    pub blur: f64,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            kind: "wallpaper".into(),
            wallpaper: "sunset".into(),
            color: "#4776e6".into(),
            gradient_from: "#4776e6".into(),
            gradient_to: "#8e54e9".into(),
            gradient_angle: 135.0,
            image: None,
            blur: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Styles {
    pub padding_ratio: f64,
    pub corner_radius: f64,
    pub shadow: Shadow,
    pub inset: Inset,
    pub background: Background,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            padding_ratio: 0.02,
            corner_radius: 16.0,
            shadow: Shadow::default(),
            inset: Inset::default(),
            background: Background::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Output {
    /// "auto" | "16:9" | "9:16" | "1:1" | "4:3" | "3:4" | "4:5" | "21:9"
    pub aspect_ratio: String,
    pub resolution: u32,
    pub fps: u32,
    pub format: String,
}

impl Default for Output {
    fn default() -> Self {
        Self { aspect_ratio: "auto".into(), resolution: 1080, fps: 60, format: "mp4".into() }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Animations {
    pub mouse_spring: SpringConfig,
    pub screen_spring: SpringConfig,
    pub motion_blur: f64,
    /// Default zoom level for newly created regions.
    pub default_zoom: f64,
    /// "center" keeps the cursor (or text caret while typing) in the middle of
    /// the zoomed view; "edge" only pans when it nears the border.
    pub follow: String,
    /// Default zoom ease-in / ease-out duration.
    pub ease_in_ms: f64,
    pub ease_out_ms: f64,
    /// How softly the camera glides after the cursor / caret.
    pub follow_smooth_ms: f64,
    /// Extra smoothing applied to the drawn cursor itself.
    pub cursor_smooth_ms: f64,
    /// Camera follow time while typing: shorter, so the caret stays near the middle.
    pub typing_smooth_ms: f64,
}

impl Default for Animations {
    fn default() -> Self {
        Self {
            mouse_spring: SpringConfig::default(),
            screen_spring: SpringConfig::default(),
            motion_blur: 1.0,
            default_zoom: 2.0,
            follow: "center".into(),
            ease_in_ms: 500.0,
            ease_out_ms: 500.0,
            follow_smooth_ms: 450.0,
            cursor_smooth_ms: 120.0,
            typing_smooth_ms: 160.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioConfig {
    pub volume: f64,
    /// Microphone level (separate mic track).
    pub mic_volume: f64,
    /// Device / system audio level (the screen recording's audio).
    pub device_volume: f64,
    /// Background music level.
    pub music_volume: f64,
    pub mute_microphone: bool,
    pub mute_system_audio: bool,
    /// Built-in mouse click sound effects, placed on every recorded click.
    pub click_sounds: bool,
    /// Sound set id (see `ui/sounds`): soft | crisp | classic.
    pub click_style: String,
    pub click_volume: f64,
    /// Built-in keyboard typing sounds, placed on every recorded key press.
    pub key_sounds: bool,
    /// Sound set id: mechanical | soft | thock.
    pub key_style: String,
    pub key_volume: f64,
    /// Lower the music while someone is speaking (microphone level).
    pub duck_music: bool,
    /// Music level while ducked, 0..1 of the normal level.
    pub duck_level: f64,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            volume: 0.5,
            mic_volume: 1.0,
            device_volume: 1.0,
            music_volume: 0.5,
            mute_microphone: false,
            mute_system_audio: false,
            click_sounds: false,
            click_style: "soft".into(),
            click_volume: 0.6,
            key_sounds: false,
            key_style: "mechanical".into(),
            key_volume: 0.5,
            duck_music: false,
            duck_level: 0.3,
        }
    }
}

/// On-screen display of keyboard shortcuts (only keys recorded with a label: shortcuts and
/// special keys; ordinary typing is never stored as text).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeystrokeConfig {
    pub enabled: bool,
    /// bottom-center | bottom-left | bottom-right | top-center
    pub position: String,
    /// Size in px at 1080p.
    pub size: f64,
    /// How long each shortcut stays up.
    pub hold_ms: f64,
}

impl Default for KeystrokeConfig {
    fn default() -> Self {
        Self { enabled: false, position: "bottom-center".into(), size: 30.0, hold_ms: 1400.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub crop: Rect,
    pub cursor: CursorConfig,
    pub camera: CameraConfig,
    pub styles: Styles,
    pub output: Output,
    pub animations: Animations,
    pub audio: AudioConfig,
    pub keystrokes: KeystrokeConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub version: String,
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub media: Media,
    pub scene: Scene,
    pub config: Config,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: PROJECT_VERSION.into(),
            id: String::new(),
            name: "Untitled".into(),
            created_at: String::new(),
            media: Media::default(),
            scene: Scene::default(),
            config: Config::default(),
        }
    }
}

impl Project {
    /// A fresh project covering the whole recording with no cuts.
    pub fn from_recording(name: &str, media: Media) -> Self {
        let slice = Slice {
            id: "slice-0".into(),
            source_start_ms: 0.0,
            source_end_ms: media.duration_ms,
            ..Default::default()
        };
        Self {
            name: name.into(),
            scene: Scene { slices: vec![slice], ..Default::default() },
            media,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_json() {
        let p = Project::from_recording("t", Media { duration_ms: 5000.0, ..Default::default() });
        let s = serde_json::to_string(&p).unwrap();
        let q: Project = serde_json::from_str(&s).unwrap();
        assert_eq!(p, q);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let q: Project = serde_json::from_str(r#"{"name":"x","config":{"cursor":{"size":40}}}"#).unwrap();
        assert_eq!(q.config.cursor.size, 40.0);
        assert_eq!(q.config.styles.corner_radius, 16.0);
    }
}
