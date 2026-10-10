//! The box drawing behind act's `--graph` output.
//!
//! A CI build computer never renders this — the DAG is written to a file for
//! GraphViz — but it is part of the upstream surface, and the rendering rules
//! have two details worth naming because they are visible in the output:
//!
//! * **Widths are measured in bytes, not characters.** `len(l)` in Go counts
//!   bytes, so a job named with an umlaut gets a bar two characters wider than
//!   its label and the box comes out skewed. Preserved.
//! * **`CLICOLOR=0` turns the colour off**, leaving the box characters. This is
//!   the `NO_COLOR`-style escape hatch a CI log needs, since an escape sequence
//!   in a log file is noise.
//!
//! Upstream ships no tests for this file, so the tests here are the port's
//! own: they pin the byte-width rule and the colour rule, which are the two
//! things that can go wrong.

use std::fmt::Write as _;

/// Which box characters to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// `╔═╗` — the heaviest lines.
    DoubleLine,
    /// `╭─╮` — rounded corners.
    SingleLine,
    /// `┌─┐` — the usual terminal box.
    DashedLine,
    /// No lines at all; the labels are still printed.
    NoLine,
}

/// The six glyphs a style is made of: the four corners and the two lines.
struct StyleDef {
    corner_tl: &'static str,
    corner_tr: &'static str,
    corner_bl: &'static str,
    corner_br: &'static str,
    line_h: &'static str,
    line_v: &'static str,
}

const STYLE_DEFS: [StyleDef; 4] = [
    StyleDef {
        corner_tl: "\u{2554}",
        corner_tr: "\u{2557}",
        corner_bl: "\u{255a}",
        corner_br: "\u{255d}",
        line_h: "\u{2550}",
        line_v: "\u{2551}",
    },
    StyleDef {
        corner_tl: "\u{256d}",
        corner_tr: "\u{256e}",
        corner_bl: "\u{2570}",
        corner_br: "\u{256f}",
        line_h: "\u{2500}",
        line_v: "\u{2502}",
    },
    StyleDef {
        corner_tl: "\u{250c}",
        corner_tr: "\u{2510}",
        corner_bl: "\u{2514}",
        corner_br: "\u{2518}",
        line_h: "\u{254c}",
        line_v: "\u{254e}",
    },
    StyleDef {
        corner_tl: " ",
        corner_tr: " ",
        corner_bl: " ",
        corner_br: " ",
        line_h: " ",
        line_v: " ",
    },
];

/// A drawing, ready to be written out at a given width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawing {
    body: String,
    width: usize,
}

impl Drawing {
    /// Writes the drawing, centred in `center_on_width`.
    ///
    /// Blank lines are skipped, which is what removes the trailing newline's
    /// empty remainder.
    pub fn draw(&self, out: &mut String, center_on_width: usize) {
        let pad = center_on_width.saturating_sub(self.width) / 2;
        for line in self.body.split('\n') {
            if !line.is_empty() {
                let _ = writeln!(out, "{}{line}", " ".repeat(pad));
            }
        }
    }

    /// The drawing's own width, before centring.
    pub fn width(&self) -> usize {
        self.width
    }

    /// The drawing without any centring, escape sequences included.
    pub fn body(&self) -> &str {
        &self.body
    }
}

/// Draws with one style and colour.
#[derive(Debug, Clone, Copy)]
pub struct Pen {
    style: Style,
    color: u16,
    bgcolor: u16,
}

/// The colour and background a pen writes, given the value of `CLICOLOR`.
///
/// Split out from [`Pen::new`] so the rule can be tested as a **value** rather
/// than by mutating the process environment. That is not a style preference:
/// `CLICOLOR` is global, libtest runs tests in parallel, and a test that sets
/// it races every other test that reads it. The race showed up as
/// `boxes_are_drawn_with_colour_by_default` failing only in a full run — it
/// happened to draw while the other test had the variable set.
///
/// `None` and `"0"` both mean "off"; upstream's comparison is against the exact
/// bytes `0`, so `CLICOLOR=00` and `CLICOLOR=false` leave colour **on**, which
/// is counter-intuitive and therefore worth stating.
fn colour_pair(clicolor: Option<&std::ffi::OsStr>, color: u16) -> (u16, u16) {
    if clicolor == Some(std::ffi::OsStr::new("0")) {
        (0, 0)
    } else {
        (color, 49)
    }
}

impl Pen {
    /// A pen for `style`. `CLICOLOR=0` disables colour, as upstream does.
    pub fn new(style: Style, color: u16) -> Self {
        let (color, bgcolor) = colour_pair(std::env::var_os("CLICOLOR").as_deref(), color);
        Pen {
            style,
            color,
            bgcolor,
        }
    }

    /// The arrow that joins two boxes.
    pub fn draw_arrow(&self) -> Drawing {
        let mut body = String::new();
        let _ = write!(body, "\x1b[{}m\u{2b07}\x1b[0m", self.color);
        Drawing { body, width: 1 }
    }

    /// A row of labelled boxes.
    pub fn draw_boxes(&self, labels: &[&str]) -> Drawing {
        // Byte lengths, as Go's `len` gives them. See the module docs.
        let width: usize = labels
            .iter()
            .map(|label| label.len() + 2 + 2 + 1)
            .sum();
        let style = &STYLE_DEFS[self.style as usize];

        let mut body = String::new();
        for row in [Bar::Top, Bar::Labels, Bar::Bottom] {
            for label in labels {
                // Leading space, then the colour, then the box, then reset.
                let _ = write!(body, " \x1b[{};{}m", self.color, self.bgcolor);
                match row {
                    Bar::Top => {
                        let bar = style.line_h.repeat(label.len() + 2);
                        let _ = write!(body, "{}{}{}", style.corner_tl, bar, style.corner_tr);
                    }
                    Bar::Labels => {
                        let _ = write!(
                            body,
                            "{} {} {}",
                            style.line_v, label, style.line_v
                        );
                    }
                    Bar::Bottom => {
                        let bar = style.line_h.repeat(label.len() + 2);
                        let _ = write!(body, "{}{}{}", style.corner_bl, bar, style.corner_br);
                    }
                }
                let _ = write!(body, "\x1b[0m");
            }
            body.push('\n');
        }

        Drawing { body, width }
    }
}

enum Bar {
    Top,
    Labels,
    Bottom,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Colour is on by default, so the escape sequences are present.
    ///
    /// No environment skip. The first version of this test returned early when
    /// `CLICOLOR=0` was set — a green test that asserted nothing — and did not
    /// stop the parallel race that made it fail in a full run anyway. The
    /// environment is now read through [`colour_pair`], and the rule itself is
    /// asserted as a value in `clicolor_only_the_exact_zero_disables_colour`.
    #[test]
    fn boxes_are_drawn_with_colour_by_default() {
        let pen = Pen {
            style: Style::DashedLine,
            color: 33,
            bgcolor: 49,
        };
        let drawing = pen.draw_boxes(&["build", "test"]);
        let body = drawing.body();
        assert!(body.contains("\x1b[33;49m"), "{body:?}");
        assert!(body.contains("\x1b[0m"), "{body:?}");
        assert!(body.contains("\u{250c}"), "{body:?}");
        // Three rows: top, labels, bottom.
        assert_eq!(body.matches('\n').count(), 3);
    }

    /// `CLICOLOR=0` resets the colour *values*; the escape sequences are
    /// still written, with `0;0` in them. That is upstream's behaviour — a
    /// log file gets the escapes either way — and it is worth pinning, because
    /// "disable colour" reading as "emit nothing" is the natural assumption
    /// and it is wrong.
    ///
    /// The pen is built with explicit fields instead of through `Pen::new`, so
    /// nothing here mutates the process environment and nothing here can race
    /// the test above.
    #[test]
    fn clicolor_zero_resets_the_colour_but_keeps_the_escapes() {
        let (color, bgcolor) = colour_pair(Some(std::ffi::OsStr::new("0")), 33);
        let body = Pen {
            style: Style::DashedLine,
            color,
            bgcolor,
        }
        .draw_boxes(&["build"])
        .body()
        .to_string();
        assert!(body.contains("\x1b[0;0m"), "{body:?}");
        assert!(!body.contains("\x1b[33;49m"), "{body:?}");
    }

    /// Only the exact bytes `0` turn colour off, and an unset variable leaves
    /// it on.
    ///
    /// `CLICOLOR=00` and `CLICOLOR=false` keep colour **on**, because upstream
    /// compares the value to `0` rather than parsing a boolean. Every
    /// `NO_COLOR`-style switch elsewhere in the world accepts more spellings
    /// than this one, so the narrower behaviour is the thing to pin.
    #[test]
    fn clicolor_only_the_exact_zero_disables_colour() {
        let off = Some(std::ffi::OsStr::new("0"));
        let cases: &[(Option<&std::ffi::OsStr>, (u16, u16))] = &[
            (None, (33, 49)),
            (off, (0, 0)),
            (Some(std::ffi::OsStr::new("00")), (33, 49)),
            (Some(std::ffi::OsStr::new("false")), (33, 49)),
            (Some(std::ffi::OsStr::new("")), (33, 49)),
            (Some(std::ffi::OsStr::new("1")), (33, 49)),
        ];
        for (value, want) in cases {
            assert_eq!(colour_pair(*value, 33), *want, "CLICOLOR={value:?}");
        }
    }

    /// Width is `sum(len(label) + 5)`, and the drawing centres inside whatever
    /// width the caller gives.
    #[test]
    fn boxes_are_centred_in_the_given_width() {
        let pen = Pen::new(Style::NoLine, 0);
        let drawing = pen.draw_boxes(&["ab", "cde"]);
        assert_eq!(drawing.width(), (2 + 5) + (3 + 5));

        let mut out = String::new();
        drawing.draw(&mut out, 40);
        let first = out.lines().next().expect("a line");
        // Upstream writes one space before each box, on top of the centring
        // padding, so the observed indent is one more than the padding.
        assert_eq!(
            first.len() - first.trim_start().len(),
            (40 - drawing.width()) / 2 + 1,
        );

        // A width narrower than the drawing pads by zero rather than
        // underflowing, which is what act's `if padSize < 0` guards. The one
        // space each box carries is still written.
        let mut narrow = String::new();
        drawing.draw(&mut narrow, 3);
        let first = narrow.lines().next().expect("a line");
        assert_eq!(first.len() - first.trim_start().len(), 1);
    }

    /// The byte-width rule: a two-byte label gets a bar two characters wider
    /// than its text, so the box is skewed. Upstream measures with `len`, and
    /// this is the visible consequence.
    #[test]
    fn widths_are_measured_in_bytes() {
        let pen = Pen::new(Style::DashedLine, 0);
        let umlaut = Pen::new(Style::DashedLine, 0).draw_boxes(&["pr\u{fc}fen"]);
        let ascii = pen.draw_boxes(&["pruefen"]);
        // "prüfen" is seven bytes, "pruefen" is seven bytes — the same, which
        // is exactly why the rule is invisible here.
        assert_eq!(umlaut.width(), ascii.width());
        // A character outside Latin-1 diverges: four bytes for one glyph.
        let emoji = pen.draw_boxes(&["\u{1f600}"]);
        assert_eq!(emoji.width(), 4 + 5);
    }

    #[test]
    fn an_arrow_is_a_single_glyph() {
        let drawing = Pen::new(Style::NoLine, 31).draw_arrow();
        assert_eq!(drawing.width(), 1);
        assert!(drawing.body().contains('\u{2b07}'));
    }

    /// Blank lines are skipped, so a drawing never ends in an empty line.
    #[test]
    fn blank_lines_are_skipped_when_writing() {
        let drawing = Pen::new(Style::NoLine, 0).draw_boxes(&["a"]);
        let mut out = String::new();
        drawing.draw(&mut out, 20);
        assert!(!out.contains("\n\n"), "{out:?}");
        assert_eq!(out.lines().count(), 3);
    }
}
