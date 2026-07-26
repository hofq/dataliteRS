mod api;
mod ui;

use std::sync::{Arc, Mutex};

use esp_idf_svc::hal::uart::UartDriver;
use esp_idf_svc::http::server::{Configuration as HttpConfig, EspHttpServer};
use esp_idf_svc::nvs::{EspNvs, NvsDefault};

use crate::settings::Settings;
use crate::state::DisplayState;

pub fn start(
    uart: Arc<Mutex<UartDriver<'static>>>,
    state: Arc<Mutex<DisplayState>>,
    settings: Arc<Mutex<Settings>>,
    settings_nvs: Arc<Mutex<EspNvs<NvsDefault>>>,
) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&HttpConfig {
        stack_size: 16384,
        ..Default::default()
    })?;

    ui::register(&mut server)?;
    api::register(&mut server, uart, state, settings, settings_nvs)?;

    Ok(server)
}
