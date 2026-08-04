// SPDX-License-Identifier: MPL-2.0

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};

pub(crate) const APPLICATION_ID: &str = "io.github.ullissescastro.RetomarAmbiente";

/// Preferências persistentes do Retomar Ambiente.
#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    /// IDs dos arquivos `.desktop` elegíveis para restauração.
    pub selected_apps: Vec<String>,
    /// Mantém o diálogo de restauração habilitado no início da sessão.
    pub ask_on_login: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            selected_apps: vec![
                "firefox.desktop".to_owned(),
                "google-chrome.desktop".to_owned(),
                "com.system76.CosmicTerm.desktop".to_owned(),
                "com.system76.CosmicFiles.desktop".to_owned(),
                "code.desktop".to_owned(),
            ],
            ask_on_login: true,
        }
    }
}

pub(crate) fn load_config() -> Config {
    cosmic_config::Config::new(APPLICATION_ID, Config::VERSION)
        .map(|context| match Config::get_entry(&context) {
            Ok(config) => config,
            Err((_errors, config)) => config,
        })
        .unwrap_or_default()
}
