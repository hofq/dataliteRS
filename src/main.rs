mod settings;
mod state;
mod web;

use std::sync::{Arc, Mutex};

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        gpio,
        prelude::Peripherals,
        units::Hertz,
        uart::{self, UartDriver},
    },
    nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault},
    wifi::{AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};
use log::info;

use settings::Settings;
use state::DisplayState;

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs_default = EspDefaultNvsPartition::take()?;

    // Load settings
    let cfg_nvs = EspNvs::<NvsDefault>::new(nvs_default.clone(), "config", true)?;
    let cfg = Settings::load(&cfg_nvs);
    info!("Settings: controllers={} lines/ctrl={} cols={} rows={} pages={}",
        cfg.controllers, cfg.lines_per_ctrl, cfg.chars_per_line, cfg.visible_lines, cfg.max_pages);

    let settings = Arc::new(Mutex::new(cfg.clone()));
    let settings_nvs = Arc::new(Mutex::new(cfg_nvs));

    // Display state
    let nvs = EspNvs::<NvsDefault>::new(nvs_default.clone(), "display", true)?;
    let display_state = Arc::new(Mutex::new(DisplayState::new(nvs)));

    // UART
    let uart = UartDriver::new(
        peripherals.uart1,
        peripherals.pins.gpio21,
        peripherals.pins.gpio20,
        Option::<gpio::AnyIOPin>::None,
        Option::<gpio::AnyIOPin>::None,
        &uart::config::Config::new().baudrate(Hertz(4800)),
    )?;
    let uart = Arc::new(Mutex::new(uart));
    info!("UART ready on GPIO21 (TX) @ 4800 baud");

    // WiFi
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs_default))?,
        sysloop,
    )?;

    let mut sta_connected = false;

    // Try STA mode if WiFi SSID is configured
    if !cfg.wifi_ssid.is_empty() {
        info!("Trying WiFi STA: {} (timeout {}s)", cfg.wifi_ssid, cfg.connect_timeout);
        wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: cfg.wifi_ssid.as_str().try_into().unwrap_or_default(),
            password: cfg.wifi_pass.as_str().try_into().unwrap_or_default(),
            ..Default::default()
        }))?;
        wifi.start()?;

        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs(cfg.connect_timeout as u64);

        while std::time::Instant::now() < deadline {
            match wifi.connect() {
                Ok(_) => {
                    match wifi.wait_netif_up() {
                        Ok(_) => {
                            let ip = wifi.wifi().sta_netif().get_ip_info()?.ip;
                            info!("WiFi STA connected: http://{}", ip);
                            sta_connected = true;
                            break;
                        }
                        Err(e) => info!("Netif error: {:?}", e),
                    }
                }
                Err(e) => {
                    info!("WiFi connect attempt failed: {:?}", e);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }

        if !sta_connected {
            info!("STA failed, switching to AP mode");
            let _ = wifi.disconnect();
            let _ = wifi.stop();
        }
    }

    // Fallback to AP mode
    if !sta_connected {
        wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
            ssid: cfg.ap_ssid.as_str().try_into().unwrap_or_default(),
            password: cfg.ap_pass.as_str().try_into().unwrap_or_default(),
            auth_method: AuthMethod::WPA2Personal,
            channel: 6,
            max_connections: 4,
            ..Default::default()
        }))?;
        wifi.start()?;
        wifi.wait_netif_up()?;

        let ip = wifi.wifi().ap_netif().get_ip_info()?.ip;
        info!("WiFi AP: {} / {}", cfg.ap_ssid, cfg.ap_pass);
        info!("Web UI: http://{}", ip);
    }

    // Web server
    let _server = web::start(uart, display_state, settings, settings_nvs)?;

    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
