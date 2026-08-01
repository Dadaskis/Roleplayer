//! The composition root (§5.6 of AGENTS.md): wires every module together,
//! registers all Tauri commands, runs migrations, seeds defaults, and forwards
//! the event bus to the webview.
//!
//! This is the only place modules meet. Nothing below here knows about the
//! wiring — each module is an independent crate with its own commands.

use std::path::PathBuf;
use std::sync::Arc;

use roleplayer_campaigns::service::CampaignService;
use roleplayer_characters::service::CharacterService;
use roleplayer_core::errors::{AppError, ErrorDto};
use roleplayer_core::eventbus::EventBus;
use roleplayer_core::storage::Database;
use roleplayer_memories::service::MemoryService;
use roleplayer_providers::registry::ProviderRegistry;
use roleplayer_providers::service::ProviderService;
use roleplayer_rulesets::service::RulesetService;
use roleplayer_search::service::SearchService;
use roleplayer_turnflow::service::TurnService;
use roleplayer_world_state::service::WorldStateService;
use tauri::{Emitter, Manager, State};

/// Everything the commands and the app need, wired once at startup.
struct AppState {
    storage: Arc<Database>,
    bus: EventBus,
    campaigns: Arc<CampaignService<Database>>,
    characters: Arc<CharacterService<Database>>,
    rulesets: Arc<RulesetService<Database>>,
    world: Arc<WorldStateService<Database>>,
    providers: Arc<ProviderService<Database>>,
    registry: Arc<ProviderRegistry>,
    turnflow: Arc<TurnService<Database>>,
    memories: Arc<MemoryService<Database>>,
    search: Arc<SearchService<Database>>,
}

impl AppState {
    /// Open the database, seed defaults, and construct every service.
    fn bootstrap(
        data_dir: &std::path::Path,
        bus: EventBus,
    ) -> Result<AppState, Box<dyn std::error::Error>> {
        let storage =
            Arc::new(Database::open(&data_dir.join("roleplayer.db"))?);
        let registry = Arc::new(ProviderRegistry::new());

        let rulesets = Arc::new(RulesetService::new(storage.clone()));
        rulesets.ensure_default()?;

        let providers =
            Arc::new(ProviderService::new(storage.clone(), registry.clone()));
        providers.ensure_defaults()?;

        let campaigns = Arc::new(CampaignService::new(storage.clone()));
        let characters = Arc::new(CharacterService::new(storage.clone()));
        let world = Arc::new(WorldStateService::new(storage.clone()));

        let turnflow = Arc::new(TurnService::new(
            storage.clone(),
            world.clone(),
            campaigns.clone(),
            characters.clone(),
            rulesets.clone(),
            registry.clone(),
            bus.clone(),
        ));

        let memories =
            Arc::new(MemoryService::new(storage.clone(), registry.clone()));
        let search = Arc::new(SearchService::new(storage.clone()));

        Ok(AppState {
            storage,
            bus,
            campaigns,
            characters,
            rulesets,
            world,
            providers,
            registry,
            turnflow,
            memories,
            search,
        })
    }
}

/// The app's data directory: `%APPDATA%\Roleplayer` (Windows) or the platform
/// equivalent elsewhere. Contains the DB, `logs/`, and future exports.
fn data_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let project_dirs =
        directories::ProjectDirs::from("com", "Roleplayer", "Roleplayer")
            .ok_or("could not resolve the platform data directory")?;
    let dir = project_dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    // Pre-create `logs/` here (not in init_logging) so the data dir is fully
    // scaffolded before logging or the panic hook try to write into it.
    let logs_dir = dir.join("logs");
    std::fs::create_dir_all(&logs_dir)?;
    Ok(dir)
}

/// Initialise `tracing` with a size-rotated file in `logs/` plus stdout in
/// debug builds (§5.13 of AGENTS.md).
///
/// tracing-appender 0.2 only rotates by time, so size rotation is done manually
/// here at startup: when the current log exceeds [`LOG_MAX_BYTES`], older files
/// are shifted (`roleplayer.log` -> `.1` -> `.2` -> `.3`) before we append.
fn init_logging(data_dir: &std::path::Path) {
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    use tracing_subscriber::EnvFilter;

    // Honour `RUST_LOG` when set (a way to debug a packaged app), and fall
    // back to info level otherwise so the default logs stay useful (§5.13).
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let logs_dir = data_dir.join("logs");
    install_panic_hook(&logs_dir);
    rotate_logs(&logs_dir);

    let log_path = logs_dir.join("roleplayer.log");
    let log_file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(file) => file,
        Err(_error) => {
            // Fall back to stdout-only rather than failing startup.
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stdout)
                .init();
            return;
        }
    };

    // In debug, tee to the console; in release, file-only.
    // `and` (MakeWriterExt) writes each line to both targets at once; in
    // release the windows subsystem has no console, so the stdout half of
    // the tee is a silent no-op and the file is the only effective sink.
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(log_file.and(std::io::stdout));
    subscriber.init();
}

/// Max size of the current log before it is rotated at next startup.
const LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

/// Install a panic hook that records every panic to `logs/panic.log`.
///
/// Without this, a Rust panic only reaches stderr — invisible in a packaged
/// Windows GUI app, and a non-unwinding panic aborts with no crash dump (that
/// is exactly how the app died earlier: `tokio::spawn` without a runtime, from
/// a WebView2 COM callback). The hook runs for **both** unwinding and aborting
/// panics, so nothing slips past the file, which agents can read directly.
fn install_panic_hook(logs_dir: &std::path::Path) {
    use std::io::Write;

    let panic_log_path = logs_dir.join("panic.log");
    tracing::info!("panic hook will log to {}", panic_log_path.display());
    // Take the current hook first so it can be chained at the end — the
    // default stderr output stays intact in dev instead of being replaced.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Panic payloads are `&str` or `String` in practice; try both so the
        // message is never lost, and fall back to a fixed string if the
        // payload is some other type.
        let message = panic_info
            .payload()
            .downcast_ref::<&str>()
            .map(|payload| (*payload).to_string())
            .or_else(|| panic_info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic with non-string payload".to_string());
        let location = panic_info
            .location()
            .map(|location| location.to_string())
            .unwrap_or_else(|| "unknown location".to_string());
        // Force capture: the backtrace is recorded even without RUST_BACKTRACE.
        let backtrace = std::backtrace::Backtrace::force_capture();
        let timestamp = roleplayer_core::now_rfc3339();

        // Append, never truncate: panic.log must accumulate across runs so a
        // crash can be diagnosed afterwards from the whole history.
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&panic_log_path)
        {
            // A panic hook must never panic, and the default hook below still
            // reports to stderr, so a failed write is deliberately swallowed.
            let _ = writeln!(
                file,
                "=== PANIC {timestamp} ===\nlocation: {location}\nmessage: {message}\n{backtrace}\n"
            );
        }
        // Keep the default hook so stderr still shows the panic in dev.
        default_hook(panic_info);
    }));
}

/// Shift the log chain: `.log` -> `.1` -> `.2` -> `.3`, dropping `.3`, but only
/// when the current file has outgrown [`LOG_MAX_BYTES`].
fn rotate_logs(logs_dir: &std::path::Path) {
    let current = logs_dir.join("roleplayer.log");
    // A missing file reads as size 0 (`unwrap_or(0)`), i.e. "nothing to
    // rotate", which keeps the very first run error-free.
    let current_size =
        std::fs::metadata(&current).map(|meta| meta.len()).unwrap_or(0);
    if current_size < LOG_MAX_BYTES {
        return;
    }
    // Remove the oldest slot, then shift newer files down one slot.
    // Order matters: `.3` must be freed before `.2 -> .3` renames into it.
    // Every step is best-effort (`let _`): a missing intermediate is normal
    // on the first rotation, and a partial chain is an acceptable state.
    let _ = std::fs::remove_file(logs_dir.join("roleplayer.log.3"));
    let _ = std::fs::rename(
        logs_dir.join("roleplayer.log.2"),
        logs_dir.join("roleplayer.log.3"),
    );
    let _ = std::fs::rename(
        logs_dir.join("roleplayer.log.1"),
        logs_dir.join("roleplayer.log.2"),
    );
    let _ = std::fs::rename(&current, logs_dir.join("roleplayer.log.1"));
    tracing::info!("rotated logs: current file exceeded {LOG_MAX_BYTES} bytes");
}

/// Command: open (or focus) the debug window for a campaign.
///
/// The debug data lives in a real second window, not an in-app panel, so the
/// world/memory/audit panels get room to breathe without shrinking the chat.
#[tauri::command]
fn open_debug_window(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    campaign_id: String,
) -> Result<(), ErrorDto> {
    // Only open a window for a campaign that actually exists; a bogus id
    // must not spawn a window pointing at nothing.
    if state.campaigns.get(&campaign_id).map_err(ErrorDto::from)?.is_none() {
        return Err(ErrorDto::from(AppError::Domain(format!(
            "campaign not found: {campaign_id}"
        ))));
    }

    // One window per campaign; a repeated click focuses the existing one
    // instead of panicking on a duplicate window label. A window that is
    // still mid-destruction may also be found here — focusing it is harmless
    // (the set_focus error is swallowed) and the next click opens a fresh one.
    let label = format!("debug-{campaign_id}");
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.set_focus();
        return Ok(());
    }

    // WebviewUrl::App resolves to the dev server in dev and the bundled dist
    // in release — the same resolution the main window uses, so the hash
    // route (`#/debug/<id>`) is picked up by main.tsx in both environments.
    tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(
            format!("index.html#/debug/{campaign_id}").into(),
        ),
    )
    .title("Roleplayer — Debug")
    // A wide window: the whole point of a pop-out is room for the panels.
    .inner_size(1000.0, 720.0)
    .build()
    // Window-creation failures (rare: OS limits) are IPC errors, not panics.
    .map_err(|error| ErrorDto::from(AppError::Ipc(error.to_string())))?;
    Ok(())
}

/// Forward every event bus message to the webview as `turn-event`.
fn forward_events(app: &tauri::App, bus: EventBus) {
    let handle = app.handle().clone();
    // Use Tauri's runtime handle, not a bare `tokio::spawn`: the app's
    // runtime is created lazily, so `tokio::spawn` from here could panic
    // with "no reactor running" — the exact crash the hook above records.
    tauri::async_runtime::spawn(async move {
        let mut receiver = bus.subscribe();
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    // A webview that closed is a valid quiet state.
                    let _ = handle.emit("turn-event", event);
                }
                // Lagged means the UI is slow; drop old events, keep going.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(
                    _lagged,
                )) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Desktop entrypoint — called from `main.rs`.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = match data_dir() {
        Ok(dir) => dir,
        Err(error) => {
            // Logging is not up yet, so a bare eprintln is the only safe
            // channel for a fatal that happens before the subscriber exists.
            eprintln!("fatal: could not resolve data directory: {error}");
            return;
        }
    };
    init_logging(&data_dir);

    // `.run` builds the app and starts the event loop; it returns `Err` only
    // on catastrophic runtime failure. `generate_context!` is called exactly
    // once — it embeds `tauri.conf.json` at compile time.
    let result = tauri::Builder::default()
        .setup(move |app| {
            let bus = EventBus::new();
            // Re-box through a String so the `?` yields a `Box<dyn Error>`
            // that is guaranteed Send + Sync and carries a stable message;
            // this is a startup-path failure reported once, so nothing but
            // the message needs to survive.
            let state = AppState::bootstrap(&data_dir, bus.clone()).map_err(
                |error| Box::<dyn std::error::Error>::from(error.to_string()),
            )?;

            // Each service is managed by its concrete Arc type so the module
            // commands can resolve their `State` arguments.
            app.manage(state.storage.clone());
            app.manage(state.campaigns.clone());
            app.manage(state.characters.clone());
            app.manage(state.rulesets.clone());
            app.manage(state.world.clone());
            app.manage(state.providers.clone());
            app.manage(state.registry.clone());
            app.manage(state.turnflow.clone());
            app.manage(state.memories.clone());
            app.manage(state.search.clone());
            app.manage(state.bus.clone());
            // Managing the whole `AppState` too is belt-and-braces: module
            // commands resolve the concrete services managed above, and this
            // keeps `AppState` itself registered for any in-crate handler
            // that wants it as a `State` argument.
            app.manage(state);

            tracing::info!("Roleplayer started");
            forward_events(app, bus);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_debug_window,
            roleplayer_campaigns::commands::list_campaigns,
            roleplayer_campaigns::commands::create_campaign,
            roleplayer_campaigns::commands::get_campaign,
            roleplayer_campaigns::commands::update_campaign,
            roleplayer_campaigns::commands::delete_campaign,
            roleplayer_characters::commands::list_characters,
            roleplayer_characters::commands::create_character,
            roleplayer_characters::commands::get_character,
            roleplayer_characters::commands::update_character,
            roleplayer_characters::commands::delete_character,
            roleplayer_rulesets::commands::list_rulesets,
            roleplayer_rulesets::commands::get_ruleset,
            roleplayer_rulesets::commands::create_ruleset,
            roleplayer_rulesets::commands::update_ruleset,
            roleplayer_rulesets::commands::delete_ruleset,
            roleplayer_world_state::commands::get_world_state,
            roleplayer_world_state::commands::set_world_key,
            roleplayer_world_state::commands::remove_world_key,
            roleplayer_world_state::commands::list_state_changes,
            roleplayer_turnflow::commands::send_turn,
            roleplayer_turnflow::commands::cancel_turn,
            roleplayer_turnflow::commands::list_messages,
            roleplayer_providers::commands::list_providers,
            roleplayer_providers::commands::list_models,
            roleplayer_providers::commands::set_provider_config,
            roleplayer_providers::commands::set_provider_api_key,
            roleplayer_providers::commands::clear_provider_api_key,
            roleplayer_providers::commands::set_default_provider,
            roleplayer_providers::commands::test_provider,
            roleplayer_memories::commands::list_memories,
            roleplayer_memories::commands::create_memory,
            roleplayer_memories::commands::delete_memory,
            roleplayer_memories::commands::summarize_memory,
            roleplayer_search::commands::search_messages,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        tracing::error!(%error, "application exited with an error");
    }
}
