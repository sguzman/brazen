use crate::app::ShellState;
use crate::engine::{BrowserEngine, EngineEvent};
use crate::navigation::normalize_url_input;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppCommand {
    NavigateTo(String),
    ReloadActiveTab,
    StopLoading,
    GoBack,
    GoForward,
    ToggleLogPanel,
    OpenPermissionPanel,
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
                if state.log_panel_open {
                    "opened"
                } else {
                    "closed"
                }
            ));
            CommandOutcome::LogPanelVisibility(state.log_panel_open)
        }
        AppCommand::OpenPermissionPanel => {
            state.permission_panel_open = true;
            state.record_event("permission panel opened");
            CommandOutcome::PermissionPanelVisibility(true)
        }
    }
}
