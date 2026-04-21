use base64::Engine;
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

use crate::config::BrazenConfig;
use crate::platform_paths::PlatformPaths;

#[derive(Parser)]
#[command(name = "introspect", about = "Introspect and audit a running Brazen instance")]
pub struct IntrospectArgs {
    #[command(subcommand)]
    pub command: IntrospectCommand,

    /// Override the automation server URL
    #[arg(short, long)]
    pub url: Option<String>,

    /// Authentication token
    #[arg(short, long)]
    pub token: Option<String>,
}

#[derive(Subcommand)]
pub enum IntrospectCommand {
    /// List all open windows
    ListWindows,
    /// List all tabs in the active window
    ListTabs,
    /// Get a full automation snapshot of the running instance
    Snapshot,
    /// Get the DOM of the active tab
    GetDom {
        /// CSS selector to retrieve
        #[arg(short, long, default_value = "body")]
        selector: String,
    },
    /// Take a screenshot of the active tab
    Screenshot {
        /// Output path for the screenshot
        #[arg(short, long, default_value = "screenshot.png")]
        output: String,
    },
    /// Stream live browser logs
    Logs {
        /// Follow logs in real-time
        #[arg(short, long)]
        follow: bool,
    },
    /// Evaluate JavaScript in the active tab
    EvaluateJs {
        /// JavaScript code to execute
        script: String,
    },
    /// Request the running instance to shut down
    Shutdown,
    /// Create a new profile (directory + state db)
    ProfileCreate {
        /// Profile id/name
        id: String,
    },
    /// Switch to a profile id/name (persisted to config file)
    ProfileSwitch {
        /// Profile id/name
        id: String,
    },
    /// Export a profile bundle (.tar.gz)
    ProfileExport {
        id: String,
        #[arg(short, long)]
        output: String,
        #[arg(long)]
        include_cache_blobs: bool,
    },
    /// Import a profile bundle (.tar.gz)
    ProfileImport {
        id: String,
        #[arg(short, long)]
        input: String,
        #[arg(long)]
        overwrite: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct AutomationEnvelope<T> {
    pub id: Option<String>,
    #[serde(flatten)]
    pub payload: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum AutomationRequest {
    WindowList,
    LogSubscribe,
    TabList,
    Snapshot,
    DomQuery { selector: String },
    Screenshot,
    EvaluateJavascript { script: String },
    Shutdown,
    ProfileCreate { profile_id: String },
    ProfileSwitch { profile_id: String },
    ProfileExport { profile_id: String, output_path: String, include_cache_blobs: Option<bool> },
    ProfileImport { profile_id: String, input_path: String, overwrite: Option<bool> },
}

#[derive(Debug, Serialize, Deserialize)]
struct AutomationResponse<T> {
    id: Option<String>,
    ok: bool,
    result: Option<T>,
    error: Option<String>,
}

pub async fn run_introspect_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut actual_args = vec!["introspect".to_string()];
    actual_args.extend_from_slice(args);
    let cli = IntrospectArgs::parse_from(actual_args);

    let platform = PlatformPaths::detect()?;
    let config_path = platform.default_config_path();
    let config = BrazenConfig::load_with_defaults(&config_path)?;

    let url_str = cli.url.unwrap_or_else(|| config.automation.bind.clone());
    let mut url = Url::parse(&url_str)?;
    
    let token = cli.token.or(config.automation.auth_token.clone());
    if let Some(token) = token {
        url.query_pairs_mut().append_pair("token", &token);
    }

    let (ws_stream, _) = connect_async(url.as_str()).await.map_err(|e| format!("Failed to connect to automation server at {}: {}", url_str, e))?;
    let (mut write, mut read) = ws_stream.split();

    match cli.command {
        IntrospectCommand::ListWindows => {
            let request = AutomationEnvelope {
                id: Some("cli-windows".to_string()),
                payload: AutomationRequest::WindowList,
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    println!("{}", serde_json::to_string_pretty(&response.result)?);
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_else(|| "Unknown error".to_string()));
                }
            }
        }
        IntrospectCommand::ListTabs => {
            let request = AutomationEnvelope {
                id: Some("cli-tabs".to_string()),
                payload: AutomationRequest::TabList,
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    if let Some(tabs) = response.result {
                        println!("{:<5} {:<30} {}", "IDX", "TITLE", "URL");
                        if let Some(tab_array) = tabs.as_array() {
                            for tab in tab_array {
                                println!("{:<5} {:<30} {}", 
                                    tab["index"], 
                                    tab["title"].as_str().unwrap_or(""), 
                                    tab["url"].as_str().unwrap_or("")
                                );
                            }
                        }
                    }
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_else(|| "Unknown error".to_string()));
                }
            }
        }
        IntrospectCommand::Snapshot => {
            let request = AutomationEnvelope {
                id: Some("cli-snapshot".to_string()),
                payload: AutomationRequest::Snapshot,
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;

            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    println!("{}", serde_json::to_string_pretty(&response.result)?);
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_default());
                }
            }
        }
        IntrospectCommand::GetDom { selector } => {
            let request = AutomationEnvelope {
                id: Some("cli-dom".to_string()),
                payload: AutomationRequest::DomQuery { selector },
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<String> = serde_json::from_str(&text)?;
                if response.ok {
                    println!("{}", response.result.unwrap_or_default());
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_else(|| "Unknown error".to_string()));
                }
            }
        }
        IntrospectCommand::Screenshot { output } => {
            let request = AutomationEnvelope {
                id: Some("cli-screenshot".to_string()),
                payload: AutomationRequest::Screenshot,
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<String> = serde_json::from_str(&text)?;
                if response.ok {
                    if let Some(base64_data) = response.result {
                        let bytes = base64::engine::general_purpose::STANDARD.decode(base64_data)?;
                        std::fs::write(&output, bytes)?;
                        println!("Screenshot saved to {}", output);
                    }
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_else(|| "Unknown error".to_string()));
                }
            }
        }
        IntrospectCommand::Logs { follow } => {
            let request = AutomationEnvelope {
                id: Some("cli-logs".to_string()),
                payload: AutomationRequest::LogSubscribe,
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            
            // Wait for subscription confirmation
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if !response.ok {
                    eprintln!("Failed to subscribe to logs: {}", response.error.unwrap_or_default());
                    return Ok(());
                }
                println!("Subscribed to log stream...");
            }

            while let Some(Ok(Message::Text(text))) = read.next().await {
                if let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&text) {
                    if envelope["type"] == "log-entry" {
                        println!("{}", envelope["message"].as_str().unwrap_or(""));
                    }
                }
                if !follow { break; }
            }
        }
        IntrospectCommand::EvaluateJs { script } => {
            let request = AutomationEnvelope {
                id: Some("cli-eval-js".to_string()),
                payload: AutomationRequest::EvaluateJavascript { script },
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    if let Some(result) = response.result {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    }
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_else(|| "Unknown error".to_string()));
                }
            }
        }
        IntrospectCommand::Shutdown => {
            let request = AutomationEnvelope {
                id: Some("cli-shutdown".to_string()),
                payload: AutomationRequest::Shutdown,
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;

            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    println!("Shutdown requested");
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_default());
                }
            }
        }
        IntrospectCommand::ProfileCreate { id } => {
            let request = AutomationEnvelope {
                id: Some("cli-profile-create".to_string()),
                payload: AutomationRequest::ProfileCreate { profile_id: id },
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    println!("{}", serde_json::to_string_pretty(&response.result)?);
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_default());
                }
            }
        }
        IntrospectCommand::ProfileSwitch { id } => {
            let request = AutomationEnvelope {
                id: Some("cli-profile-switch".to_string()),
                payload: AutomationRequest::ProfileSwitch { profile_id: id },
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    println!("{}", serde_json::to_string_pretty(&response.result)?);
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_default());
                }
            }
        }
        IntrospectCommand::ProfileExport { id, output, include_cache_blobs } => {
            let request = AutomationEnvelope {
                id: Some("cli-profile-export".to_string()),
                payload: AutomationRequest::ProfileExport {
                    profile_id: id,
                    output_path: output,
                    include_cache_blobs: Some(include_cache_blobs),
                },
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    println!("{}", serde_json::to_string_pretty(&response.result)?);
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_default());
                }
            }
        }
        IntrospectCommand::ProfileImport { id, input, overwrite } => {
            let request = AutomationEnvelope {
                id: Some("cli-profile-import".to_string()),
                payload: AutomationRequest::ProfileImport {
                    profile_id: id,
                    input_path: input,
                    overwrite: Some(overwrite),
                },
            };
            write.send(Message::Text(serde_json::to_string(&request)?.into())).await?;
            if let Some(Ok(Message::Text(text))) = read.next().await {
                let response: AutomationResponse<serde_json::Value> = serde_json::from_str(&text)?;
                if response.ok {
                    println!("{}", serde_json::to_string_pretty(&response.result)?);
                } else {
                    eprintln!("Error: {}", response.error.unwrap_or_default());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clap_parses_list_windows() {
        let cli = IntrospectArgs::parse_from(["introspect", "list-windows"]);
        assert!(matches!(cli.command, IntrospectCommand::ListWindows));
    }

    #[test]
    fn clap_parses_list_tabs() {
        let cli = IntrospectArgs::parse_from(["introspect", "list-tabs"]);
        assert!(matches!(cli.command, IntrospectCommand::ListTabs));
    }

    #[test]
    fn clap_parses_logs_follow() {
        let cli = IntrospectArgs::parse_from(["introspect", "logs", "--follow"]);
        assert!(matches!(cli.command, IntrospectCommand::Logs { follow: true }));
    }

    #[test]
    fn clap_parses_get_dom_selector() {
        let cli = IntrospectArgs::parse_from(["introspect", "get-dom", "--selector", "body"]);
        assert!(matches!(
            cli.command,
            IntrospectCommand::GetDom { selector } if selector == "body"
        ));
    }

    #[test]
    fn clap_parses_screenshot_output() {
        let cli = IntrospectArgs::parse_from(["introspect", "screenshot", "--output", "x.png"]);
        assert!(matches!(
            cli.command,
            IntrospectCommand::Screenshot { output } if output == "x.png"
        ));
    }
}
