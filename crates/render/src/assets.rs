//! Decoded images shared by the compositor (background pictures), cached by content hash.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

type Img = Arc<(u32, u32, Vec<u8>)>;

fn cache() -> &'static Mutex<HashMap<u64, Option<Img>>> {
    static C: OnceLock<Mutex<HashMap<u64, Option<Img>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.len().hash(&mut h);
    s.get(..s.len().min(4096)).unwrap_or("").hash(&mut h);
    s.get(s.len().saturating_sub(4096)..).unwrap_or("").hash(&mut h);
    h.finish()
}

/// A background image given as a data URL, decoded once.
pub fn bg_image(url: &str) -> Option<Img> {
    let k = key(url);
    let mut c = cache().lock().unwrap();
    if let Some(v) = c.get(&k) {
        return v.clone();
    }
    let v = crate::overlay::decode_image(url).map(Arc::new);
    c.insert(k, v.clone());
    v
}
