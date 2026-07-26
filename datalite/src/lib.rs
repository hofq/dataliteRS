//! Datalite DX-3200 LED display controller SDK.
//!
//! Abstracts away controller addresses and protocol details.
//! You configure a [`Display`] with your physical layout, then write
//! to pages using absolute line numbers. The SDK routes lines to the
//! correct controllers automatically.
//!
//! # Example
//! ```no_run
//! use datalite::Display;
//!
//! // 2 controllers, 8 lines each, 48 chars per line
//! let display = Display::new(2, 8, 48);
//! let mut serial = std::fs::File::create("/dev/ttyUSB0").unwrap();
//!
//! // Send multiple pages at once (required to avoid overwriting)
//! display.commit_pages(
//!     &mut serial,
//!     &[
//!         display.page(1)
//!             .line(1, "Page 1 line 1")
//!             .brightness(17)
//!             .readtime_secs(5.0),
//!         display.page(2)
//!             .line(1, "Page 2 line 1")
//!             .readtime_secs(3.0),
//!     ],
//! ).unwrap();
//! ```

use std::io::{self, Write};

// Protocol constants
const SOH: u8 = 0x01;
const CR: u8 = 0x0D;
const SYN: u8 = 0x16;
const ESC: u8 = 0x1B;
const FS: u8 = 0x1C;
const GS: u8 = 0x1D;

fn enc(value: u8) -> u8 {
    value + 32
}

/// Physical display configuration.
#[derive(Clone)]
pub struct Display {
    controllers: u8,
    lines_per_controller: u8,
    chars_per_line: usize,
}

impl Display {
    /// Create a display configuration.
    ///
    /// - `controllers`: number of controllers on the bus (1–32)
    /// - `lines_per_controller`: lines each controller drives (typically 8)
    /// - `chars_per_line`: max characters per line (typically 48)
    pub fn new(controllers: u8, lines_per_controller: u8, chars_per_line: usize) -> Self {
        assert!(controllers >= 1 && controllers <= 32);
        assert!(lines_per_controller >= 1 && lines_per_controller <= 8);
        Self {
            controllers,
            lines_per_controller,
            chars_per_line,
        }
    }

    /// Total number of lines across all controllers.
    pub fn total_lines(&self) -> u16 {
        self.controllers as u16 * self.lines_per_controller as u16
    }

    /// Start building a page. Page numbers start at 1.
    pub fn page(&self, page: u8) -> Page {
        assert!(page >= 1, "page must be >= 1");
        Page {
            display: self.clone(),
            page,
            lines: Vec::new(),
            attrs: PageAttrs::default(),
        }
    }

    /// Build the wire bytes for multiple pages sent together.
    /// Pages are grouped per controller so the controller receives all its
    /// pages in one command — this avoids overwriting earlier pages.
    pub fn build_pages(&self, pages: &[Page]) -> Vec<u8> {
        let multi = self.controllers > 1;
        let lpc = self.lines_per_controller as u16;
        let max_page = pages.iter().map(|p| p.page).max().unwrap_or(1);
        let mut buf = Vec::new();

        for addr in 0..self.controllers {
            let range_start = addr as u16 * lpc + 1;

            buf.push(SOH);
            buf.push(enc(addr));
            buf.push(FS);

            // Emit pages 1..max_page in order
            for pg in 1..=max_page {
                let page_data = pages.iter().find(|p| p.page == pg);

                // Emit ALL lines for this controller, filling in content where set
                for local_line in 0..self.lines_per_controller {
                    let abs = range_start + local_line as u16;
                    buf.push(b'0' + local_line);
                    if let Some(p) = page_data {
                        if let Some(entry) = p.lines.iter().find(|l| l.abs_line == abs) {
                            buf.extend_from_slice(&entry.text);
                        }
                    }
                    buf.push(FS);
                }

                // Page attributes
                if let Some(p) = page_data {
                    p.write_attrs(&mut buf);
                }
            }

            if multi {
                buf.push(SYN);
            }
        }

        buf.push(CR);
        buf
    }

    /// Build and send multiple pages at once.
    pub fn commit_pages<W: Write>(&self, writer: &mut W, pages: &[Page]) -> io::Result<()> {
        let data = self.build_pages(pages);
        writer.write_all(&data)?;
        writer.flush()
    }

    /// Send a blank page to all controllers, clearing all content.
    pub fn clear<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let data = self.build_clear();
        writer.write_all(&data)?;
        writer.flush()
    }

    /// Build wire bytes that clear all controllers (blank page 1).
    pub fn build_clear(&self) -> Vec<u8> {
        let multi = self.controllers > 1;
        let mut buf = Vec::new();
        for addr in 0..self.controllers {
            buf.push(SOH);
            buf.push(enc(addr));
            buf.push(FS);
            for local_line in 0..self.lines_per_controller {
                buf.push(b'0' + local_line);
                buf.push(FS);
            }
            if multi {
                buf.push(SYN);
            }
        }
        buf.push(CR);
        buf
    }

    /// Send a display mode command to all controllers.
    pub fn set_display_mode<W: Write>(&self, writer: &mut W, mode: DisplayMode) -> io::Result<()> {
        for addr in 0..self.controllers {
            let buf = [SOH, enc(addr), FS, ESC, b'D', enc(mode as u8), FS, CR];
            writer.write_all(&buf)?;
        }
        writer.flush()
    }

    /// Set the realtime clock on all controllers.
    pub fn set_clock<W: Write>(
        &self,
        writer: &mut W,
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> io::Result<()> {
        let y = if year >= 1980 { (year - 1980) as u8 } else { year as u8 };
        for addr in 0..self.controllers {
            let buf = [
                SOH, enc(addr), FS,
                ESC, b'T', enc(y), enc(month), enc(0), enc(day),
                enc(hour), enc(minute), enc(second), FS, CR,
            ];
            writer.write_all(&buf)?;
        }
        writer.flush()
    }
}

#[derive(Clone, Copy)]
pub enum DisplayMode {
    Off = 0,
    On = 1,
    TestOff = 2,
    TestHalt = 3,
    TestOn = 4,
    SyncToggle = 10,
}

#[derive(Default, Clone)]
struct PageAttrs {
    blink_speed: Option<u8>,
    readtime: Option<u16>,
    scheduler: Option<(u8, u8, u8, u8, u8, u8)>,
    brightness: Option<u8>,
    scroll_effect: Option<bool>,
    fading_effect: Option<bool>,
    moving_speed: Option<u8>,
    text_width: Option<bool>,
    text_activity: Option<bool>,
}

#[derive(Clone)]
struct LineEntry {
    abs_line: u16,
    text: Vec<u8>,
}

/// A page builder. Queue lines and attributes, then commit.
#[derive(Clone)]
pub struct Page {
    display: Display,
    page: u8,
    lines: Vec<LineEntry>,
    attrs: PageAttrs,
}

impl Page {
    pub fn page_number(&self) -> u8 {
        self.page
    }

    /// Set text on an absolute line number (1-based across all controllers).
    pub fn line(mut self, line: u16, text: &str) -> Self {
        assert!(
            line >= 1 && line <= self.display.total_lines(),
            "line {} out of range (1-{})",
            line,
            self.display.total_lines()
        );
        let truncated: Vec<u8> = text.as_bytes()
            .iter()
            .copied()
            .take(self.display.chars_per_line)
            .collect();
        self.lines.push(LineEntry {
            abs_line: line,
            text: truncated,
        });
        self
    }

    /// Set text with inline blink markers (`[blink]...[/blink]`).
    pub fn line_styled(mut self, line: u16, text: &str) -> Self {
        assert!(
            line >= 1 && line <= self.display.total_lines(),
            "line {} out of range (1-{})",
            line,
            self.display.total_lines()
        );
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(start) = rest.find("[blink]") {
            out.extend_from_slice(rest[..start].as_bytes());
            rest = &rest[start + 7..];
            out.push(GS);
            if let Some(end) = rest.find("[/blink]") {
                out.extend_from_slice(rest[..end].as_bytes());
                rest = &rest[end + 8..];
                out.push(GS);
            }
        }
        out.extend_from_slice(rest.as_bytes());
        out.truncate(self.display.chars_per_line);
        self.lines.push(LineEntry {
            abs_line: line,
            text: out,
        });
        self
    }

    pub fn blink_speed(mut self, speed: u8) -> Self {
        assert!((1..=4).contains(&speed));
        self.attrs.blink_speed = Some(speed);
        self
    }

    pub fn readtime(mut self, value: u16) -> Self {
        assert!(value <= 12800);
        self.attrs.readtime = Some(value);
        self
    }

    pub fn readtime_secs(self, seconds: f32) -> Self {
        let units = (seconds / 0.0267).round() as u16;
        self.readtime(units)
    }

    pub fn scheduler(mut self, year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        let y = if year >= 1980 { (year - 1980) as u8 } else { year as u8 };
        self.attrs.scheduler = Some((y, month, day, hour, minute, second));
        self
    }

    pub fn brightness(mut self, value: u8) -> Self {
        assert!((1..=17).contains(&value));
        self.attrs.brightness = Some(value);
        self
    }

    pub fn scroll_effect(mut self, enabled: bool) -> Self {
        self.attrs.scroll_effect = Some(enabled);
        self
    }

    pub fn fading_effect(mut self, enabled: bool) -> Self {
        self.attrs.fading_effect = Some(enabled);
        self
    }

    pub fn moving_speed(mut self, speed: u8) -> Self {
        assert!((1..=20).contains(&speed));
        self.attrs.moving_speed = Some(speed);
        self
    }

    pub fn bold(mut self, enabled: bool) -> Self {
        self.attrs.text_width = Some(enabled);
        self
    }

    pub fn steady(mut self, enabled: bool) -> Self {
        self.attrs.text_activity = Some(enabled);
        self
    }

    /// Send this single page (shorthand when only using one page).
    pub fn commit<W: Write>(self, writer: &mut W) -> io::Result<()> {
        let data = self.display.build_pages(&[self.clone()]);
        writer.write_all(&data)?;
        writer.flush()
    }

    /// Return the raw bytes for this single page.
    pub fn to_bytes(self) -> Vec<u8> {
        self.display.build_pages(&[self.clone()])
    }

    fn write_attrs(&self, buf: &mut Vec<u8>) {
        if let Some(v) = self.attrs.blink_speed {
            buf.extend_from_slice(&[ESC, b'B', enc(v), FS]);
        }
        if let Some(v) = self.attrs.readtime {
            let b0 = (v / 4096) as u8;
            let rem = v % 4096;
            let b1 = (rem / 256) as u8;
            let rem = rem % 256;
            let b2 = (rem / 16) as u8;
            let b3 = (rem % 16) as u8;
            buf.extend_from_slice(&[ESC, b'A', enc(b0), enc(b1), enc(b2), enc(b3), FS]);
        }
        if let Some((y, m, d, h, min, s)) = self.attrs.scheduler {
            buf.extend_from_slice(&[
                ESC, b'P', enc(y), enc(m), enc(0), enc(d), enc(h), enc(min), enc(s), FS,
            ]);
        }
        if let Some(v) = self.attrs.brightness {
            buf.extend_from_slice(&[ESC, b'Q', enc(v), FS]);
        }
        if let Some(v) = self.attrs.scroll_effect {
            buf.extend_from_slice(&[ESC, b'R', enc(v as u8), FS]);
        }
        if let Some(v) = self.attrs.fading_effect {
            buf.extend_from_slice(&[ESC, b'S', enc(v as u8), FS]);
        }
        if let Some(v) = self.attrs.moving_speed {
            buf.extend_from_slice(&[ESC, b'F', enc(v), FS]);
        }
        if let Some(v) = self.attrs.text_width {
            buf.extend_from_slice(&[ESC, b'K', enc(v as u8), FS]);
        }
        if let Some(v) = self.attrs.text_activity {
            buf.extend_from_slice(&[ESC, b'S', enc(v as u8), FS]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single() -> Display { Display::new(1, 8, 48) }
    fn dual() -> Display { Display::new(2, 8, 48) }

    #[test]
    fn test_single_page() {
        let d = single();
        let bytes = d.page(1).line(1, "hello").to_bytes();
        // Line 0 has "hello", lines 1-7 are empty
        assert_eq!(bytes[0], SOH);
        assert_eq!(bytes[1], enc(0));
        assert_eq!(bytes[2], FS);
        assert_eq!(bytes[3], b'0');
        assert_eq!(&bytes[4..9], b"hello");
        assert_eq!(bytes[9], FS);
        // 7 empty lines follow
        for i in 0..7 {
            assert_eq!(bytes[10 + i * 2], b'1' + i as u8);
            assert_eq!(bytes[11 + i * 2], FS);
        }
        assert_eq!(*bytes.last().unwrap(), CR);
    }

    #[test]
    fn test_two_pages_independent() {
        let d = single();
        let pages = [
            d.page(1).line(1, "page1"),
            d.page(2).line(1, "page2"),
        ];
        let bytes = d.build_pages(&pages);

        // Should contain both "page1" and "page2"
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("page1"));
        assert!(s.contains("page2"));

        // Only one SOH (single controller)
        assert_eq!(bytes.iter().filter(|&&b| b == SOH).count(), 1);
    }

    #[test]
    fn test_two_pages_dont_overwrite() {
        let d = single();
        let pages = [
            d.page(1).line(1, "KEEP THIS"),
            d.page(2).line(1, "page2"),
        ];
        let bytes = d.build_pages(&pages);
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("KEEP THIS"));
        assert!(s.contains("page2"));
    }

    #[test]
    fn test_dual_controller_multi_page() {
        let d = dual();
        let pages = [
            d.page(1).line(1, "c0p1").line(9, "c1p1"),
            d.page(2).line(1, "c0p2").line(9, "c1p2"),
        ];
        let bytes = d.build_pages(&pages);

        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("c0p1"));
        assert!(s.contains("c0p2"));
        assert!(s.contains("c1p1"));
        assert!(s.contains("c1p2"));

        // Two controllers
        assert_eq!(bytes.iter().filter(|&&b| b == SOH).count(), 2);
        // Two SYN (multi-controller)
        assert_eq!(bytes.iter().filter(|&&b| b == SYN).count(), 2);
        // One CR
        assert_eq!(bytes.iter().filter(|&&b| b == CR).count(), 1);
    }

    #[test]
    fn test_line_truncation() {
        let d = Display::new(1, 8, 5);
        let bytes = d.page(1).line(1, "hello world").to_bytes();
        // First line truncated to 5 chars
        assert_eq!(bytes[3], b'0');
        assert_eq!(&bytes[4..9], b"hello");
        assert_eq!(bytes[9], FS);
    }

    #[test]
    fn test_brightness() {
        let d = single();
        let bytes = d.page(1).brightness(17).to_bytes();
        let pos = bytes.windows(2).position(|w| w == [ESC, b'Q']).unwrap();
        assert_eq!(bytes[pos + 2], enc(17));
    }

    #[test]
    fn test_display_mode() {
        let mut buf = Vec::new();
        dual().set_display_mode(&mut buf, DisplayMode::On).unwrap();
        assert_eq!(buf.iter().filter(|&&b| b == SOH).count(), 2);
    }

    #[test]
    fn test_clock() {
        let mut buf = Vec::new();
        single().set_clock(&mut buf, 2026, 7, 20, 14, 30, 0).unwrap();
        let pos = buf.windows(2).position(|w| w == [ESC, b'T']).unwrap();
        assert_eq!(buf[pos + 2], enc(46)); // 2026-1980
    }

    #[test]
    fn test_total_lines() {
        assert_eq!(single().total_lines(), 8);
        assert_eq!(dual().total_lines(), 16);
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn test_line_out_of_range() {
        single().page(1).line(9, "nope");
    }

    #[test]
    fn test_blink_markers() {
        let d = single();
        let bytes = d.page(1).line_styled(1, "ok [blink]WARN[/blink] ok").to_bytes();
        assert_eq!(bytes.iter().filter(|&&b| b == GS).count(), 2);
    }
}
