//! Live, system-wide grammar and spelling highlights powered by Harper.
//!
//! Most of this module is ported from Harper Desktop (Apache-2.0, see HARPER-LICENSE):
//! https://github.com/Automattic/harper/tree/v2.11.0/harper-desktop
//!
//! The highlighter runs as a subprocess of the main app (the same executable launched with
//! [`HIGHLIGHTER_ARG`]) and talks to it over stdin/stdout. The parent owns the persisted
//! [`config::Config`]; the subprocess owns the accessibility polling and the overlay windows.

pub mod color;
pub mod communication;
pub mod config;
mod debounce;
pub mod highlighter;
pub mod highlighter_service;
pub mod lint_kind_color;
pub mod os_broker;
pub mod rect;

#[cfg(target_os = "macos")]
mod mac_broker;

#[cfg(target_os = "windows")]
mod windows_broker;

pub use harper_core;
pub use os_broker::{AccessibilityPermissionStatus, AppSearchResult};

use self::communication::{Client, ProtocolError};
use self::config::{Config, Integration};
use self::debounce::{DebounceState, DebounceStatus};
use self::highlighter::Highlighter;
use self::os_broker::OsBroker;
use harper_core::{
    Dialect, DictWordMetadata, Document, IgnoredLints,
    linting::{Lint, LintGroup},
    spell::MutableDictionary,
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex as StdMutex},
};
use tokio::{
    io::{Stdin, Stdout},
    runtime::{Builder, Runtime},
};

/// Command-line flag that turns the app executable into the highlighter subprocess.
pub const HIGHLIGHTER_ARG: &str = "--grammar-highlighter";

/// Whether live highlighting has an OS integration on this platform (macOS and Windows).
pub const fn is_supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "windows"))
}

/// Whether the app may read and edit other apps' text through the OS accessibility API.
pub fn accessibility_granted() -> bool {
    platform_broker(Arc::default()).accessibility_permission_status()
        == AccessibilityPermissionStatus::Granted
}

/// Installed applications whose name or bundle ID matches `query` (all of them when empty).
pub fn search_installed_apps(query: &str) -> Result<Vec<AppSearchResult>, String> {
    platform_broker(Arc::default()).search_apps(query)
}

/// Human-readable name for an application identifier, falling back to the identifier itself.
pub fn app_display_name(bundle_id: &str) -> String {
    platform_broker(Arc::default()).integration_display_name(bundle_id)
}

/// Whether this process was launched as the highlighter subprocess.
///
/// Checked in `main` before CLI parsing and single-instance handling, like the transcription
/// worker, so the subprocess never touches Tauri.
pub fn is_highlighter_invocation() -> bool {
    std::env::args().nth(1).as_deref() == Some(HIGHLIGHTER_ARG)
}

#[cfg(target_os = "macos")]
pub(crate) type PlatformBroker = self::mac_broker::MacBroker;

#[cfg(target_os = "windows")]
pub(crate) type PlatformBroker = self::windows_broker::WindowsBroker;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) type PlatformBroker = self::os_broker::NoopBroker;

/// Creates the process-local platform broker.
///
/// The Tauri process stores its single broker as managed state, while the highlighter subprocess
/// creates its own broker because it is a separate process.
fn platform_broker(integrations: Arc<StdMutex<Vec<Integration>>>) -> PlatformBroker {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        PlatformBroker::new(integrations)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = integrations;
        PlatformBroker::default()
    }
}

/// Run as a highlighter process.
/// Can configure whether to run standalone, or with a parent Tauri process
pub fn run_highlighter(has_parent: bool) {
    let client = Rc::new(RefCell::new(Client::current_process()));
    let sync_runtime = Rc::new(
        Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build highlighter protocol runtime"),
    );

    let startup_config = if has_parent {
        fetch_highlighter_config(&mut client.borrow_mut(), &sync_runtime)
    } else {
        Ok(Config::default())
    };

    let startup_config = match startup_config {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to hydrate highlighter config, using defaults: {error}");
            Config::new()
        }
    };

    let startup_linter = startup_config.create_linter();
    let ignored_lints = Rc::new(RefCell::new(startup_config.ignored_lints));
    let user_dictionary = Rc::new(RefCell::new(startup_config.mutable_dictionary));
    let dialect = Rc::new(RefCell::new(startup_config.dialect));
    let integrations = Arc::new(StdMutex::new(startup_config.integrations));
    let debounce_ms = Rc::new(RefCell::new(startup_config.debounce_ms));
    let linter = Rc::new(RefCell::new(startup_linter));

    let broker = platform_broker(integrations.clone());

    let lint_ignored_lints = ignored_lints.clone();
    let lint_linter = linter.clone();
    let lint_user_dictionary = user_dictionary.clone();
    let lint_debounce_ms = debounce_ms.clone();
    let lint_debounce_state = Rc::new(RefCell::new(DebounceState::default()));

    let ignore_client = client.clone();
    let ignore_runtime = sync_runtime.clone();
    let ignore_ignored_lints = ignored_lints.clone();

    let dictionary_client = client.clone();
    let dictionary_runtime = sync_runtime.clone();
    let dictionary_user_dictionary = user_dictionary.clone();
    let dictionary_linter = linter.clone();
    let dictionary_dialect = dialect.clone();
    let dictionary_debounce_ms = debounce_ms.clone();

    let disable_client = client.clone();
    let disable_runtime = sync_runtime.clone();
    let disable_linter = linter.clone();

    let refresh_client = client.clone();
    let refresh_runtime = sync_runtime.clone();
    let refresh_ignored_lints = ignored_lints.clone();
    let refresh_user_dictionary = user_dictionary.clone();
    let refresh_dialect = dialect.clone();
    let refresh_integrations = integrations.clone();
    let refresh_debounce_ms = debounce_ms.clone();
    let refresh_linter = linter.clone();

    let lint_text = move |text: &str| {
        let debounce_ms = *lint_debounce_ms.borrow();
        let mut debounce_state = lint_debounce_state.borrow_mut();

        match debounce_state.status(text, debounce_ms) {
            DebounceStatus::Cached(lints) => return lints,
            DebounceStatus::Ready => {}
        }

        let dictionary =
            Config::dictionary_from_user_dictionary(lint_user_dictionary.borrow().clone());
        let doc = Document::new_markdown_default(text, &dictionary);
        let mut organized_lints = lint_linter.borrow_mut().organized_lints(&doc);

        for lints in organized_lints.values_mut() {
            lint_ignored_lints.borrow().remove_ignored(lints, &doc);
        }

        debounce_state.store_lints(text, debounce_ms, &organized_lints);

        organized_lints
    };

    let ignore_lint = move |lint: &Lint, document: &Document| {
        {
            ignore_ignored_lints
                .borrow_mut()
                .ignore_lint(lint, document);
        }

        let snapshot = ignore_ignored_lints.borrow().clone();
        if let Err(error) =
            ignore_runtime.block_on(ignore_client.borrow_mut().ignore_lint(&snapshot))
        {
            eprintln!("failed to sync ignored lints: {error}");
        }
    };

    let add_to_dictionary = move |word: &str| {
        dictionary_user_dictionary
            .borrow_mut()
            .append_word_str(word, DictWordMetadata::default());

        let lint_config = dictionary_linter.borrow().config.clone();
        let config = Config {
            mutable_dictionary: dictionary_user_dictionary.borrow().clone(),
            dialect: *dictionary_dialect.borrow(),
            ignored_lints: IgnoredLints::new(),
            lint_config,
            integrations: Vec::new(),
            onboarding_completed: false,
            debounce_ms: *dictionary_debounce_ms.borrow(),
            auto_update: true,
            last_update_check: None,
            highlighter_service_enabled: true,
        };
        *dictionary_linter.borrow_mut() = config.create_linter();

        if let Err(error) =
            dictionary_runtime.block_on(dictionary_client.borrow_mut().add_to_dictionary(word))
        {
            eprintln!("failed to sync dictionary update: {error}");
        }
    };

    let disable_rule = move |rule_name: &str| match disable_runtime
        .block_on(disable_client.borrow_mut().disable_rule(rule_name))
    {
        Ok(config) => disable_linter.borrow_mut().config = config,
        Err(error) => eprintln!("failed to disable rule {rule_name}: {error}"),
    };

    let refresh_config = move || {
        if !has_parent {
            return;
        }

        match fetch_highlighter_config(&mut refresh_client.borrow_mut(), &refresh_runtime) {
            Ok(config) => apply_highlighter_config(
                config,
                &refresh_ignored_lints,
                &refresh_user_dictionary,
                &refresh_dialect,
                &refresh_integrations,
                &refresh_debounce_ms,
                &refresh_linter,
            ),
            Err(error) => {
                eprintln!("failed to refresh highlighter config: {error}");
                std::process::exit(1);
            }
        }
    };

    if let Err(error) = Highlighter::new(
        broker,
        lint_text,
        ignore_lint,
        add_to_dictionary,
        disable_rule,
        refresh_config,
    )
    .and_then(Highlighter::run_window_for_each_monitor)
    {
        eprintln!("failed to run highlighter: {error}");
    }
}

fn fetch_highlighter_config(
    client: &mut Client<Stdin, Stdout>,
    runtime: &Runtime,
) -> Result<Config, ProtocolError> {
    runtime.block_on(async {
        let dialect = client.get_dialect().await?;
        let mutable_dictionary = client.get_dictionary().await?;
        let ignored_lints = client.get_ignored_lints().await?;
        let lint_config = client.get_lint_config().await?;
        let integrations = client.get_integrations().await?;
        let debounce_ms = client.get_debounce_ms().await?;

        Ok(Config {
            dialect,
            mutable_dictionary,
            ignored_lints,
            lint_config,
            integrations,
            onboarding_completed: false,
            debounce_ms,
            auto_update: true,
            last_update_check: None,
            highlighter_service_enabled: true,
        })
    })
}

fn apply_highlighter_config(
    config: Config,
    ignored_lints: &Rc<RefCell<IgnoredLints>>,
    user_dictionary: &Rc<RefCell<MutableDictionary>>,
    dialect: &Rc<RefCell<Dialect>>,
    integrations: &Arc<StdMutex<Vec<Integration>>>,
    debounce_ms: &Rc<RefCell<u64>>,
    linter: &Rc<RefCell<LintGroup>>,
) {
    let linter_config = config.create_linter();
    *ignored_lints.borrow_mut() = config.ignored_lints;
    *user_dictionary.borrow_mut() = config.mutable_dictionary;
    *dialect.borrow_mut() = config.dialect;
    match integrations.lock() {
        Ok(mut integrations) => *integrations = config.integrations,
        Err(error) => eprintln!("failed to update integrations: {error}"),
    }
    *debounce_ms.borrow_mut() = config.debounce_ms;
    *linter.borrow_mut() = linter_config;
}
