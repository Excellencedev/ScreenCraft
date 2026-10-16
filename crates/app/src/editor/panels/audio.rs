use super::*;
use crate::editor::Sel;

#[derive(Default)]
pub struct AudioState {}

const CLICK_STYLES: [(&str, &str); 3] = [("soft", "Soft"), ("crisp", "Crisp"), ("classic", "Classic")];
const KEY_STYLES: [(&str, &str); 3] = [("mechanical", "Mechanical"), ("soft", "Soft"), ("thock", "Thock")];

pub fn show(st: &mut EditorState, ui: &mut Ui) {
    let a = st.project.config.audio.clone();
    let m = st.project.media.clone();
    kit::h3(ui, "Audio", true);
    if m.mic.is_none() && !m.has_microphone && !m.has_system_audio {
        note(ui, "No audio was recorded. Enable the microphone or system audio in the recorder toolbar.");
    }
    slider(st, ui, "a-master", "Master volume", a.volume, (0.0, 1.0, 0.01), pct, |p, v| p.config.audio.volume = v);
    slider(st, ui, "a-mic", "Microphone", a.mic_volume, (0.0, 1.0, 0.01), pct, |p, v| p.config.audio.mic_volume = v);
    if m.mic.is_none() && m.mic_track.is_none() {
        kit::hint(ui, "This recording has no separate microphone track (it was recorded before microphones were split out), so the Device slider controls everything.");
    }
    slider(st, ui, "a-dev", "Device audio", a.device_volume, (0.0, 1.0, 0.01), pct, |p, v| p.config.audio.device_volume = v);

    kit::h3(ui, "Sound effects", false);
    toggle(st, ui, "Mouse click sounds", Some("Plays a click at every click in the recording."), a.click_sounds, |p, v| p.config.audio.click_sounds = v);
    if a.click_sounds {
        let opts: Vec<(String, &str)> = CLICK_STYLES.iter().map(|(a, b)| (a.to_string(), *b)).collect();
        let before = a.click_style.clone();
        seg(st, ui, Some("Click style"), &opts, a.click_style.clone(), |p, v| p.config.audio.click_style = v);
        if st.project.config.audio.click_style != before {
            let (style, vol) = (st.project.config.audio.click_style.clone(), a.click_volume);
            st.preview_sound("click", &style, vol);
        }
        slider(st, ui, "a-clickvol", "Click volume", a.click_volume, (0.0, 1.0, 0.01), pct, |p, v| p.config.audio.click_volume = v);
    }
    toggle(st, ui, "Keyboard typing sounds", Some("Plays a key sound for every key you pressed."), a.key_sounds, |p, v| p.config.audio.key_sounds = v);
    if a.key_sounds {
        let opts: Vec<(String, &str)> = KEY_STYLES.iter().map(|(a, b)| (a.to_string(), *b)).collect();
        let before = a.key_style.clone();
        seg(st, ui, Some("Keyboard style"), &opts, a.key_style.clone(), |p, v| p.config.audio.key_style = v);
        if st.project.config.audio.key_style != before {
            let (style, vol) = (st.project.config.audio.key_style.clone(), a.key_volume);
            st.preview_sound("key", &style, vol);
        }
        slider(st, ui, "a-keyvol", "Typing volume", a.key_volume, (0.0, 1.0, 0.01), pct, |p, v| p.config.audio.key_volume = v);
    }

    kit::h3(ui, "Music", false);
    let clips = st.project.scene.music_clips.clone();
    if !clips.is_empty() && (m.mic.is_some() || m.has_microphone) {
        toggle(st, ui, "Lower music when I speak", Some("Ducks the music under your microphone."), a.duck_music, |p, v| p.config.audio.duck_music = v);
        if a.duck_music {
            slider(st, ui, "duck-level", "Music while speaking", a.duck_level, (0.05, 0.8, 0.05), pct, |p, v| p.config.audio.duck_level = v);
        }
    }
    let sel = if let Sel::Music(id) = &st.sel { clips.iter().find(|c| &c.id == id).cloned() } else { None };
    if let Some(c) = &sel {
        let id = c.id.clone();
        slider(st, ui, &format!("fade-in-{id}"), "Fade in", c.fade_in_ms / 1000.0, (0.0, 5.0, 0.1), |v| format!("{v:.1} s"), {
            let id = id.clone();
            move |p, v| {
                if let Some(x) = p.scene.music_clips.iter_mut().find(|x| x.id == id) {
                    x.fade_in_ms = (v * 1000.0).round();
                }
            }
        });
        slider(st, ui, &format!("fade-out-{id}"), "Fade out", c.fade_out_ms / 1000.0, (0.0, 5.0, 0.1), |v| format!("{v:.1} s"), {
            let id = id.clone();
            move |p, v| {
                if let Some(x) = p.scene.music_clips.iter_mut().find(|x| x.id == id) {
                    x.fade_out_ms = (v * 1000.0).round();
                }
            }
        });
    }
    if !clips.is_empty() {
        slider(st, ui, "a-music", "Music volume", a.music_volume, (0.0, 1.0, 0.01), pct, |p, v| p.config.audio.music_volume = v);
        kit::hint(ui, "Drag a green block to place it, drag its ends to trim, and press S to split at the playhead.");
    }
    if kit::btn_line(ui, "Add music…", false).clicked() {
        st.add_music();
    }
    if sel.is_some() && kit::btn_line(ui, "Delete selected music clip", true).clicked() {
        st.delete_selected();
    }
}
