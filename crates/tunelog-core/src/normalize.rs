//! Normalization of artist and track names so the same song from different
//! sources (live tracker, Spotify export, Takeout, …) maps to the same key.

/// Normalized artist name: lowercase, without featured artists and the
/// YouTube auto-generated " - Topic" suffix.
pub fn artist(raw: &str) -> String {
    let mut s = raw.trim();
    if let Some(stripped) = strip_suffix_ci(s, " - topic") {
        s = stripped;
    }
    let lower = s.to_lowercase();
    let cut = cut_featuring(&lower);
    collapse_ws(cut)
}

/// Normalized track title: lowercase, without featured artists and common
/// video decorations such as "(Official Video)" or "[Lyrics]".
pub fn title(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    let mut out = String::with_capacity(lower.len());
    // Drop bracketed groups that only decorate the title.
    let mut rest = lower.as_str();
    while let Some(open) = rest.find(['(', '[']) {
        let close_ch = if rest.as_bytes()[open] == b'(' {
            ')'
        } else {
            ']'
        };
        let Some(close_rel) = rest[open + 1..].find(close_ch) else {
            break;
        };
        let close = open + 1 + close_rel;
        let inner = &rest[open + 1..close];
        out.push_str(&rest[..open]);
        if !is_decoration(inner) {
            out.push_str(&rest[open..=close]);
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    let cut = cut_featuring(&out);
    collapse_ws(cut)
}

const DECORATIONS: &[&str] = &[
    "official video",
    "official music video",
    "official audio",
    "official lyric video",
    "official visualizer",
    "lyric video",
    "lyrics",
    "audio",
    "visualizer",
    "hd",
    "hq",
    "4k",
];

fn is_decoration(inner: &str) -> bool {
    let inner = inner.trim();
    inner.starts_with("feat") || inner.starts_with("ft.") || DECORATIONS.contains(&inner)
}

/// Cuts everything starting at a featuring marker ("feat.", "ft.", "featuring").
fn cut_featuring(s: &str) -> &str {
    const MARKERS: &[&str] = &[" feat. ", " feat ", " ft. ", " featuring "];
    let mut end = s.len();
    for m in MARKERS {
        if let Some(i) = s.find(m) {
            end = end.min(i);
        }
    }
    &s[..end]
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_suffix_ci<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let n = s.len().checked_sub(suffix.len())?;
    (s.is_char_boundary(n) && s[n..].eq_ignore_ascii_case(suffix)).then(|| &s[..n])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artist_variants_collapse() {
        assert_eq!(artist("Daft Punk"), "daft punk");
        assert_eq!(artist("Daft Punk - Topic"), "daft punk");
        assert_eq!(artist("  DAFT   PUNK "), "daft punk");
        assert_eq!(artist("Daft Punk feat. Pharrell Williams"), "daft punk");
        assert_eq!(artist("Daft Punk ft. Pharrell"), "daft punk");
        assert_eq!(artist("Simon & Garfunkel"), "simon & garfunkel");
    }

    #[test]
    fn title_variants_collapse() {
        let expected = "get lucky";
        for raw in [
            "Get Lucky",
            "Get Lucky (feat. Pharrell Williams)",
            "Get Lucky [feat. Pharrell Williams]",
            "Get Lucky feat. Pharrell Williams",
            "Get Lucky (Official Video)",
            "Get Lucky [Official Music Video] (feat. Pharrell)",
            "GET LUCKY (Lyrics)",
        ] {
            assert_eq!(title(raw), expected, "{raw}");
        }
    }

    #[test]
    fn title_keeps_meaningful_brackets() {
        assert_eq!(title("Strobe (Club Edit)"), "strobe (club edit)");
        assert_eq!(title("Song (Remix) [Official Video]"), "song (remix)");
    }

    #[test]
    fn title_handles_unbalanced_brackets() {
        assert_eq!(title("Weird (title"), "weird (title");
    }

    #[test]
    fn non_ascii_is_safe() {
        assert_eq!(artist("Кино"), "кино");
        assert_eq!(title("Группа крови (Official Audio)"), "группа крови");
        assert_eq!(artist("ä"), "ä");
    }
}
