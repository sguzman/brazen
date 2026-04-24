use crate::app::ShellState;
use crate::engine::{BrowserEngine, EngineEvent, EngineFrame};
use crate::navigation::normalize_url_input;
use tokio::sync::oneshot;

pub type CommandResult<T> = Result<T, String>;

#[derive(Debug)]
pub enum AppCommand {
    NavigateTo(String),
    ReloadActiveTab,
    StopLoading,
    GoBack,
    GoForward,
    ToggleLogPanel,
    OpenPermissionPanel,
    // Automation variants
    DomQuery {
        selector: String,
        response_tx: oneshot::Sender<CommandResult<serde_json::Value>>,
    },
    ScreenshotTab {
        response_tx: oneshot::Sender<CommandResult<EngineFrame>>,
    },
    ScreenshotWindow {
        response_tx: oneshot::Sender<CommandResult<EngineFrame>>,
    },
    EvaluateJavascript {
        script: String,
        response_tx: oneshot::Sender<CommandResult<serde_json::Value>>,
    },
    GetRenderedText {
        response_tx: oneshot::Sender<CommandResult<String>>,
    },
    GetArticleText {
        response_tx: oneshot::Sender<CommandResult<String>>,
    },
    InteractDom {
        selector: String,
        event: String,
        value: Option<String>,
        response_tx: oneshot::Sender<CommandResult<()>>,
    },
    TtsPause,
    TtsResume,
    TtsStop,
    TtsEnqueue(String),
    OpenReaderMode(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    NavigationScheduled,
    NavigationFailed,
    ReloadScheduled,
    StopScheduled,
    BackScheduled,
    ForwardScheduled,
    LogPanelVisibility(bool),
    PermissionPanelVisibility(bool),
    AutomationCommandQueued,
}

pub fn dispatch_command(
    state: &mut ShellState,
    engine: &mut dyn BrowserEngine,
    command: AppCommand,
) -> CommandOutcome {
    match command {
        AppCommand::NavigateTo(url) => match normalize_url_input(&url) {
            Ok(normalized) => {
                state.address_bar_input = normalized.clone();
                state.record_event(format!("queued navigation to {normalized}"));
                engine.navigate(&normalized);
                CommandOutcome::NavigationScheduled
            }
            Err(reason) => {
                state.record_event(format!("navigation failed: {url} ({reason})"));
                engine.inject_event(EngineEvent::NavigationFailed { input: url, reason });
                CommandOutcome::NavigationFailed
            }
        },
        AppCommand::ReloadActiveTab => {
            state.record_event("queued reload for active tab");
            engine.reload();
            CommandOutcome::ReloadScheduled
        }
        AppCommand::StopLoading => {
            state.record_event("queued stop for active tab");
            engine.stop();
            CommandOutcome::StopScheduled
        }
        AppCommand::GoBack => {
            state.record_event("queued back navigation");
            engine.go_back();
            CommandOutcome::BackScheduled
        }
        AppCommand::GoForward => {
            state.record_event("queued forward navigation");
            engine.go_forward();
            CommandOutcome::ForwardScheduled
        }
        AppCommand::ToggleLogPanel => {
            state.log_panel_open = !state.log_panel_open;
            state.record_event(format!(
                "log panel {}",
                if state.log_panel_open { "opened" } else { "closed" }
            ));
            CommandOutcome::LogPanelVisibility(state.log_panel_open)
        }
        AppCommand::OpenPermissionPanel => {
            state.permission_panel_open = true;
            state.record_event("permission panel opened");
            CommandOutcome::PermissionPanelVisibility(true)
        }
        AppCommand::DomQuery { selector, response_tx } => {
            engine.evaluate_javascript(
                format!(
                    "document.querySelector('{}') ? document.querySelector('{}').outerHTML : null",
                    selector, selector
                ),
                Box::new(|result| {
                    let _ = response_tx.send(result);
                }),
            );
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::ScreenshotTab { response_tx } => {
            let _ = response_tx.send(engine.take_screenshot());
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::ScreenshotWindow { response_tx } => {
            let _ = response_tx.send(engine.take_screenshot());
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::EvaluateJavascript { script, response_tx } => {
            engine.evaluate_javascript(
                script,
                Box::new(|result| {
                    let _ = response_tx.send(result);
                }),
            );
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::GetRenderedText { response_tx } => {
            engine.evaluate_javascript(
                "document.body.innerText".to_string(),
                Box::new(|result| {
                    let _ = response_tx.send(result.map(|v| {
                        if let serde_json::Value::String(s) = v { s } else { v.to_string() }
                    }));
                }),
            );
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::GetArticleText { response_tx } => {
            engine.evaluate_javascript(
                "(document.querySelector('article') || document.querySelector('main') || document.body).innerText".to_string(),
                Box::new(|result| {
                    let _ = response_tx.send(result.map(|v| {
                        if let serde_json::Value::String(s) = v { s } else { v.to_string() }
                    }));
                }),
            );
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::InteractDom { selector, event, value, response_tx } => {
            engine.interact_dom(selector, event, value, Box::new(|result| {
                let _ = response_tx.send(result);
            }));
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::TtsPause => {
            state.tts_playing = false;
            state.record_event("tts paused");
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::TtsResume => {
            state.tts_playing = true;
            state.record_event("tts resumed");
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::TtsStop => {
            state.tts_playing = false;
            state.tts_queue.clear();
            state.record_event("tts stopped");
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::TtsEnqueue(text) => {
            state.tts_queue.push_back(text);
            state.record_event("tts enqueued");
            CommandOutcome::AutomationCommandQueued
        }
        AppCommand::OpenReaderMode(url) => {
            state.reader_mode_open = true;
            state.reader_mode_source_url = Some(url);
            state.record_event("reader mode opened");
            CommandOutcome::AutomationCommandQueued
        }
    }
}
