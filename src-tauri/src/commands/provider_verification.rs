use crate::{
    app_config::AppType,
    services::provider_verification::{self, VerificationReport},
    store::AppState,
};
use tauri::State;

#[tauri::command]
pub async fn verify_provider_configuration(
    state: State<'_, AppState>,
    handle: tauri::AppHandle,
    app_type: AppType,
    provider_id: String,
    profile_id: Option<String>,
    model: Option<String>,
    generate: bool,
) -> Result<VerificationReport, String> {
    provider_verification::verify_provider(
        state.db.clone(),
        Some(handle),
        app_type,
        &provider_id,
        profile_id.as_deref(),
        model.as_deref(),
        generate,
    )
    .await
}
