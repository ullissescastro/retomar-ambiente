// SPDX-License-Identifier: MPL-2.0

mod app;
mod config;
mod desktop;
mod i18n;
mod session;

use app::StartupMode;
use cosmic::iced::{Limits, Size};

fn main() -> cosmic::iced::Result {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    if arguments.iter().any(|argument| argument == "--agent") {
        if let Err(error) = session::run_agent() {
            eprintln!("agente de sessão encerrado: {error}");
        }
        return Ok(());
    }

    if arguments
        .iter()
        .any(|argument| argument == "--restore-layout")
    {
        if let Err(error) = session::run_restore_worker() {
            eprintln!("restauração de monitores encerrada: {error}");
        }
        return Ok(());
    }

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    let mode = if arguments.iter().any(|argument| argument == "--manage") {
        StartupMode::Manage
    } else {
        StartupMode::Prompt
    };

    let settings = cosmic::app::Settings::default()
        .size(Size::new(760.0, 700.0))
        .size_limits(Limits::NONE.min_width(520.0).min_height(420.0));

    cosmic::app::run::<app::AppModel>(settings, mode)
}
