//! Plain-text palette files, as Lospec and pixel-art tools share them:
//!
//! - `.gpl`: GIMP palette (`GIMP Palette` header, `Name:`/`Columns:` lines, `#` comments, then
//!   `R G B name` rows, 0–255).
//! - `.pal`: JASC (Paint Shop Pro) palette (`JASC-PAL`, `0100`, a count, then `R G B` rows).
//! - `.hex`: one `RRGGBB` per line (a leading `#` is fine). Paint.NET's `.txt` flavour, with `;`
//!   comments and `AARRGGBB` lines, reads too.
//!
//! All three hold sRGB colours with no colour model, groups or spot colours. Parsing never
//! panics: bad rows are skipped and counted, and at most [`MAX_COLORS`] are read.

/// Most colours read from one file (the Swatches panel's own limit is far above any palette).
pub const MAX_COLORS: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFormat {
    Gpl,
    Pal,
    Hex,
}

/// One colour: its name (may be empty) and 8-bit sRGB.
pub type Entry = (String, [u8; 3]);

/// What a file held: its colours, the palette's own name (GIMP `Name:`), and how many rows
/// couldn't be read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextPalette {
    pub colors: Vec<Entry>,
    pub name: Option<String>,
    pub skipped: usize,
}

/// The format of `bytes`, from its header (GIMP, JASC) or, failing that, from its lines all
/// being hex colours.
pub fn sniff(bytes: &[u8]) -> Option<TextFormat> {
    let text = std::str::from_utf8(bytes).ok()?;
    let first = text.trim_start_matches('\u{feff}').lines().map(str::trim).find(|l| !l.is_empty())?;
    if first.eq_ignore_ascii_case("GIMP Palette") {
        return Some(TextFormat::Gpl);
    }
    if first.eq_ignore_ascii_case("JASC-PAL") {
        return Some(TextFormat::Pal);
    }
    let mut rows = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(';')).peekable();
    rows.peek()?;
    rows.all(|l| hex_color(l).is_some()).then_some(TextFormat::Hex)
}

/// Read a palette file. Errors when it isn't text or holds no colour.
pub fn parse(bytes: &[u8], format: TextFormat) -> Result<TextPalette, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not a text palette file".to_string())?;
    let text = text.trim_start_matches('\u{feff}');
    let mut p = match format {
        TextFormat::Gpl => gpl(text)?,
        TextFormat::Pal => pal(text)?,
        TextFormat::Hex => hex(text),
    };
    p.colors.truncate(MAX_COLORS);
    if p.colors.is_empty() {
        return Err("the file holds no colours".into());
    }
    Ok(p)
}

fn byte(s: &str) -> Option<u8> {
    s.parse::<u16>().ok().and_then(|v| u8::try_from(v).ok())
}

/// `R G B` (0–255) at the start of `line`, and the rest of it.
fn rgb_row(line: &str) -> Option<([u8; 3], &str)> {
    let mut rest = line.trim_start();
    let mut c = [0u8; 3];
    for v in &mut c {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        *v = byte(rest.get(..end)?)?;
        rest = rest.get(end..)?.trim_start();
    }
    Some((c, rest))
}

fn gpl(text: &str) -> Result<TextPalette, String> {
    let mut lines = text.lines().map(str::trim);
    if !lines.next().is_some_and(|l| l.eq_ignore_ascii_case("GIMP Palette")) {
        return Err("not a GIMP palette (no `GIMP Palette` header)".into());
    }
    let mut p = TextPalette::default();
    for l in lines {
        if p.colors.len() >= MAX_COLORS {
            break;
        }
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(n) = l.strip_prefix("Name:") {
            let n = n.trim();
            if !n.is_empty() {
                p.name = Some(n.chars().take(200).collect());
            }
            continue;
        }
        if l.starts_with("Columns:") {
            continue;
        }
        match rgb_row(l) {
            Some((c, name)) => {
                // GIMP writes "Untitled" for unnamed colours.
                let name = if name.eq_ignore_ascii_case("untitled") { "" } else { name };
                p.colors.push((name.chars().take(200).collect(), c));
            }
            None => p.skipped += 1,
        }
    }
    Ok(p)
}

fn pal(text: &str) -> Result<TextPalette, String> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if !lines.next().is_some_and(|l| l.eq_ignore_ascii_case("JASC-PAL")) {
        return Err("not a JASC palette (no `JASC-PAL` header)".into());
    }
    // Version ("0100") and the colour count; the rows that follow are what counts.
    let _version = lines.next();
    let _count = lines.next();
    let mut p = TextPalette::default();
    for l in lines {
        if p.colors.len() >= MAX_COLORS {
            break;
        }
        match rgb_row(l) {
            Some((c, _)) => p.colors.push((String::new(), c)),
            None => p.skipped += 1,
        }
    }
    Ok(p)
}

/// `RRGGBB`, `#RRGGBB` or Paint.NET's `AARRGGBB` (alpha ignored).
fn hex_color(line: &str) -> Option<[u8; 3]> {
    let h = line.trim().trim_start_matches('#');
    let h = match h.len() {
        6 => h,
        8 => h.get(2..)?,
        _ => return None,
    };
    if !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
    Some([v(0)?, v(2)?, v(4)?])
}

fn hex(text: &str) -> TextPalette {
    let mut p = TextPalette::default();
    for l in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with(';')) {
        if p.colors.len() >= MAX_COLORS {
            break;
        }
        match hex_color(l) {
            Some(c) => p.colors.push((String::new(), c)),
            None => p.skipped += 1,
        }
    }
    p
}

/// Write `colors` (named `name`) in `format`.
pub fn write(colors: &[Entry], name: &str, format: TextFormat) -> String {
    let mut out = String::new();
    match format {
        TextFormat::Gpl => {
            out.push_str("GIMP Palette\n");
            let name: String = name.chars().filter(|c| !c.is_control()).collect();
            out.push_str(&format!("Name: {name}\n#\n"));
            for (n, [r, g, b]) in colors {
                let n: String = n.chars().filter(|c| !c.is_control()).collect();
                out.push_str(&format!("{r:3} {g:3} {b:3}\t{n}\n"));
            }
        }
        TextFormat::Pal => {
            out.push_str(&format!("JASC-PAL\r\n0100\r\n{}\r\n", colors.len()));
            for (_, [r, g, b]) in colors {
                out.push_str(&format!("{r} {g} {b}\r\n"));
            }
        }
        TextFormat::Hex => {
            for (_, [r, g, b]) in colors {
                out.push_str(&format!("{r:02x}{g:02x}{b:02x}\n"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_lospec_gimp_palette() {
        let f = "GIMP Palette\nName: Sweetie 16\nColumns: 4\n#\n# from lospec\n 26  28  44\tUntitled\n 93  39  93\tplum\n";
        assert_eq!(sniff(f.as_bytes()), Some(TextFormat::Gpl));
        let p = parse(f.as_bytes(), TextFormat::Gpl).unwrap();
        assert_eq!(p.name.as_deref(), Some("Sweetie 16"));
        assert_eq!(p.colors, vec![(String::new(), [26, 28, 44]), ("plum".into(), [93, 39, 93])]);
    }

    #[test]
    fn reads_jasc_and_hex() {
        let f = "JASC-PAL\r\n0100\r\n2\r\n26 28 44\r\n255 0 77\r\n";
        assert_eq!(sniff(f.as_bytes()), Some(TextFormat::Pal));
        assert_eq!(parse(f.as_bytes(), TextFormat::Pal).unwrap().colors.len(), 2);
        let h = "1a1c2c\n#5D275D\n\n";
        assert_eq!(sniff(h.as_bytes()), Some(TextFormat::Hex));
        assert_eq!(parse(h.as_bytes(), TextFormat::Hex).unwrap().colors[1].1, [0x5d, 0x27, 0x5d]);
        let paint_net = "; paint.net palette\nFF1A1C2C\n";
        assert_eq!(parse(paint_net.as_bytes(), TextFormat::Hex).unwrap().colors[0].1, [0x1a, 0x1c, 0x2c]);
    }

    #[test]
    fn round_trips_through_every_format() {
        let colors = vec![("ink".to_string(), [1u8, 2, 3]), (String::new(), [255, 128, 0])];
        for f in [TextFormat::Gpl, TextFormat::Pal, TextFormat::Hex] {
            let text = write(&colors, "Test", f);
            assert_eq!(sniff(text.as_bytes()), Some(f));
            let back = parse(text.as_bytes(), f).unwrap();
            assert_eq!(back.colors.iter().map(|c| c.1).collect::<Vec<_>>(), [[1, 2, 3], [255, 128, 0]], "{f:?}");
        }
    }

    #[test]
    fn bad_input_is_an_error_or_skipped_never_a_panic() {
        assert!(sniff(b"\xff\xfe\x00").is_none());
        assert!(sniff(b"").is_none());
        assert!(sniff(b"hello world").is_none());
        assert!(parse(b"\xff\xfe", TextFormat::Hex).is_err());
        assert!(parse(b"GIMP Palette\n", TextFormat::Gpl).is_err(), "no colours");
        assert!(parse(b"not gimp\n1 2 3\n", TextFormat::Gpl).is_err());
        let p = parse("GIMP Palette\n300 0 0\n1 2\n-1 2 3\n9 9 9\né é é\n".as_bytes(), TextFormat::Gpl).unwrap();
        assert_eq!((p.colors.len(), p.skipped), (1, 4));
        assert!(parse("JASC-PAL".as_bytes(), TextFormat::Pal).is_err());
        assert_eq!(parse("12345g\nabcdef\n".as_bytes(), TextFormat::Hex).unwrap().skipped, 1);
        let many = "ffffff\n".repeat(MAX_COLORS + 10);
        assert_eq!(parse(many.as_bytes(), TextFormat::Hex).unwrap().colors.len(), MAX_COLORS);
    }
}
