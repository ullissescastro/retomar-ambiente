// SPDX-License-Identifier: MPL-2.0

use crate::config::{Config, load_config};
use crate::desktop::{DesktopApp, discover_desktop_apps, find_desktop_id_for_app_id};
use cosmic::cctk::{
    self,
    sctk::{
        self,
        output::{OutputHandler, OutputState},
        registry::{ProvidesRegistryState, RegistryState},
    },
    toplevel_info::{ToplevelInfoHandler, ToplevelInfoState},
};
use cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::State as ToplevelState;
use cosmic_protocols::{
    toplevel_info::v1::client::{zcosmic_toplevel_handle_v1, zcosmic_toplevel_info_v1},
    toplevel_management::v1::client::zcosmic_toplevel_manager_v1,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wayland_client::{
    Connection, Dispatch, QueueHandle, WEnum, event_created_child,
    globals::registry_queue_init,
    protocol::{wl_output, wl_registry},
};
use wayland_protocols::ext::{
    foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    workspace::v1::client::{
        ext_workspace_group_handle_v1, ext_workspace_handle_v1, ext_workspace_manager_v1,
    },
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct SessionSnapshot {
    pub(crate) session_id: String,
    pub(crate) saved_at: u64,
    pub(crate) windows: Vec<WindowSnapshot>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct WindowSnapshot {
    pub(crate) desktop_id: String,
    pub(crate) app_id: String,
    pub(crate) title: String,
    pub(crate) output: Option<String>,
    #[serde(default)]
    pub(crate) output_description: Option<String>,
    #[serde(default)]
    pub(crate) output_make: Option<String>,
    #[serde(default)]
    pub(crate) output_model: Option<String>,
    pub(crate) maximized: bool,
    pub(crate) minimized: bool,
    pub(crate) fullscreen: bool,
}

impl SessionSnapshot {
    pub(crate) fn restorable_desktop_ids(&self, config: &Config) -> Vec<String> {
        let present: HashSet<&str> = self
            .windows
            .iter()
            .map(|window| window.desktop_id.as_str())
            .collect();

        config
            .selected_apps
            .iter()
            .filter(|desktop_id| present.contains(desktop_id.as_str()))
            .cloned()
            .collect()
    }

    fn filtered_for_config(mut self, config: &Config) -> Self {
        let selected: HashSet<&str> = config.selected_apps.iter().map(String::as_str).collect();
        self.windows
            .retain(|window| selected.contains(window.desktop_id.as_str()));
        self
    }
}

pub(crate) fn load_restore_snapshot(config: &Config) -> Option<SessionSnapshot> {
    let paths = SnapshotPaths::new()?;
    let session_id = current_session_id();

    if let Some(current) = read_snapshot(&paths.current) {
        if current.session_id != session_id {
            return Some(current.filtered_for_config(config));
        }
    }

    read_snapshot(&paths.previous).map(|snapshot| snapshot.filtered_for_config(config))
}

pub(crate) fn spawn_restore_worker() -> std::io::Result<()> {
    let executable = std::env::current_exe()?;
    std::process::Command::new(executable)
        .arg("--restore-layout")
        .spawn()
        .map(|_| ())
}

pub(crate) fn run_agent() -> Result<(), Box<dyn Error>> {
    let apps = discover_desktop_apps();
    let paths = SnapshotPaths::new().ok_or("não foi possível determinar o diretório de estado")?;
    let session_id = current_session_id();
    paths.prepare_for_session(&session_id)?;

    let connection = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init(&connection)?;
    let queue_handle = queue.handle();
    let registry_state = RegistryState::new(&globals);

    let mut state = AgentState {
        apps,
        snapshot_path: paths.current,
        session_id,
        output_state: OutputState::new(&globals, &queue_handle),
        toplevel_info_state: ToplevelInfoState::new(&registry_state, &queue_handle),
        registry_state,
        last_outputs: HashMap::new(),
    };

    // O ext_foreign_toplevel_list enumera também janelas que já estavam abertas
    // antes de o agente iniciar. O protocolo COSMIC v2/v3 adiciona geometria,
    // estado, monitor e workspace a cada uma delas.
    for _ in 0..4 {
        queue.roundtrip(&mut state)?;
    }
    state.persist_snapshot()?;

    loop {
        queue.blocking_dispatch(&mut state)?;
        state.persist_snapshot()?;
    }
}

struct AgentState {
    apps: Vec<DesktopApp>,
    snapshot_path: PathBuf,
    session_id: String,
    output_state: OutputState,
    toplevel_info_state: ToplevelInfoState,
    registry_state: RegistryState,
    // output_enter/output_leave descrevem visibilidade. Uma janela em outro
    // workspace pode sair temporariamente de todos os outputs, então guardamos
    // o último monitor válido até a janela fechar ou o monitor ser removido.
    last_outputs: HashMap<String, wl_output::WlOutput>,
}

impl AgentState {
    fn remember_current_output(&mut self, toplevel: &ExtForeignToplevelHandleV1) {
        let Some(info) = self.toplevel_info_state.info(toplevel).cloned() else {
            return;
        };

        let output = info
            .geometry
            .keys()
            .next()
            .cloned()
            .or_else(|| info.output.iter().next().cloned());

        if let Some(output) = output {
            self.last_outputs.insert(info.identifier, output);
        }
    }

    fn persist_snapshot(&mut self) -> Result<(), Box<dyn Error>> {
        // As preferências podem mudar pela interface enquanto o agente está
        // ativo. Recarregar aqui evita exigir logout ou reinício do processo.
        let eligible: HashSet<String> = load_config().selected_apps.into_iter().collect();
        let toplevels = self
            .toplevel_info_state
            .toplevels()
            .cloned()
            .collect::<Vec<_>>();

        let active_identifiers = toplevels
            .iter()
            .map(|info| info.identifier.clone())
            .collect::<HashSet<_>>();
        self.last_outputs
            .retain(|identifier, _| active_identifiers.contains(identifier));

        let mut windows = Vec::new();
        for info in toplevels {
            let app_id = info.app_id.trim();
            if app_id.is_empty() {
                continue;
            }

            let Some(desktop_id) = find_desktop_id_for_app_id(app_id, &self.apps, &eligible) else {
                continue;
            };

            let output = info
                .geometry
                .keys()
                .next()
                .cloned()
                .or_else(|| info.output.iter().next().cloned())
                .or_else(|| self.last_outputs.get(&info.identifier).cloned());
            let output_info = output
                .as_ref()
                .and_then(|handle| self.output_state.info(handle));

            windows.push(WindowSnapshot {
                desktop_id: desktop_id.to_owned(),
                app_id: app_id.to_owned(),
                title: info.title,
                output: output_info.as_ref().and_then(|value| value.name.clone()),
                output_description: output_info
                    .as_ref()
                    .and_then(|value| value.description.clone()),
                output_make: output_info
                    .as_ref()
                    .and_then(|value| non_empty_string(&value.make)),
                output_model: output_info
                    .as_ref()
                    .and_then(|value| non_empty_string(&value.model)),
                maximized: info.state.contains(&ToplevelState::Maximized),
                minimized: info.state.contains(&ToplevelState::Minimized),
                fullscreen: info.state.contains(&ToplevelState::Fullscreen),
            });
        }

        windows.sort_by(|left, right| {
            left.desktop_id
                .cmp(&right.desktop_id)
                .then_with(|| left.title.cmp(&right.title))
        });

        write_snapshot(
            &self.snapshot_path,
            &SessionSnapshot {
                session_id: self.session_id.clone(),
                saved_at: unix_timestamp(),
                windows,
            },
        )?;
        Ok(())
    }
}

impl ProvidesRegistryState for AgentState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers!();
}

impl OutputHandler for AgentState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.last_outputs
            .retain(|_, remembered| remembered != &output);
    }
}

impl ToplevelInfoHandler for AgentState {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info_state
    }

    fn new_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        self.remember_current_output(toplevel);
    }

    fn update_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        self.remember_current_output(toplevel);
    }

    fn toplevel_closed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        toplevel: &ExtForeignToplevelHandleV1,
    ) {
        if let Some(info) = self.toplevel_info_state.info(toplevel) {
            self.last_outputs.remove(&info.identifier);
        }
    }
}

sctk::delegate_registry!(AgentState);
sctk::delegate_output!(AgentState);
cctk::delegate_toplevel_info!(AgentState);

pub(crate) fn run_restore_worker() -> Result<(), Box<dyn Error>> {
    let config = load_config();
    let Some(snapshot) = load_restore_snapshot(&config) else {
        return Ok(());
    };
    if snapshot.windows.is_empty() {
        return Ok(());
    }

    let apps = discover_desktop_apps();
    let mut state = WaylandState::new_restore(apps, snapshot.windows);
    let connection = Connection::connect_to_env()?;
    let display = connection.display();
    let mut queue = connection.new_event_queue();
    let queue_handle = queue.handle();
    let _registry = display.get_registry(&queue_handle, ());

    for _ in 0..4 {
        queue.roundtrip(&mut state)?;
    }

    // Janelas que já existiam antes do clique em Restaurar não pertencem
    // à operação atual e não devem consumir os destinos do retrato salvo.
    state.ignore_existing_toplevels();

    for _ in 0..100 {
        queue.roundtrip(&mut state)?;
        state.apply_restore_requests();
        connection.flush()?;

        if state.restore_finished() {
            break;
        }

        thread::sleep(Duration::from_millis(250));
    }

    Ok(())
}

#[derive(Clone)]
struct OutputInfo {
    handle: wl_output::WlOutput,
    name: Option<String>,
    description: Option<String>,
    make: Option<String>,
    model: Option<String>,
}

struct ToplevelInfo {
    handle: zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
    title: Option<String>,
    app_id: Option<String>,
    outputs: Vec<wl_output::WlOutput>,
    states: Vec<u32>,
    ready: bool,
    restore_assigned: bool,
}

struct WorkspaceGroupInfo {
    handle: ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1,
    outputs: Vec<wl_output::WlOutput>,
    workspaces: Vec<ext_workspace_handle_v1::ExtWorkspaceHandleV1>,
}

struct WorkspaceInfo {
    handle: ext_workspace_handle_v1::ExtWorkspaceHandleV1,
    states: ext_workspace_handle_v1::State,
}

struct RestoreTarget {
    snapshot: WindowSnapshot,
    assigned: bool,
}

struct WaylandState {
    apps: Vec<DesktopApp>,
    eligible: HashSet<String>,
    toplevel_info: Option<zcosmic_toplevel_info_v1::ZcosmicToplevelInfoV1>,
    toplevel_manager: Option<zcosmic_toplevel_manager_v1::ZcosmicToplevelManagerV1>,
    workspace_manager: Option<ext_workspace_manager_v1::ExtWorkspaceManagerV1>,
    outputs: Vec<OutputInfo>,
    toplevels: Vec<ToplevelInfo>,
    workspace_groups: Vec<WorkspaceGroupInfo>,
    workspaces: Vec<WorkspaceInfo>,
    restore_targets: Vec<RestoreTarget>,
}

impl WaylandState {
    fn new_restore(apps: Vec<DesktopApp>, windows: Vec<WindowSnapshot>) -> Self {
        let eligible = windows
            .iter()
            .map(|window| window.desktop_id.clone())
            .collect();
        Self {
            apps,
            eligible,
            toplevel_info: None,
            toplevel_manager: None,
            workspace_manager: None,
            outputs: Vec::new(),
            toplevels: Vec::new(),
            workspace_groups: Vec::new(),
            workspaces: Vec::new(),
            restore_targets: windows
                .into_iter()
                .map(|snapshot| RestoreTarget {
                    snapshot,
                    assigned: false,
                })
                .collect(),
        }
    }

    fn ignore_existing_toplevels(&mut self) {
        for toplevel in &mut self.toplevels {
            toplevel.restore_assigned = true;
        }
    }

    fn apply_restore_requests(&mut self) {
        let Some(manager) = self.toplevel_manager.clone() else {
            return;
        };

        for toplevel_index in 0..self.toplevels.len() {
            if !self.toplevels[toplevel_index].ready
                || self.toplevels[toplevel_index].restore_assigned
            {
                continue;
            }

            let Some(app_id) = self.toplevels[toplevel_index].app_id.clone() else {
                continue;
            };
            let Some(desktop_id) =
                find_desktop_id_for_app_id(&app_id, &self.apps, &self.eligible).map(str::to_owned)
            else {
                continue;
            };

            let title = self.toplevels[toplevel_index]
                .title
                .clone()
                .unwrap_or_default();

            let target_index = self.find_restore_target(&desktop_id, &title);
            let Some(target_index) = target_index else {
                self.toplevels[toplevel_index].restore_assigned = true;
                continue;
            };

            let target = self.restore_targets[target_index].snapshot.clone();
            let toplevel = self.toplevels[toplevel_index].handle.clone();

            let destination_output = self.output_for_snapshot(&target).cloned();

            if let Some(output) = destination_output {
                if let Some(workspace) = self.workspace_for_output(&output).cloned() {
                    manager.move_to_ext_workspace(&toplevel, &workspace, &output);
                }

                if target.fullscreen {
                    manager.set_fullscreen(&toplevel, Some(&output));
                } else if target.maximized {
                    manager.set_maximized(&toplevel);
                }
            } else if target.maximized {
                manager.set_maximized(&toplevel);
            }

            if target.minimized {
                manager.set_minimized(&toplevel);
            }

            self.restore_targets[target_index].assigned = true;
            self.toplevels[toplevel_index].restore_assigned = true;
        }
    }

    fn find_restore_target(&self, desktop_id: &str, title: &str) -> Option<usize> {
        if !title.is_empty() {
            if let Some((index, _)) = self.restore_targets.iter().enumerate().find(|(_, target)| {
                !target.assigned
                    && target.snapshot.desktop_id == desktop_id
                    && !target.snapshot.title.is_empty()
                    && target.snapshot.title == title
            }) {
                return Some(index);
            }
        }

        self.restore_targets
            .iter()
            .enumerate()
            .find(|(_, target)| !target.assigned && target.snapshot.desktop_id == desktop_id)
            .map(|(index, _)| index)
    }

    fn output_for_snapshot(&self, snapshot: &WindowSnapshot) -> Option<&wl_output::WlOutput> {
        if let (Some(make), Some(model)) = (
            snapshot
                .output_make
                .as_deref()
                .filter(|value| meaningful_identity(value)),
            snapshot
                .output_model
                .as_deref()
                .filter(|value| meaningful_identity(value)),
        ) {
            if let Some(output) = self.outputs.iter().find(|output| {
                output.make.as_deref() == Some(make) && output.model.as_deref() == Some(model)
            }) {
                return Some(&output.handle);
            }
        }

        if let Some(name) = snapshot.output.as_deref() {
            if let Some(output) = self
                .outputs
                .iter()
                .find(|output| output.name.as_deref() == Some(name))
            {
                return Some(&output.handle);
            }
        }

        let description = snapshot.output_description.as_deref()?;
        self.outputs
            .iter()
            .find(|output| output.description.as_deref() == Some(description))
            .map(|output| &output.handle)
    }

    fn workspace_for_output(
        &self,
        output: &wl_output::WlOutput,
    ) -> Option<&ext_workspace_handle_v1::ExtWorkspaceHandleV1> {
        let group = self
            .workspace_groups
            .iter()
            .find(|group| group.outputs.iter().any(|candidate| candidate == output))?;

        group
            .workspaces
            .iter()
            .find(|workspace| {
                self.workspaces
                    .iter()
                    .find(|info| &info.handle == *workspace)
                    .is_some_and(|info| {
                        info.states.contains(ext_workspace_handle_v1::State::Active)
                    })
            })
            .or_else(|| group.workspaces.first())
    }

    fn restore_finished(&self) -> bool {
        !self.restore_targets.is_empty()
            && self.restore_targets.iter().all(|target| target.assigned)
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        queue_handle: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };

        match interface.as_str() {
            "zcosmic_toplevel_info_v1" => {
                state.toplevel_info = Some(
                    registry.bind::<zcosmic_toplevel_info_v1::ZcosmicToplevelInfoV1, _, _>(
                        name,
                        1,
                        queue_handle,
                        (),
                    ),
                );
            }
            "zcosmic_toplevel_manager_v1" => {
                state.toplevel_manager = Some(
                    registry.bind::<zcosmic_toplevel_manager_v1::ZcosmicToplevelManagerV1, _, _>(
                        name,
                        version.min(4),
                        queue_handle,
                        (),
                    ),
                );
            }
            "ext_workspace_manager_v1" => {
                state.workspace_manager = Some(
                    registry.bind::<ext_workspace_manager_v1::ExtWorkspaceManagerV1, _, _>(
                        name,
                        version.min(1),
                        queue_handle,
                        (),
                    ),
                );
            }
            "wl_output" => {
                let output = registry.bind::<wl_output::WlOutput, _, _>(
                    name,
                    version.min(4),
                    queue_handle,
                    (),
                );
                state.outputs.push(OutputInfo {
                    handle: output,
                    name: None,
                    description: None,
                    make: None,
                    model: None,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for WaylandState {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let Some(info) = state.outputs.iter_mut().find(|info| &info.handle == output) {
            match event {
                wl_output::Event::Geometry { make, model, .. } => {
                    info.make = non_empty_string(&make);
                    info.model = non_empty_string(&model);
                }
                wl_output::Event::Name { name } => info.name = Some(name),
                wl_output::Event::Description { description } => {
                    info.description = Some(description);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<zcosmic_toplevel_info_v1::ZcosmicToplevelInfoV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &zcosmic_toplevel_info_v1::ZcosmicToplevelInfoV1,
        event: zcosmic_toplevel_info_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zcosmic_toplevel_info_v1::Event::Toplevel { toplevel } = event {
            state.toplevels.push(ToplevelInfo {
                handle: toplevel,
                title: None,
                app_id: None,
                outputs: Vec::new(),
                states: Vec::new(),
                ready: false,
                restore_assigned: false,
            });
        }
    }

    event_created_child!(
        WaylandState,
        zcosmic_toplevel_info_v1::ZcosmicToplevelInfoV1,
        [
            zcosmic_toplevel_info_v1::EVT_TOPLEVEL_OPCODE =>
                (zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1, ()),
        ]
    );
}

impl Dispatch<zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        toplevel: &zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1,
        event: zcosmic_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zcosmic_toplevel_handle_v1::Event::Title { title } => {
                if let Some(info) = state
                    .toplevels
                    .iter_mut()
                    .find(|info| &info.handle == toplevel)
                {
                    info.title = Some(title);
                }
            }
            zcosmic_toplevel_handle_v1::Event::AppId { app_id } => {
                if let Some(info) = state
                    .toplevels
                    .iter_mut()
                    .find(|info| &info.handle == toplevel)
                {
                    info.app_id = Some(app_id);
                }
            }
            zcosmic_toplevel_handle_v1::Event::OutputEnter { output } => {
                if let Some(info) = state
                    .toplevels
                    .iter_mut()
                    .find(|info| &info.handle == toplevel)
                {
                    if !info.outputs.iter().any(|candidate| candidate == &output) {
                        info.outputs.push(output);
                    }
                }
            }
            zcosmic_toplevel_handle_v1::Event::OutputLeave { output } => {
                if let Some(info) = state
                    .toplevels
                    .iter_mut()
                    .find(|info| &info.handle == toplevel)
                {
                    info.outputs.retain(|candidate| candidate != &output);
                }
            }
            zcosmic_toplevel_handle_v1::Event::State { state: values } => {
                if let Some(info) = state
                    .toplevels
                    .iter_mut()
                    .find(|info| &info.handle == toplevel)
                {
                    info.states = parse_u32_array(&values);
                }
            }
            zcosmic_toplevel_handle_v1::Event::Done => {
                if let Some(info) = state
                    .toplevels
                    .iter_mut()
                    .find(|info| &info.handle == toplevel)
                {
                    info.ready = true;
                }
            }
            zcosmic_toplevel_handle_v1::Event::Closed => {
                state.toplevels.retain(|info| &info.handle != toplevel);
            }
            _ => {}
        }
    }
}

impl Dispatch<zcosmic_toplevel_manager_v1::ZcosmicToplevelManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &zcosmic_toplevel_manager_v1::ZcosmicToplevelManagerV1,
        _: zcosmic_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_workspace_manager_v1::ExtWorkspaceManagerV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &ext_workspace_manager_v1::ExtWorkspaceManagerV1,
        event: ext_workspace_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_workspace_manager_v1::Event::WorkspaceGroup { workspace_group } => {
                state.workspace_groups.push(WorkspaceGroupInfo {
                    handle: workspace_group,
                    outputs: Vec::new(),
                    workspaces: Vec::new(),
                });
            }
            ext_workspace_manager_v1::Event::Workspace { workspace } => {
                state.workspaces.push(WorkspaceInfo {
                    handle: workspace,
                    states: ext_workspace_handle_v1::State::empty(),
                });
            }
            _ => {}
        }
    }

    event_created_child!(
        WaylandState,
        ext_workspace_manager_v1::ExtWorkspaceManagerV1,
        [
            ext_workspace_manager_v1::EVT_WORKSPACE_GROUP_OPCODE =>
                (ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1, ()),
            ext_workspace_manager_v1::EVT_WORKSPACE_OPCODE =>
                (ext_workspace_handle_v1::ExtWorkspaceHandleV1, ()),
        ]
    );
}

impl Dispatch<ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        group: &ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1,
        event: ext_workspace_group_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if matches!(&event, ext_workspace_group_handle_v1::Event::Removed) {
            state
                .workspace_groups
                .retain(|candidate| &candidate.handle != group);
            return;
        }

        let Some(info) = state
            .workspace_groups
            .iter_mut()
            .find(|info| &info.handle == group)
        else {
            return;
        };

        match event {
            ext_workspace_group_handle_v1::Event::OutputEnter { output } => {
                if !info.outputs.iter().any(|candidate| candidate == &output) {
                    info.outputs.push(output);
                }
            }
            ext_workspace_group_handle_v1::Event::OutputLeave { output } => {
                info.outputs.retain(|candidate| candidate != &output);
            }
            ext_workspace_group_handle_v1::Event::WorkspaceEnter { workspace } => {
                if !info
                    .workspaces
                    .iter()
                    .any(|candidate| candidate == &workspace)
                {
                    info.workspaces.push(workspace);
                }
            }
            ext_workspace_group_handle_v1::Event::WorkspaceLeave { workspace } => {
                info.workspaces.retain(|candidate| candidate != &workspace);
            }
            _ => {}
        }
    }
}

impl Dispatch<ext_workspace_handle_v1::ExtWorkspaceHandleV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        workspace: &ext_workspace_handle_v1::ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_workspace_handle_v1::Event::Removed => {
                state.workspaces.retain(|info| &info.handle != workspace);
                for group in &mut state.workspace_groups {
                    group.workspaces.retain(|candidate| candidate != workspace);
                }
            }
            ext_workspace_handle_v1::Event::State { state: value } => {
                if let Some(info) = state
                    .workspaces
                    .iter_mut()
                    .find(|info| &info.handle == workspace)
                {
                    info.states = match value {
                        WEnum::Value(states) => states,
                        WEnum::Unknown(bits) => {
                            ext_workspace_handle_v1::State::from_bits_retain(bits)
                        }
                    };
                }
            }
            _ => {}
        }
    }
}

struct SnapshotPaths {
    current: PathBuf,
    previous: PathBuf,
}

impl SnapshotPaths {
    fn new() -> Option<Self> {
        let state_home = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".local/state"))
            })?;
        let directory = state_home.join("retomar-ambiente");
        Some(Self {
            current: directory.join("current-session.json"),
            previous: directory.join("previous-session.json"),
        })
    }

    fn prepare_for_session(&self, session_id: &str) -> std::io::Result<()> {
        if let Some(parent) = self.current.parent() {
            fs::create_dir_all(parent)?;
        }

        if let Some(snapshot) = read_snapshot(&self.current) {
            if snapshot.session_id != session_id {
                if self.previous.exists() {
                    fs::remove_file(&self.previous)?;
                }
                fs::rename(&self.current, &self.previous)?;
            }
        }

        Ok(())
    }
}

fn current_session_id() -> String {
    // O gerenciador systemd --user pode sobreviver ao logout e conservar
    // variáveis antigas. O inode do socket Wayland muda quando o compositor
    // cria uma nova sessão, mesmo que o nome continue sendo `wayland-1`.
    if let (Some(runtime_dir), Some(display)) = (
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("WAYLAND_DISPLAY"),
    ) {
        let socket = PathBuf::from(runtime_dir).join(display);
        if let Ok(metadata) = fs::metadata(socket) {
            return format!(
                "wayland-{}-{}-{}",
                metadata.dev(),
                metadata.ino(),
                metadata.ctime()
            );
        }
    }

    std::env::var("XDG_SESSION_ID")
        .or_else(|_| std::env::var("WAYLAND_DISPLAY"))
        .unwrap_or_else(|_| format!("process-{}", std::process::id()))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_snapshot(path: &Path) -> Option<SessionSnapshot> {
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

fn write_snapshot(path: &Path, snapshot: &SessionSnapshot) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temporary = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(snapshot)?;
    fs::write(&temporary, data)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn non_empty_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn meaningful_identity(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && !value.eq_ignore_ascii_case("unknown")
}

fn parse_u32_array(values: &[u8]) -> Vec<u32> {
    values
        .chunks_exact(4)
        .map(|chunk| u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
