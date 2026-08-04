// SPDX-License-Identifier: MPL-2.0

use crate::config::{APPLICATION_ID, Config, load_config};
use crate::desktop::{DesktopApp, discover_desktop_apps};
use crate::fl;
use crate::session::{SessionSnapshot, load_restore_snapshot, spawn_restore_worker};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::{Alignment, Length, Subscription};
use cosmic::prelude::*;
use cosmic::widget::{self, settings};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const AUTOSTART_FILE: &str = "io.github.ullissescastro.RetomarAmbiente.desktop";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupMode {
    Prompt,
    Manage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Screen {
    Prompt,
    Manage,
}

pub struct AppModel {
    core: cosmic::Core,
    screen: Screen,
    apps: Vec<DesktopApp>,
    config: Config,
    restore_snapshot: Option<SessionSnapshot>,
    search: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Restore,
    StartClean,
    OpenManager,
    Back,
    SearchChanged(String),
    ToggleApp(String, bool),
    ToggleAutostart(bool),
    UpdateConfig(Config),
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = StartupMode;
    type Message = Message;

    const APP_ID: &'static str = APPLICATION_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, mode: Self::Flags) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let config = load_config();
        let restore_snapshot = load_restore_snapshot(&config);

        let mut app = Self {
            core,
            screen: match mode {
                StartupMode::Prompt => Screen::Prompt,
                StartupMode::Manage => Screen::Manage,
            },
            apps: discover_desktop_apps(),
            config,
            restore_snapshot,
            search: String::new(),
        };

        let task = app.update_title();
        (app, task)
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self.screen {
            Screen::Prompt => self.prompt_view(),
            Screen::Manage => self.manager_view(),
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        self.core()
            .watch_config::<Config>(APPLICATION_ID)
            .map(|update| Message::UpdateConfig(update.config))
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::Restore => {
                self.launch_restorable_apps();
                std::process::exit(0);
            }
            Message::StartClean => {
                std::process::exit(0);
            }
            Message::OpenManager => {
                self.screen = Screen::Manage;
                return self.update_title();
            }
            Message::Back => {
                self.screen = Screen::Prompt;
                return self.update_title();
            }
            Message::SearchChanged(value) => {
                self.search = value;
            }
            Message::ToggleApp(id, enabled) => {
                if enabled {
                    if !self
                        .config
                        .selected_apps
                        .iter()
                        .any(|selected| selected == &id)
                    {
                        self.config.selected_apps.push(id);
                    }
                } else {
                    self.config.selected_apps.retain(|selected| selected != &id);
                }
                self.persist_config();
                self.restore_snapshot = load_restore_snapshot(&self.config);
            }
            Message::ToggleAutostart(enabled) => match set_autostart(enabled) {
                Ok(()) => {
                    self.config.ask_on_login = enabled;
                    self.persist_config();
                }
                Err(error) => {
                    eprintln!("não foi possível atualizar a inicialização automática: {error}");
                }
            },
            Message::UpdateConfig(config) => {
                self.config = config;
                self.restore_snapshot = load_restore_snapshot(&self.config);
            }
        }

        Task::none()
    }
}

impl AppModel {
    fn prompt_view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let restorable_names = self.restorable_app_names();
        let count = restorable_names.len();

        let mut selected_section = settings::section().title(fl!("previous-session-apps"));
        for name in restorable_names.iter().take(6) {
            selected_section = selected_section
                .add(settings::item::builder(name.clone()).control(widget::text::body("✓")));
        }
        if count > 6 {
            selected_section = selected_section.add(
                settings::item::builder(fl!("more-apps", count = ((count - 6) as i64)))
                    .control(widget::text::body("…")),
            );
        }
        if count == 0 {
            selected_section = selected_section.add(
                settings::item::builder(fl!("no-previous-session"))
                    .control(widget::text::body("—")),
            );
        }

        let restore_button = if count == 0 {
            widget::button::suggested(fl!("restore-apps"))
        } else {
            widget::button::suggested(fl!("restore-apps")).on_press(Message::Restore)
        };

        let buttons = widget::row::with_capacity(3)
            .push(widget::button::text(fl!("manage-apps")).on_press(Message::OpenManager))
            .push(widget::button::text(fl!("start-clean")).on_press(Message::StartClean))
            .push(restore_button)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center);

        let description = if count == 0 {
            fl!("restore-description-empty")
        } else {
            fl!("restore-description")
        };

        let content = widget::column::with_capacity(6)
            .push(widget::text::title1(fl!("restore-title")))
            .push(widget::text::body(description))
            .push(widget::text::caption(fl!(
                "session-count",
                count = (count as i64)
            )))
            .push(selected_section)
            .push(buttons)
            .spacing(spacing.space_m)
            .width(Length::Fill);

        widget::container(content)
            .padding(spacing.space_l)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }

    fn manager_view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::spacing();
        let query = self.search.trim().to_lowercase();

        let mut visible: Vec<&DesktopApp> = self
            .apps
            .iter()
            .filter(|app| {
                query.is_empty()
                    || app.name.to_lowercase().contains(&query)
                    || app.id.to_lowercase().contains(&query)
            })
            .collect();

        visible.sort_by(|left, right| {
            let left_selected = self.is_selected(&left.id);
            let right_selected = self.is_selected(&right.id);
            right_selected
                .cmp(&left_selected)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });

        let login_section = settings::section().add(
            settings::item::builder(fl!("ask-on-login"))
                .description(fl!("ask-on-login-description"))
                .toggler(self.config.ask_on_login, Message::ToggleAutostart),
        );

        let app_list: Element<'_, Message> = if visible.is_empty() {
            widget::container(widget::text::body(fl!("no-apps-found")))
                .padding(spacing.space_m)
                .width(Length::Fill)
                .into()
        } else {
            let mut section = settings::section().title(fl!("installed-apps"));

            for app in visible {
                let id = app.id.clone();
                let selected = self.is_selected(&app.id);
                section = section.add(
                    settings::item::builder(app.name.clone())
                        .description(app.id.clone())
                        .toggler(selected, move |enabled| {
                            Message::ToggleApp(id.clone(), enabled)
                        }),
                );
            }

            widget::scrollable(section).height(Length::Fill).into()
        };

        let search = widget::text_input::search_input(fl!("search-placeholder"), &self.search)
            .on_input(Message::SearchChanged)
            .on_clear(Message::SearchChanged(String::new()));

        let selected_count = self.selected_app_names().len();
        let restore_count = self.restorable_app_names().len();
        let restore_button = if restore_count == 0 {
            widget::button::suggested(fl!("restore-now"))
        } else {
            widget::button::suggested(fl!("restore-now")).on_press(Message::Restore)
        };

        let footer = widget::row::with_capacity(2)
            .push(widget::button::text(fl!("back")).on_press(Message::Back))
            .push(restore_button)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .width(Length::Fill);

        let content = widget::column::with_capacity(8)
            .push(widget::text::title1(fl!("manager-title")))
            .push(widget::text::body(fl!("manager-description")))
            .push(widget::text::caption(fl!(
                "eligible-count",
                count = (selected_count as i64)
            )))
            .push(login_section)
            .push(search)
            .push(app_list)
            .push(footer)
            .spacing(spacing.space_m)
            .height(Length::Fill)
            .width(Length::Fill);

        widget::container(content)
            .padding(spacing.space_l)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn is_selected(&self, id: &str) -> bool {
        self.config
            .selected_apps
            .iter()
            .any(|selected| selected == id)
    }

    fn selected_app_names(&self) -> Vec<String> {
        let by_id: BTreeMap<&str, &DesktopApp> =
            self.apps.iter().map(|app| (app.id.as_str(), app)).collect();

        self.config
            .selected_apps
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).map(|app| app.name.clone()))
            .collect()
    }

    fn restorable_app_ids(&self) -> Vec<String> {
        self.restore_snapshot
            .as_ref()
            .map(|snapshot| snapshot.restorable_desktop_ids(&self.config))
            .unwrap_or_default()
    }

    fn restorable_app_names(&self) -> Vec<String> {
        let by_id: BTreeMap<&str, &DesktopApp> =
            self.apps.iter().map(|app| (app.id.as_str(), app)).collect();

        self.restorable_app_ids()
            .iter()
            .filter_map(|id| by_id.get(id.as_str()).map(|app| app.name.clone()))
            .collect()
    }

    fn launch_restorable_apps(&self) {
        let ids = self.restorable_app_ids();
        if ids.is_empty() {
            return;
        }

        if let Err(error) = spawn_restore_worker() {
            eprintln!("não foi possível iniciar a restauração de monitores: {error}");
        }
        std::thread::sleep(Duration::from_millis(300));

        let by_id: BTreeMap<&str, &DesktopApp> =
            self.apps.iter().map(|app| (app.id.as_str(), app)).collect();

        for id in ids {
            let Some(app) = by_id.get(id.as_str()) else {
                eprintln!("aplicativo não encontrado: {id}");
                continue;
            };

            if let Err(error) = Command::new("gio").arg("launch").arg(&app.path).spawn() {
                eprintln!("não foi possível abrir {}: {error}", app.name);
            }

            std::thread::sleep(Duration::from_millis(220));
        }
    }

    fn persist_config(&self) {
        let Ok(context) = cosmic_config::Config::new(APPLICATION_ID, Config::VERSION) else {
            return;
        };

        if let Err(error) = self.config.write_entry(&context) {
            eprintln!("não foi possível salvar as preferências: {error}");
        }
    }

    fn update_title(&mut self) -> Task<cosmic::Action<Message>> {
        let title = match self.screen {
            Screen::Prompt => fl!("app-title"),
            Screen::Manage => format!("{} — {}", fl!("manager-title"), fl!("app-title")),
        };

        self.set_header_title(title.clone());
        if let Some(id) = self.core.main_window_id() {
            self.set_window_title(title, id)
        } else {
            Task::none()
        }
    }
}

fn set_autostart(enabled: bool) -> std::io::Result<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("diretório pessoal não encontrado"))?;
    let directory = home.join(".config/autostart");
    let destination = directory.join(AUTOSTART_FILE);

    if !enabled {
        match fs::remove_file(destination) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
    }

    fs::create_dir_all(&directory)?;

    let preferred_binary = home.join(".local/bin/retomar-ambiente");
    let binary = if preferred_binary.is_file() {
        preferred_binary
    } else {
        std::env::current_exe()?
    };
    let exec = desktop_quote(&binary);

    let desktop_entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Retomar Ambiente\n\
         Comment=Pergunta se o ambiente de trabalho deve ser restaurado\n\
         Exec={exec} --autostart\n\
         Terminal=false\n\
         StartupNotify=false\n\
         X-GNOME-Autostart-enabled=true\n"
    );

    fs::write(destination, desktop_entry)
}

fn desktop_quote(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}
