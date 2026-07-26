use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use log::info;

pub const MAX_LINES: usize = 32;
pub const MAX_PAGES: usize = 8;

#[derive(Clone)]
pub struct PageState {
    pub lines: [String; MAX_LINES],
    pub brightness: u8,
    pub readtime: f32,
    pub blink_speed: u8, // 0=off, 1-4
    pub scroll: bool,
    pub fade: bool,
    pub move_speed: u8, // 1-20
    pub bold: bool,
}

impl Default for PageState {
    fn default() -> Self {
        const EMPTY: String = String::new();
        Self {
            lines: [EMPTY; MAX_LINES],
            brightness: 17,
            readtime: 5.0,
            blink_speed: 0,
            scroll: false,
            fade: false,
            move_speed: 1,
            bold: false,
        }
    }
}

impl PageState {
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.is_empty())
    }
}

/// Persistent display state backed by NVS flash.
pub struct DisplayState {
    nvs: EspNvs<NvsDefault>,
    pub pages: [PageState; MAX_PAGES],
}

impl DisplayState {
    pub fn new(nvs: EspNvs<NvsDefault>) -> Self {
        let mut s = Self {
            nvs,
            pages: Default::default(),
        };
        s.load_all();
        s
    }

    /// Update a page (1-based) and persist to flash.
    pub fn update_page(
        &mut self,
        page: usize,
        lines: [String; MAX_LINES],
        brightness: u8,
        readtime: f32,
        blink_speed: u8,
        scroll: bool,
        fade: bool,
        move_speed: u8,
        bold: bool,
    ) {
        let ps = &mut self.pages[page - 1];
        ps.lines = lines;
        ps.brightness = brightness;
        ps.readtime = readtime;
        ps.blink_speed = blink_speed;
        ps.scroll = scroll;
        ps.fade = fade;
        ps.move_speed = move_speed;
        ps.bold = bold;
        self.save_page(page);
    }

    /// Clear all pages and persist.
    pub fn reset(&mut self) {
        self.pages = Default::default();
        for pg in 1..=MAX_PAGES {
            self.save_page(pg);
        }
        info!("State reset");
    }

    /// Format: "brightness,readtime,blink,scroll,fade,speed,bold\nline1\nline2\n..."
    fn save_page(&mut self, page: usize) {
        let ps = &self.pages[page - 1];
        let mut data = format!(
            "{},{},{},{},{},{},{}\n",
            ps.brightness, ps.readtime, ps.blink_speed,
            ps.scroll as u8, ps.fade as u8, ps.move_speed, ps.bold as u8
        );
        for line in &ps.lines {
            data.push_str(line);
            data.push('\n');
        }
        let key = page_key(page);
        if let Err(e) = self.nvs.set_str(&key, &data) {
            info!("NVS save error for {}: {:?}", key, e);
        }
    }

    fn load_all(&mut self) {
        for pg in 1..=MAX_PAGES {
            self.load_page(pg);
        }
        let active = self.pages.iter().filter(|p| !p.is_empty()).count();
        info!("Loaded {} active page(s) from NVS", active);
    }

    fn load_page(&mut self, page: usize) {
        let key = page_key(page);
        let mut buf = [0u8; 2048];
        let data = match self.nvs.get_str(&key, &mut buf) {
            Ok(Some(s)) => s.to_string(),
            _ => return,
        };

        let mut lines_iter = data.split('\n');

        if let Some(header) = lines_iter.next() {
            let parts: Vec<&str> = header.split(',').collect();
            if parts.len() >= 3 {
                let ps = &mut self.pages[page - 1];
                ps.brightness = parts[0].parse().unwrap_or(17);
                ps.readtime = parts[1].parse().unwrap_or(5.0);
                ps.blink_speed = parts[2].parse().unwrap_or(0);
                ps.scroll = parts.get(3).map(|v| *v == "1").unwrap_or(false);
                ps.fade = parts.get(4).map(|v| *v == "1").unwrap_or(false);
                ps.move_speed = parts.get(5).and_then(|v| v.parse().ok()).unwrap_or(1);
                ps.bold = parts.get(6).map(|v| *v == "1").unwrap_or(false);

                for (i, line) in lines_iter.enumerate() {
                    if i >= MAX_LINES { break; }
                    ps.lines[i] = line.to_string();
                }
            }
        }
    }
}

fn page_key(page: usize) -> String {
    format!("pg{}", page)
}
