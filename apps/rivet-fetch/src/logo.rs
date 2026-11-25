//! The picture drawn beside the facts: Rivet as a bitmap, or a text logo from a file. A text
//! logo is lines with `$1`…`$9` switching colour, the same form fastfetch reads, so a fastfetch
//! logo file works here too.

/// Rivet, pixel for pixel from docs/mascot/pixel/rivet-pixel.png (made by docs/mascot/pixel.py),
/// one letter per pixel: `k` ink, `w` his body, `h` its highlights, `g` its shading, `p` his
/// cheeks, `.` clear. Each character cell draws two rows, so he is 31 columns by 17 lines,
/// with one clear line above.
const MASCOT: [&str; 34] = [
    ".......ghhhhwwwhwgkkk..........",
    ".....kghwwwwwwwwwwwwwgk........",
    "....gwhwwwgwwwwwwwwwwwwgk......",
    "..kgwhwgkkkkkwwwwgkkkgwwwk.....",
    "..gwwwkkkgwgkwwwwkkkkkkgwwg....",
    ".kwwwwkgwwwwwwwwwwwwwwkkwwwk...",
    ".gwwwwwwwwwwwwwwwwwwwwwgwwwwk..",
    ".gwwwwwwwwwwwwwwwwwwwwwwwwwwkk.",
    "kkggwwwwwwwwwwwwwwwwwwwwwwwwgk.",
    ".kkkkkkkggggwwwwwwwwwwwwwwwwgk.",
    "..kkkkkkkggkkkkkkkkkkkkkkkkkkk.",
    "....kkkkhhhhgkkkkghhgkkkkkkkkkk",
    "......kwhhwhhgwgwhhhhwkkkkkkkk.",
    "......ghhgkghggghhgkghgkkkk....",
    "......ghwkkggwkwhwkkgwgk.......",
    "......ghgkkkgwkwhgkkkwwk.......",
    "......ghhkkkhggghwkkkhgkk......",
    ".......whwgwwgwghhgkwhkkk......",
    ".......kwhhwgwwgghhhhgkk.......",
    "........ggggwwwwggggkkkk.......",
    "........gwpwwwwwwgpgkkk........",
    "........wwwwwwgwwgggkk.........",
    ".......kwwhwwkkgwgggkk.........",
    ".......kwwhwwkkgwgggkk.........",
    ".......kwhhwwwgwwggkkk.........",
    ".......kwhhwwwwwwggkkk.........",
    ".......kwhhwwwwwwggkkk.........",
    ".......kwhhwwwwwwggkk..........",
    ".......kwwwwwwwwgggkk..........",
    ".......kwwwwwwwwgggkk..........",
    ".......kggwwwwwwgggkk..........",
    "........kkkkkkkkkkkkk..........",
    ".........kkkkkkkkkkkk..........",
    "..................kk...........",
];

/// The colours of the mascot's pixels, the website's palette.
fn colour(c: u8) -> Option<(u8, u8, u8)> {
    match c {
        b'k' => Some((0x24, 0x12, 0x1b)),
        b'w' => Some((0xe9, 0xde, 0xe5)),
        b'h' => Some((0xf7, 0xf2, 0xf4)),
        b'g' => Some((0xb0, 0x9a, 0xa2)),
        b'p' => Some((0xe0, 0x7a, 0xab)),
        _ => None,
    }
}

pub type Rgb = (u8, u8, u8);

/// One character cell of a bitmap: the colours of its top and bottom pixels.
pub type Cell = (Option<Rgb>, Option<Rgb>);

pub enum Logo {
    Text(String),
    Bitmap(Vec<Vec<Cell>>),
}

pub const NAMES: [&str; 2] = ["mascot", "none"];

/// The logo for a name from `NAMES`, or a text logo read from a file at that path.
pub fn load(name: &str) -> Result<Logo, String> {
    match name {
        "mascot" | "rivet" | "" => {
            // A clear line above his cap, so he isn't pressed against the prompt.
            let mut c = cells(&MASCOT);
            c.insert(0, vec![(None, None); MASCOT[0].len()]);
            Ok(Logo::Bitmap(c))
        }
        "none" => Ok(Logo::Text(String::new())),
        path => {
            let path = match path.strip_prefix("~/") {
                Some(rest) => rivet_xdg::places::home().join(rest),
                None => std::path::PathBuf::from(path),
            };
            std::fs::read_to_string(&path)
                .map(Logo::Text)
                .map_err(|e| format!("Couldn't read the logo {}: {e}. Use a file or one of: {}.", path.display(), NAMES.join(", ")))
        }
    }
}

/// A bitmap's rows paired up into character cells.
fn cells(rows: &[&str]) -> Vec<Vec<Cell>> {
    rows.chunks(2)
        .map(|pair| {
            let top = pair[0].as_bytes();
            let bottom = pair.get(1).map(|r| r.as_bytes()).unwrap_or(&[]);
            (0..top.len()).map(|x| (colour(top[x]), bottom.get(x).and_then(|&c| colour(c)))).collect()
        })
        .collect()
}

/// One text logo line, cut into runs of text, each with the colour slot it's drawn in (0
/// before the first `$N`).
pub fn runs(line: &str) -> Vec<(u8, String)> {
    let mut out: Vec<(u8, String)> = vec![(0, String::new())];
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                Some(d @ '1'..='9') => {
                    let slot = *d as u8 - b'0';
                    chars.next();
                    out.push((slot, String::new()));
                    continue;
                }
                // `$$` is a dollar sign, as in fastfetch.
                Some('$') => {
                    chars.next();
                }
                _ => {}
            }
        }
        out.last_mut().unwrap().1.push(c);
    }
    out.retain(|(_, t)| !t.is_empty());
    out
}

/// How many columns a text logo line takes, without its colour codes.
pub fn width(line: &str) -> usize {
    runs(line).iter().map(|(_, t)| t.chars().count()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_codes_take_no_room() {
        assert_eq!(width("$1ab$2c"), 3);
        assert_eq!(width("$$5"), 2);
        assert_eq!(runs("x$3yy"), vec![(0, "x".into()), (3, "yy".into())]);
    }

    #[test]
    fn the_mascot_is_whole() {
        assert!(MASCOT.iter().all(|r| r.len() == MASCOT[0].len() && r.bytes().all(|c| c == b'.' || colour(c).is_some())));
        let Ok(Logo::Bitmap(c)) = load("mascot") else { panic!() };
        assert_eq!((c.len(), c[0].len()), (18, 31));
    }
}
