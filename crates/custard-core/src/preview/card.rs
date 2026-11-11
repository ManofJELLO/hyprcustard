//! What the demo window has in it.
//!
//! A terminal showing two lines of text is mostly black, and black is the one
//! thing a shader cannot be judged against: a dissolve, a tint, a blur and a
//! saturation curve all look identical on it. So the demo window shows a test
//! card instead — colour bars, a grey ramp, a hue sweep, a fine checkerboard
//! and a line of text — chosen so that every common kind of shader has
//! something in the window it visibly acts on.
//!
//! It is rendered to a file and `cat`ed by the demo shell rather than built
//! out of `printf` arguments: the card is a few hundred escape sequences, and
//! threading those through a shell command line means quoting them twice.

use std::path::{Path, PathBuf};

/// Terminal cell size in pixels, for working out how big the card can be.
///
/// A guess, and it has to be: the window is sized by the compositor in pixels
/// and filled by whichever terminal is installed, at whatever font that
/// terminal was configured with, which nothing here can ask about. The guess
/// is deliberately on the large side, because the two ways of being wrong are
/// not equal — a card narrower than the window leaves a margin, while one a
/// single column too wide wraps every band onto the next line and the card
/// falls apart.
const CELL: (u32, u32) = (12, 24);

/// Pixels the window spends on its own frame: the preview's gaps and border.
const CHROME: u32 = 40;

/// The card at its smallest and largest, in cells.
const MIN: (usize, usize) = (24, 10);
const MAX: (usize, usize) = (96, 40);

/// Lines the card spends on everything that is not a colour bar.
const FIXED_LINES: usize = 14;

/// Where the card is written.
pub fn path(dir: &Path) -> PathBuf {
    dir.join("testcard.ans")
}

/// How many cells of card a pane that size can hold.
pub fn geometry(pane: (u32, u32)) -> (usize, usize) {
    let cells = |px: u32, cell: u32| (px.saturating_sub(CHROME) / cell) as usize;
    (cells(pane.0, CELL.0).clamp(MIN.0, MAX.0), cells(pane.1, CELL.1).clamp(MIN.1, MAX.1))
}

/// One row of background colour, as a line.
fn band(cells: impl Iterator<Item = (u8, u8, u8)>) -> String {
    let mut out = String::new();
    for (r, g, b) in cells {
        out.push_str(&format!("\x1b[48;2;{r};{g};{b}m "));
    }
    out.push_str("\x1b[0m");
    out
}

/// A fully saturated colour at `hue` degrees.
fn hue(deg: f32) -> (u8, u8, u8) {
    let h = deg.rem_euclid(360.0) / 60.0;
    let x = 255.0 * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (255.0, x, 0.0),
        1 => (x, 255.0, 0.0),
        2 => (0.0, 255.0, x),
        3 => (0.0, x, 255.0),
        4 => (x, 0.0, 255.0),
        _ => (255.0, 0.0, x),
    };
    (r as u8, g as u8, b as u8)
}

/// One line of the card, and how readily it is given up.
///
/// A small pane cannot hold every band, and which lines go first is a
/// judgement: the blank lines separating the bands are worth less than the
/// bands, and a second row of a band is worth less than its first.
struct Line {
    text: String,
    /// Higher goes first when the card has to be cut down.
    spare: u8,
}

/// The longest of `choices` that fits, indented, in `w` columns.
fn pick(w: usize, choices: &[&str]) -> String {
    let room = w.saturating_sub(2);
    let chosen =
        choices.iter().find(|c| c.chars().count() <= room).unwrap_or(&choices[choices.len() - 1]);
    format!("  {}", chosen.chars().take(room).collect::<String>())
}

/// The whole card, as the bytes a terminal is fed.
pub fn render(cols: usize, rows: usize) -> String {
    // The broadcast bar order, which is the one that makes a hue shift
    // obvious: primaries and secondaries alternate, so a rotation moves every
    // bar onto a colour its neighbour just had.
    const BARS: [(u8, u8, u8); 8] = [
        (192, 192, 192),
        (192, 192, 0),
        (0, 192, 192),
        (0, 192, 0),
        (192, 0, 192),
        (192, 0, 0),
        (0, 0, 192),
        (16, 16, 16),
    ];

    let w = cols.max(MIN.0);
    let rows = rows.max(MIN.1);
    // Everything left over after the other bands goes to the colour bars, so
    // the card fills the window rather than leaving the bottom half black.
    let bar_rows = rows.saturating_sub(FIXED_LINES).max(1);

    let mut lines: Vec<Line> = Vec::new();
    let mut put = |text: String, spare: u8| lines.push(Line { text, spare });

    put(
        format!(
            "\x1b[1;97m{}\x1b[0m",
            pick(w, &["HyprWindowShade preview", "HyprWindowShade", "HWS"])
        ),
        0,
    );
    put(String::new(), 4);

    for _ in 0..bar_rows {
        put(band((0..w).map(|i| BARS[i * BARS.len() / w])), 0);
    }
    put(String::new(), 4);

    // A black-to-white ramp: where a shader lifts blacks, crushes whites or
    // bends the curve between them.
    for row in 0..2 {
        put(
            band((0..w).map(|i| {
                let v = (i * 255 / (w - 1)) as u8;
                (v, v, v)
            })),
            row,
        );
    }
    put(String::new(), 4);

    // Every hue at full saturation, for tints and desaturation.
    for row in 0..2 {
        put(band((0..w).map(|i| hue(i as f32 * 360.0 / w as f32))), row * 2);
    }
    put(String::new(), 4);

    // Half-block checkerboard: detail at half a cell, which is what a blur
    // softens, a wobble bends and a sharpen rings around.
    for row in 0..2 {
        let mut s = String::from("\x1b[97;40m");
        for i in 0..w {
            s.push(if (i + row) % 2 == 0 { '▀' } else { '▄' });
        }
        s.push_str("\x1b[0m");
        put(s, row as u8);
    }
    put(String::new(), 4);

    // Text, because most of a real window is text and legibility is what a
    // shader most often costs.
    put(pick(w, &["the quick brown fox jumps over the lazy dog", "the quick brown fox"]), 0);
    put(pick(w, &["0123456789  ABCDEFGHIJKLM  .,;:!?()[]{}", "0123456789  ABCDEF"]), 3);

    // Trim from the least valuable end until the card fits the window. Left
    // to scroll instead, the terminal would take the title off the top.
    while lines.len() > rows {
        let Some(worst) = lines.iter().map(|l| l.spare).max().filter(|s| *s > 0) else {
            break;
        };
        let at = lines.iter().rposition(|l| l.spare == worst).expect("just found one");
        lines.remove(at);
    }

    // No cursor: it sits under the last line for the whole hold, blinking or
    // not, and it is the one thing in the window that is not being previewed.
    let mut out = String::from("\x1b[?25l");
    for line in &lines {
        out.push_str(&line.text);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drop the escape sequences, leaving what is actually on screen.
    fn strip(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for c in chars.by_ref() {
                    if c == 'm' || c == 'l' || c == 'h' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn nothing_is_wider_than_the_window() {
        // One column too many and every band wraps, which is the one failure
        // that would make the card worse than the black window it replaced.
        for cols in [MIN.0, 40, 60, MAX.0] {
            for line in render(cols, 20).lines() {
                let visible = strip(line);
                assert!(
                    visible.chars().count() <= cols,
                    "{cols} columns: {visible:?} is {} wide",
                    visible.chars().count()
                );
            }
        }
    }

    #[test]
    fn the_card_fills_the_window_it_is_given() {
        // Within a line or two: the card grows by whole bands, and the point
        // is that a tall window is not left half black.
        for rows in [14, 20, 30] {
            let lines = render(60, rows).lines().count();
            assert!(lines <= rows, "{rows} rows: the card is {lines} lines and would scroll");
            assert!(lines >= rows - 1, "{rows} rows: the card is only {lines} lines");
        }
    }

    #[test]
    fn a_tiny_pane_still_gets_a_whole_card() {
        // Clamped rather than shrunk to nothing: the bands are the preview.
        let (cols, rows) = geometry((160, 120));
        assert_eq!((cols, rows), MIN);
        assert!(render(cols, rows).contains("quick brown fox"));
    }

    #[test]
    fn the_card_is_sized_to_the_pane() {
        let (cols, rows) = geometry((760, 480));
        assert_eq!((cols, rows), (60, 18));
        // A huge pane does not mean a card of a thousand columns.
        assert_eq!(geometry((4096, 2160)), MAX);
    }

    #[test]
    fn the_card_has_something_for_every_kind_of_shader() {
        let card = render(60, 20);
        assert!(card.contains("\x1b[48;2;192;192;0m"), "no colour bars");
        assert!(card.contains("\x1b[48;2;255;255;255m"), "the ramp never reaches white");
        assert!(card.contains("\x1b[48;2;0;0;0m"), "the ramp never reaches black");
        assert!(card.contains('▀'), "no fine detail");
        assert!(card.contains("quick brown fox"), "no text");
    }

    #[test]
    fn the_cursor_is_hidden() {
        assert!(render(60, 20).starts_with("\x1b[?25l"));
    }

    #[test]
    fn the_hue_sweep_goes_all_the_way_round() {
        assert_eq!(hue(0.0), (255, 0, 0));
        assert_eq!(hue(120.0), (0, 255, 0));
        assert_eq!(hue(240.0), (0, 0, 255));
        assert_eq!(hue(360.0), hue(0.0));
    }

    #[test]
    fn the_card_lives_beside_the_rest_of_the_previews_files() {
        assert_eq!(path(Path::new("/run/preview")), PathBuf::from("/run/preview/testcard.ans"));
    }
}
