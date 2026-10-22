//! Live spelling and grammar highlights in other apps, powered by Harper.
//!
//! The `dictum-grammar` crate owns the persisted config and the highlighter subprocess. This
//! module ties their lifetime to the app and exposes the settings commands the frontend uses.

use dictum_grammar::config::Config;
use dictum_grammar::harper_core::spell::{Dictionary, MutableDictionary};
use dictum_grammar::harper_core::{Dialect, DictWordMetadata};
use dictum_grammar::highlighter_service::HighlighterService;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

/// How often to re-check for Accessibility access while waiting to start the highlighter.
const ACCESSIBILITY_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub struct GrammarState {
    config: Arc<Mutex<Config>>,
    service: HighlighterService,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
pub enum GrammarDialect {
    American,
    British,
    Canadian,
    Australian,
    Indian,
}

impl From<Dialect> for GrammarDialect {
    fn from(dialect: Dialect) -> Self {
        match dialect {
            Dialect::American => Self::American,
            Dialect::British => Self::British,
            Dialect::Canadian => Self::Canadian,
            Dialect::Australian => Self::Australian,
            Dialect::Indian => Self::Indian,
        }
    }
}

impl From<GrammarDialect> for Dialect {
    fn from(dialect: GrammarDialect) -> Self {
        match dialect {
            GrammarDialect::American => Self::American,
            GrammarDialect::British => Self::British,
            GrammarDialect::Canadian => Self::Canadian,
            GrammarDialect::Australian => Self::Australian,
            GrammarDialect::Indian => Self::Indian,
        }
    }
}

#[derive(Serialize, Debug, Clone, Type)]
pub struct GrammarApp {
    pub bundle_id: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Serialize, Debug, Clone, Type)]
pub struct GrammarSettings {
    /// False on platforms without an accessibility integration (Linux).
    pub supported: bool,
    pub enabled: bool,
    pub running: bool,
    pub accessibility_granted: bool,
    pub dialect: GrammarDialect,
    pub dictionary: Vec<String>,
    pub apps: Vec<GrammarApp>,
}

/// Loads the grammar config and starts the highlighter once Accessibility access is granted.
pub fn init(app: &AppHandle) {
    let config = tauri::async_runtime::block_on(load_config());
    let config = Arc::new(Mutex::new(config));
    app.manage(GrammarState {
        service: HighlighterService::new(config.clone()),
        config,
    });

    if dictum_grammar::is_supported() {
        start_when_accessible(app.clone());
    }
}

async fn load_config() -> Config {
    match Config::main_config_exists() {
        Ok(true) => match Config::load_from_system().await {
            Ok(config) => return config,
            Err(e) => warn!("Failed to load grammar config, using defaults: {e}"),
        },
        Ok(false) => {}
        Err(e) => warn!("Failed to check for grammar config: {e}"),
    }

    let config = Config::new();
    if let Err(e) = config.save_to_system().await {
        error!("Failed to save default grammar config: {e}");
    }
    config
}

/// Starts the highlighter in the background as soon as it is enabled and allowed to run.
///
/// Accessibility is often granted later, during onboarding, so this waits for it instead of
/// requiring a restart.
fn start_when_accessible(app: AppHandle) {
    std::thread::spawn(move || {
        while !dictum_grammar::accessibility_granted() {
            std::thread::sleep(ACCESSIBILITY_POLL_INTERVAL);
        }

        let state = app.state::<GrammarState>();
        if !state.config.blocking_lock().highlighter_service_enabled {
            return;
        }

        match state.service.start() {
            Ok(_) => info!("Grammar highlighter started"),
            Err(e) => error!("Failed to start grammar highlighter: {e}"),
        }
    });
}

/// Display name for an app, cached because each lookup runs a Spotlight query.
fn app_name(bundle_id: &str) -> String {
    static NAMES: OnceLock<std::sync::Mutex<HashMap<String, String>>> = OnceLock::new();
    let names = NAMES.get_or_init(Default::default);

    if let Some(name) = names.lock().unwrap().get(bundle_id) {
        return name.clone();
    }
    let name = dictum_grammar::app_display_name(bundle_id);
    names
        .lock()
        .unwrap()
        .insert(bundle_id.to_string(), name.clone());
    name
}

async fn snapshot(state: &GrammarState) -> GrammarSettings {
    let config = state.config.lock().await;

    let mut dictionary = config
        .mutable_dictionary
        .words_iter()
        .map(|word| word.iter().collect::<String>())
        .collect::<Vec<_>>();
    dictionary.sort_by_key(|word| word.to_lowercase());

    let integrations = config.integrations.clone();
    let enabled = config.highlighter_service_enabled;
    let dialect = config.dialect.into();
    drop(config);

    let mut apps = tokio::task::spawn_blocking(move || {
        integrations
            .into_iter()
            .map(|integration| GrammarApp {
                name: app_name(&integration.bundle_id),
                bundle_id: integration.bundle_id,
                enabled: integration.enabled,
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    apps.sort_by_key(|app| app.name.to_lowercase());

    GrammarSettings {
        supported: dictum_grammar::is_supported(),
        enabled,
        running: state.service.is_running(),
        accessibility_granted: dictum_grammar::accessibility_granted(),
        dialect,
        dictionary,
        apps,
    }
}

/// Applies `change` to the config, persists it, and returns the updated settings.
///
/// The highlighter subprocess polls the config every second, so changes apply without a restart.
async fn update(
    state: &GrammarState,
    change: impl FnOnce(&mut Config),
) -> Result<GrammarSettings, String> {
    {
        let mut config = state.config.lock().await;
        change(&mut config);
        config.save_to_system().await.map_err(|e| e.to_string())?;
    }
    Ok(snapshot(state).await)
}

#[tauri::command]
#[specta::specta]
pub async fn get_grammar_settings(
    state: State<'_, GrammarState>,
) -> Result<GrammarSettings, String> {
    Ok(snapshot(&state).await)
}

#[tauri::command]
#[specta::specta]
pub async fn set_grammar_enabled(
    enabled: bool,
    state: State<'_, GrammarState>,
) -> Result<GrammarSettings, String> {
    update(&state, |config| {
        config.highlighter_service_enabled = enabled
    })
    .await?;

    if !enabled {
        state.service.stop();
    } else if dictum_grammar::accessibility_granted() {
        state.service.start().map_err(|e| e.to_string())?;
    }

    Ok(snapshot(&state).await)
}

#[tauri::command]
#[specta::specta]
pub async fn set_grammar_dialect(
    dialect: GrammarDialect,
    state: State<'_, GrammarState>,
) -> Result<GrammarSettings, String> {
    update(&state, |config| config.dialect = dialect.into()).await
}

#[tauri::command]
#[specta::specta]
pub async fn add_grammar_word(
    word: String,
    state: State<'_, GrammarState>,
) -> Result<GrammarSettings, String> {
    let word = word.trim().to_string();
    if word.is_empty() {
        return Err("Word cannot be empty".to_string());
    }

    update(&state, |config| {
        config
            .mutable_dictionary
            .append_word_str(&word, DictWordMetadata::default());
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_grammar_word(
    word: String,
    state: State<'_, GrammarState>,
) -> Result<GrammarSettings, String> {
    update(&state, |config| {
        let remaining = config
            .mutable_dictionary
            .words_iter()
            .filter(|chars| chars.iter().collect::<String>() != word)
            .map(|chars| (chars.to_vec(), DictWordMetadata::default()))
            .collect::<Vec<_>>();

        let mut dictionary = MutableDictionary::new();
        dictionary.extend_words(remaining);
        config.mutable_dictionary = dictionary;
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn set_grammar_app_enabled(
    bundle_id: String,
    enabled: bool,
    state: State<'_, GrammarState>,
) -> Result<GrammarSettings, String> {
    update(&state, |config| {
        config.add_integration(bundle_id.clone());
        config.set_integration_enabled(&bundle_id, enabled);
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_grammar_app(
    bundle_id: String,
    state: State<'_, GrammarState>,
) -> Result<GrammarSettings, String> {
    update(&state, |config| config.remove_integration(&bundle_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn search_grammar_apps(query: String) -> Result<Vec<GrammarApp>, String> {
    tokio::task::spawn_blocking(move || dictum_grammar::search_installed_apps(&query))
        .await
        .map_err(|e| format!("App search panicked: {e}"))?
        .map(|results| {
            results
                .into_iter()
                .map(|result| GrammarApp {
                    bundle_id: result.bundle_id,
                    name: result.name,
                    enabled: false,
                })
                .collect()
        })
}

#[tauri::command]
#[specta::specta]
pub async fn reset_grammar_dismissed(
    state: State<'_, GrammarState>,
) -> Result<GrammarSettings, String> {
    update(&state, |config| {
        config.ignored_lints = dictum_grammar::harper_core::IgnoredLints::new();
    })
    .await
}
