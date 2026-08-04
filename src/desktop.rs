// SPDX-License-Identifier: MPL-2.0

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) const DESKTOP_ID: &str = "io.github.ullissescastro.RetomarAmbiente.desktop";

#[derive(Clone, Debug)]
pub(crate) struct DesktopApp {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    aliases: Vec<String>,
}

impl DesktopApp {
    pub(crate) fn matches_app_id(&self, app_id: &str) -> bool {
        let candidate = normalize_identifier(app_id);
        if candidate.is_empty() {
            return false;
        }

        self.aliases.iter().any(|alias| {
            candidate == *alias
                || (candidate.len() >= 5
                    && alias.len() >= 5
                    && (candidate.ends_with(alias) || alias.ends_with(&candidate)))
        })
    }
}

pub(crate) fn discover_desktop_apps() -> Vec<DesktopApp> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut search_dirs = Vec::new();

    if let Some(home) = &home {
        search_dirs.push(home.join(".local/share/applications"));
        search_dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }

    search_dirs.push(PathBuf::from("/usr/share/applications"));
    search_dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));

    let mut discovered = BTreeMap::<String, DesktopApp>::new();
    let mut seen_paths = HashSet::<PathBuf>::new();

    for directory in search_dirs {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension() != Some(OsStr::new("desktop")) {
                continue;
            }

            let canonical = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen_paths.insert(canonical) {
                continue;
            }

            let Some(id) = path.file_name().and_then(OsStr::to_str).map(str::to_owned) else {
                continue;
            };

            if id == DESKTOP_ID || discovered.contains_key(&id) {
                continue;
            }

            if let Some(app) = parse_desktop_file(id.clone(), &path) {
                discovered.insert(id, app);
            }
        }
    }

    let mut apps: Vec<_> = discovered.into_values().collect();
    apps.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    apps
}

pub(crate) fn find_desktop_id_for_app_id<'a>(
    app_id: &str,
    apps: &'a [DesktopApp],
    eligible: &HashSet<String>,
) -> Option<&'a str> {
    apps.iter()
        .filter(|app| eligible.contains(&app.id))
        .find(|app| app.matches_app_id(app_id))
        .map(|app| app.id.as_str())
}

fn parse_desktop_file(id: String, path: &Path) -> Option<DesktopApp> {
    let content = fs::read_to_string(path).ok()?;
    let mut in_desktop_entry = false;
    let mut values = BTreeMap::<String, String>::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }

        if !in_desktop_entry {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        values
            .entry(key.to_owned())
            .or_insert_with(|| value.to_owned());
    }

    if values
        .get("Type")
        .is_some_and(|value| value != "Application")
        || parse_bool(values.get("Hidden"))
        || parse_bool(values.get("NoDisplay"))
        || values.get("Exec").is_none_or(String::is_empty)
    {
        return None;
    }

    let name = values
        .get("Name[pt_BR]")
        .or_else(|| values.get("Name[pt]"))
        .or_else(|| values.get("Name"))?
        .trim()
        .to_owned();

    if name.is_empty() {
        return None;
    }

    let mut aliases = HashSet::<String>::new();
    aliases.insert(normalize_identifier(id.trim_end_matches(".desktop")));

    if let Some(startup_wm_class) = values.get("StartupWMClass") {
        aliases.insert(normalize_identifier(startup_wm_class));
    }

    if let Some(exec) = values.get("Exec") {
        if let Some(executable) = executable_from_exec(exec) {
            aliases.insert(normalize_identifier(&executable));

            match executable.as_str() {
                "google-chrome-stable" => {
                    aliases.insert(normalize_identifier("google-chrome"));
                    aliases.insert(normalize_identifier("Google-chrome"));
                }
                "firefox" => {
                    aliases.insert(normalize_identifier("org.mozilla.firefox"));
                }
                "code" => {
                    aliases.insert(normalize_identifier("visual-studio-code"));
                }
                _ => {}
            }
        }
    }

    aliases.retain(|alias| !alias.is_empty());

    Some(DesktopApp {
        id,
        name,
        path: path.to_path_buf(),
        aliases: aliases.into_iter().collect(),
    })
}

fn executable_from_exec(exec: &str) -> Option<String> {
    let mut token = String::new();
    let mut quoted = false;
    let mut escaped = false;

    for character in exec.trim().chars() {
        if escaped {
            token.push(character);
            escaped = false;
            continue;
        }

        match character {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            value if value.is_whitespace() && !quoted => break,
            value => token.push(value),
        }
    }

    if token.is_empty() {
        return None;
    }

    Path::new(&token)
        .file_name()
        .and_then(OsStr::to_str)
        .map(str::to_owned)
}

fn normalize_identifier(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn parse_bool(value: Option<&String>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}
