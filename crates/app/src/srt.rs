//! SRT / WebVTT parsing and writing (captions), in milliseconds.

pub struct Item {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

fn ts(s: &str) -> Option<f64> {
    // hh:mm:ss,mmm | mm:ss.mmm
    let s = s.trim();
    let (hms, frac) = s.split_once([',', '.'])?;
    let parts: Vec<f64> = hms.split(':').filter_map(|p| p.trim().parse().ok()).collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (*h, *m, *s),
        [m, s] => (0.0, *m, *s),
        _ => return None,
    };
    let frac: String = frac.chars().take_while(|c| c.is_ascii_digit()).take(3).collect();
    let ms: f64 = format!("{frac:0<3}").parse().ok()?;
    Some(((h * 60.0 + m) * 60.0 + sec) * 1000.0 + ms)
}

pub fn parse(text: &str) -> Vec<Item> {
    let text = text.replace('\r', "");
    let mut out = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.split('\n').filter(|l| !l.is_empty()).collect();
        let Some(at) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = lines[at].split_once("-->") else { continue };
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (ts(a), ts(b)) else { continue };
        let mut body = String::new();
        let mut in_tag = false;
        for ch in lines[at + 1..].join("\n").chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => body.push(c),
                _ => {}
            }
        }
        let body = body.trim().to_string();
        if !body.is_empty() {
            out.push(Item { start, end, text: body });
        }
    }
    out
}

fn stamp(ms: f64) -> String {
    let ms = ms.max(0.0).round() as u64;
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000)
}

pub fn write(items: &[Item]) -> String {
    items.iter().enumerate().map(|(i, it)| format!("{}\n{} --> {}\n{}\n", i + 1, stamp(it.start), stamp(it.end), it.text)).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let items = vec![Item { start: 1500.0, end: 3250.0, text: "Hello there".into() }, Item { start: 4000.0, end: 5000.0, text: "Bye".into() }];
        let s = write(&items);
        let back = parse(&s);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].start, 1500.0);
        assert_eq!(back[0].end, 3250.0);
        assert_eq!(back[1].text, "Bye");
    }

    #[test]
    fn parses_vtt_with_tags() {
        let v = "WEBVTT\n\n00:01.000 --> 00:02.500 align:start\n<c>Hi</c> you\n";
        let it = parse(v);
        assert_eq!(it.len(), 1);
        assert_eq!(it[0].text, "Hi you");
        assert_eq!(it[0].end, 2500.0);
    }
}
