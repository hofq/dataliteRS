use std::sync::{Arc, Mutex};

use esp_idf_svc::hal::uart::UartDriver;
use esp_idf_svc::http::server::EspHttpServer;
use log::info;

use datalite::Display;
use crate::settings::Settings;
use crate::state::{DisplayState, MAX_LINES};

// ── JSON serialization helpers (no serde to save flash space) ──

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Serialize full display state (all pages + config) for the UI to hydrate from.
fn state_to_json(state: &DisplayState, cfg: &Settings) -> String {
    let mut out = format!(
        r#"{{"config":{{"rows":{},"cols":{},"pages":{}}},"pages":["#,
        cfg.visible_lines, cfg.chars_per_line, cfg.max_pages
    );
    for pi in 0..cfg.max_pages {
        if pi > 0 { out.push(','); }
        let ps = &state.pages[pi];
        out.push_str(&format!(
            r#"{{"brightness":{},"readtime":{},"blink_speed":{},"scroll":{},"fade":{},"move_speed":{},"bold":{},"lines":["#,
            ps.brightness, ps.readtime, ps.blink_speed, ps.scroll, ps.fade, ps.move_speed, ps.bold
        ));
        for li in 0..cfg.visible_lines {
            if li > 0 { out.push(','); }
            out.push('"');
            out.push_str(&json_escape(&ps.lines[li]));
            out.push('"');
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

fn page_to_json(state: &DisplayState, page: usize, cfg: &Settings) -> String {
    let ps = &state.pages[page - 1];
    let mut j = format!(
        r#"{{"brightness":{},"readtime":{},"blink_speed":{},"scroll":{},"fade":{},"move_speed":{},"bold":{},"lines":["#,
        ps.brightness, ps.readtime, ps.blink_speed, ps.scroll, ps.fade, ps.move_speed, ps.bold
    );
    for li in 0..cfg.visible_lines {
        if li > 0 { j.push(','); }
        j.push('"');
        j.push_str(&json_escape(&ps.lines[li]));
        j.push('"');
    }
    j.push_str("]}");
    j
}

fn settings_to_json(cfg: &Settings) -> String {
    format!(
        r#"{{"wifi_ssid":"{}","wifi_pass":"{}","connect_timeout":{},"ap_ssid":"{}","ap_pass":"{}","controllers":{},"lines_per_ctrl":{},"chars_per_line":{},"visible_lines":{},"max_pages":{}}}"#,
        json_escape(&cfg.wifi_ssid), json_escape(&cfg.wifi_pass), cfg.connect_timeout,
        json_escape(&cfg.ap_ssid), json_escape(&cfg.ap_pass),
        cfg.controllers, cfg.lines_per_ctrl, cfg.chars_per_line,
        cfg.visible_lines, cfg.max_pages
    )
}

// ── Minimal JSON request parser (no serde, keeps binary small) ──

/// Read the full request body into a String (max 4 KB).
fn read_body(req: &mut impl esp_idf_svc::io::Read) -> String {
    let mut buf = vec![0u8; 4096];
    let mut total = 0;
    loop {
        match req.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf[..total]).into_owned()
}

/// Extract a JSON value by key (string or number). Flat objects only.
fn json_str(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let pos = body.find(&needle)? + needle.len();
    let rest = body[pos..].trim_start();
    if rest.starts_with('"') {
        let inner = &rest[1..];
        let end = inner.find('"')?;
        Some(inner[..end].replace("\\\"", "\"").replace("\\\\", "\\"))
    } else {
        let end = rest.find(|c: char| c == ',' || c == '}' || c == ']')?;
        Some(rest[..end].trim().to_string())
    }
}

fn json_bool(body: &str, key: &str) -> bool {
    json_str(body, key).map(|v| v == "true").unwrap_or(false)
}

/// Parse the "lines" JSON array from the request body into a fixed-size array.
fn json_lines(body: &str, max: usize) -> [String; MAX_LINES] {
    const EMPTY: String = String::new();
    let mut lines = [EMPTY; MAX_LINES];
    if let Some(start) = body.find("\"lines\":[") {
        let arr = &body[start + 9..];
        let mut idx = 0;
        let mut pos = 0;
        while idx < max && idx < MAX_LINES {
            while pos < arr.len() && arr.as_bytes()[pos].is_ascii_whitespace() { pos += 1; }
            if pos >= arr.len() || arr.as_bytes()[pos] == b']' { break; }
            if arr.as_bytes()[pos] == b',' { pos += 1; continue; }
            if arr.as_bytes()[pos] == b'"' {
                pos += 1;
                let mut val = String::new();
                while pos < arr.len() && arr.as_bytes()[pos] != b'"' {
                    if arr.as_bytes()[pos] == b'\\' && pos + 1 < arr.len() {
                        pos += 1;
                        val.push(arr.as_bytes()[pos] as char);
                    } else {
                        val.push(arr.as_bytes()[pos] as char);
                    }
                    pos += 1;
                }
                pos += 1;
                lines[idx] = val;
                idx += 1;
            } else { break; }
        }
    }
    lines
}

/// Build the protocol payload from current state and write it out over UART.
/// Sends all non-empty pages in a single commit so earlier pages aren't overwritten.
fn send_all(state: &DisplayState, uart: &UartDriver, cfg: &Settings) {
    let display = Display::new(cfg.controllers, cfg.lines_per_ctrl, cfg.chars_per_line);

    // Find the highest non-empty page; only pages up to this index are sent
    let max_page = (0..cfg.max_pages).rev()
        .find(|&i| state.pages[i].lines[..cfg.visible_lines].iter().any(|l| !l.is_empty()))
        .map(|i| i + 1)
        .unwrap_or(0);

    if max_page == 0 {
        let bytes = display.build_clear();
        let _ = uart.write(&bytes);
        info!("Sent blank/clear");
        return;
    }

    let mut pages = Vec::new();
    for pg_idx in 0..max_page {
        let ps = &state.pages[pg_idx];
        let mut page = display.page((pg_idx + 1) as u8);
        let mut has_blink = false;
        for i in 0..cfg.visible_lines {
            let text = &ps.lines[i];
            if !text.is_empty() {
                if text.contains("[blink]") {
                    page = page.line_styled((i + 1) as u16, text);
                    has_blink = true;
                } else {
                    page = page.line((i + 1) as u16, text);
                }
            }
        }
        page = page.brightness(ps.brightness).readtime_secs(ps.readtime);
        // Default blink speed to 2 (medium) when blink markers are present but no speed is set
        if has_blink && ps.blink_speed > 0 {
            page = page.blink_speed(ps.blink_speed);
        } else if has_blink {
            page = page.blink_speed(2);
        }
        if ps.scroll { page = page.scroll_effect(true); }
        if ps.fade { page = page.fading_effect(true); }
        if ps.move_speed > 1 { page = page.moving_speed(ps.move_speed); }
        if ps.bold { page = page.bold(true); }
        pages.push(page);
    }

    let bytes = display.build_pages(&pages);
    match uart.write(&bytes) {
        Ok(_) => info!("Sent {} page(s), {} bytes", max_page, bytes.len()),
        Err(e) => info!("UART error: {:?}", e),
    }
}

fn respond_json(req: esp_idf_svc::http::server::Request<&mut esp_idf_svc::http::server::EspHttpConnection>, json: &str) -> Result<(), anyhow::Error> {
    let mut resp = req.into_response(200, None, &[("Content-Type", "application/json")])?;
    resp.write(json.as_bytes())?;
    Ok(())
}

// ── Route registration ──

/// Register all API endpoints on the HTTP server.
pub fn register(
    server: &mut EspHttpServer<'static>,
    uart: Arc<Mutex<UartDriver<'static>>>,
    state: Arc<Mutex<DisplayState>>,
    settings: Arc<Mutex<Settings>>,
    settings_nvs: Arc<Mutex<esp_idf_svc::nvs::EspNvs<esp_idf_svc::nvs::NvsDefault>>>,
) -> anyhow::Result<()> {
    // GET /api/state — returns all pages + display config for the UI
    let state_g = state.clone();
    let cfg_g = settings.clone();
    server.fn_handler("/api/state", esp_idf_svc::http::Method::Get, move |req| {
        let json = match (state_g.lock(), cfg_g.lock()) {
            (Ok(st), Ok(cfg)) => state_to_json(&st, &cfg),
            _ => r#"{"error":"lock"}"#.to_string(),
        };
        respond_json(req, &json)
    })?;

    // POST /api/page — update a single page's content + attributes, then push all pages to UART
    let uart_p = uart.clone();
    let state_p = state.clone();
    let cfg_p = settings.clone();
    server.fn_handler("/api/page", esp_idf_svc::http::Method::Post, move |mut req| {
        let body = read_body(&mut req);
        let cfg = cfg_p.lock().unwrap().clone();

        let page: usize = json_str(&body, "page")
            .and_then(|v| v.parse().ok()).unwrap_or(1).max(1).min(cfg.max_pages);
        let brightness: u8 = json_str(&body, "brightness")
            .and_then(|v| v.parse().ok()).unwrap_or(17).max(1).min(17);
        let readtime: f32 = json_str(&body, "readtime")
            .and_then(|v| v.parse().ok()).unwrap_or(5.0);
        let blink_speed: u8 = json_str(&body, "blink_speed")
            .and_then(|v| v.parse().ok()).unwrap_or(0).min(4);
        let scroll = json_bool(&body, "scroll");
        let fade = json_bool(&body, "fade");
        let move_speed: u8 = json_str(&body, "move_speed")
            .and_then(|v| v.parse().ok()).unwrap_or(1).max(1).min(20);
        let bold = json_bool(&body, "bold");
        let lines = json_lines(&body, cfg.visible_lines);

        let resp_json = if let Ok(mut st) = state_p.lock() {
            st.update_page(page, lines, brightness, readtime, blink_speed, scroll, fade, move_speed, bold);
            if let Ok(uart) = uart_p.lock() {
                send_all(&st, &uart, &cfg);
            }
            page_to_json(&st, page, &cfg)
        } else {
            r#"{"error":"lock"}"#.to_string()
        };
        respond_json(req, &resp_json)
    })?;

    // POST /api/reset — clear all pages in NVS and send a blank frame to the display
    let uart_r = uart.clone();
    let state_r = state.clone();
    let cfg_r = settings.clone();
    server.fn_handler("/api/reset", esp_idf_svc::http::Method::Post, move |req| {
        let cfg = cfg_r.lock().unwrap().clone();
        if let Ok(mut st) = state_r.lock() {
            st.reset();
            if let Ok(uart) = uart_r.lock() {
                send_all(&st, &uart, &cfg);
            }
        }
        respond_json(req, r#"{"ok":true}"#)
    })?;

    // GET /api/settings — return current WiFi + display hardware config
    let cfg_sg = settings.clone();
    server.fn_handler("/api/settings", esp_idf_svc::http::Method::Get, move |req| {
        let json = if let Ok(cfg) = cfg_sg.lock() {
            settings_to_json(&cfg)
        } else {
            r#"{"error":"lock"}"#.to_string()
        };
        respond_json(req, &json)
    })?;

    // POST /api/settings — persist new config to NVS (requires reboot to take effect)
    let cfg_sp = settings.clone();
    let nvs_sp = settings_nvs.clone();
    server.fn_handler("/api/settings", esp_idf_svc::http::Method::Post, move |mut req| {
        let body = read_body(&mut req);
        if let (Ok(mut cfg), Ok(mut nvs)) = (cfg_sp.lock(), nvs_sp.lock()) {
            cfg.wifi_ssid = json_str(&body, "wifi_ssid").unwrap_or_default();
            cfg.wifi_pass = json_str(&body, "wifi_pass").unwrap_or_default();
            cfg.connect_timeout = json_str(&body, "connect_timeout")
                .and_then(|v| v.parse().ok()).unwrap_or(30);
            cfg.ap_ssid = json_str(&body, "ap_ssid").unwrap_or("DatalitePanel".to_string());
            cfg.ap_pass = json_str(&body, "ap_pass").unwrap_or("datalite123".to_string());
            cfg.controllers = json_str(&body, "controllers")
                .and_then(|v| v.parse().ok()).unwrap_or(3).max(1).min(32);
            cfg.lines_per_ctrl = json_str(&body, "lines_per_ctrl")
                .and_then(|v| v.parse().ok()).unwrap_or(8).max(1).min(8);
            cfg.chars_per_line = json_str(&body, "chars_per_line")
                .and_then(|v| v.parse().ok()).unwrap_or(32).max(1).min(64);
            cfg.visible_lines = json_str(&body, "visible_lines")
                .and_then(|v| v.parse().ok()).unwrap_or(20).max(1).min(32);
            cfg.max_pages = json_str(&body, "max_pages")
                .and_then(|v| v.parse().ok()).unwrap_or(4).max(1).min(8);
            cfg.save(&mut nvs);
            respond_json(req, r#"{"ok":true,"reboot":true}"#)
        } else {
            respond_json(req, r#"{"error":"lock"}"#)
        }
    })?;

    // POST /api/reboot — respond first, then restart the chip after a short delay
    server.fn_handler("/api/reboot", esp_idf_svc::http::Method::Post, move |req| {
        respond_json(req, r#"{"ok":true}"#)?;
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            unsafe { esp_idf_svc::sys::esp_restart(); }
        });
        Ok::<(), anyhow::Error>(())
    })?;

    // GET /api/diag — probe UART health and return the raw hex payload for debugging
    let uart_d = uart.clone();
    let state_d = state.clone();
    let cfg_d = settings.clone();
    server.fn_handler("/api/diag", esp_idf_svc::http::Method::Get, move |req| {
        let cfg = cfg_d.lock().unwrap().clone();
        let display = Display::new(cfg.controllers, cfg.lines_per_ctrl, cfg.chars_per_line);
        let mut out = String::from("{");

        if let Ok(uart) = uart_d.lock() {
            match uart.write(&[0x0D]) {
                Ok(_) => out.push_str(r#""uart":"ok","#),
                Err(e) => out.push_str(&format!(r#""uart":"error: {:?}","#, e)),
            }
        }

        if let Ok(st) = state_d.lock() {
            let max_page = (0..cfg.max_pages).rev()
                .find(|&i| st.pages[i].lines[..cfg.visible_lines].iter().any(|l| !l.is_empty()))
                .map(|i| i + 1).unwrap_or(0);
            let bytes = if max_page == 0 {
                display.build_clear()
            } else {
                let mut pages = Vec::new();
                for pg_idx in 0..max_page {
                    let ps = &st.pages[pg_idx];
                    let mut page = display.page((pg_idx + 1) as u8);
                    for i in 0..cfg.visible_lines {
                        if !ps.lines[i].is_empty() {
                            page = page.line((i + 1) as u16, &ps.lines[i]);
                        }
                    }
                    page = page.brightness(ps.brightness).readtime_secs(ps.readtime);
                    pages.push(page);
                }
                display.build_pages(&pages)
            };
            out.push_str(&format!(r#""active_pages":{},"payload_bytes":{},"hex":""#, max_page, bytes.len()));
            for b in &bytes { out.push_str(&format!("{:02X} ", b)); }
            out.push('"');
        }
        out.push('}');
        respond_json(req, &out)
    })?;

    Ok(())
}
