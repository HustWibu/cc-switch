use crate::services::launch_profiles::{self, LaunchProfile};
use crate::store::AppState;
use tauri::State;

#[tauri::command]
pub fn list_launch_profiles(state: State<'_, AppState>) -> Result<Vec<LaunchProfile>, String> {
    state.db.list_launch_profiles().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_launch_profile(
    state: State<'_, AppState>,
    profile: LaunchProfile,
) -> Result<LaunchProfile, String> {
    launch_profiles::save(&state.db, profile).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_launch_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::services::profile_launchers::uninstall(&state.db, &id).map_err(|e| e.to_string())?;
    state
        .db
        .delete_launch_profile(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn launch_profile_terminal(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let profile = state
        .db
        .list_launch_profiles()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("Unknown launch profile")?;
    launch_profiles::provider(&state.db, &profile).map_err(|e| e.to_string())?;
    if profile.routing_mode == launch_profiles::RoutingMode::Proxy {
        state.proxy_service.start().await?;
    }
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let shell_command = format!(
            "{} profile run-id {}",
            quote(&exe.to_string_lossy()),
            quote(&profile.id)
        );
        crate::commands::misc::launch_terminal_running(&shell_command, "profile")
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Use cc-switch profile run in your terminal on this platform".into())
    }
}

#[tauri::command]
pub async fn install_profile_launcher(
    state: State<'_, AppState>,
    id: String,
) -> Result<crate::services::profile_launchers::LauncherStatus, String> {
    let profile = state
        .db
        .list_launch_profiles()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("Unknown profile")?;
    if profile.routing_mode == crate::services::launch_profiles::RoutingMode::Proxy {
        state.proxy_service.start().await?;
    }
    crate::services::profile_launchers::install(&state.db, &id).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn profile_launcher_status(
    state: State<'_, AppState>,
    id: String,
) -> Result<crate::services::profile_launchers::LauncherStatus, String> {
    let p = state
        .db
        .list_launch_profiles()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("Unknown profile")?;
    Ok(crate::services::profile_launchers::status(&p))
}

#[tauri::command]
pub fn set_default_launch_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .db
        .set_default_launch_profile(&id)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn install_profile_dispatcher(
) -> Result<crate::services::profile_launchers::LauncherStatus, String> {
    crate::services::profile_launchers::install_dispatcher().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn profile_dispatcher_status() -> crate::services::profile_launchers::LauncherStatus {
    crate::services::profile_launchers::dispatcher_status()
}
