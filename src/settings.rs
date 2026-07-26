use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use log::info;

#[derive(Clone)]
pub struct Settings {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub connect_timeout: u32,
    pub ap_ssid: String,
    pub ap_pass: String,
    pub controllers: u8,
    pub lines_per_ctrl: u8,
    pub chars_per_line: usize,
    pub visible_lines: usize,
    pub max_pages: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            wifi_ssid: String::new(),
            wifi_pass: String::new(),
            connect_timeout: 30,
            ap_ssid: "DatalitePanel".to_string(),
            ap_pass: "datalite123".to_string(),
            controllers: 3,
            lines_per_ctrl: 8,
            chars_per_line: 32,
            visible_lines: 20,
            max_pages: 4,
        }
    }
}

impl Settings {
    pub fn load(nvs: &EspNvs<NvsDefault>) -> Self {
        let mut buf = [0u8; 512];
        let data = match nvs.get_str("cfg", &mut buf) {
            Ok(Some(s)) => s.to_string(),
            _ => return Self::default(),
        };
        let p: Vec<&str> = data.split('\n').collect();
        if p.len() < 10 { return Self::default(); }
        Self {
            wifi_ssid: p[0].to_string(),
            wifi_pass: p[1].to_string(),
            connect_timeout: p[2].parse().unwrap_or(30),
            ap_ssid: if p[3].is_empty() { "DatalitePanel".to_string() } else { p[3].to_string() },
            ap_pass: if p[4].is_empty() { "datalite123".to_string() } else { p[4].to_string() },
            controllers: p[5].parse().unwrap_or(3).max(1).min(32),
            lines_per_ctrl: p[6].parse().unwrap_or(8).max(1).min(8),
            chars_per_line: p[7].parse().unwrap_or(32).max(1).min(64),
            visible_lines: p[8].parse().unwrap_or(20).max(1).min(32),
            max_pages: p[9].parse().unwrap_or(4).max(1).min(8),
        }
    }

    pub fn save(&self, nvs: &mut EspNvs<NvsDefault>) {
        let data = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            self.wifi_ssid, self.wifi_pass, self.connect_timeout,
            self.ap_ssid, self.ap_pass,
            self.controllers, self.lines_per_ctrl, self.chars_per_line,
            self.visible_lines, self.max_pages,
        );
        if let Err(e) = nvs.set_str("cfg", &data) {
            info!("Settings save error: {:?}", e);
        } else {
            info!("Settings saved");
        }
    }
}
