use std::process::Stdio;
use tokio::process::Command;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalRequest {
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalResponse {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

/// A simple broker for executing terminal commands.
pub struct TerminalBroker;

impl TerminalBroker {
    pub async fn execute(request: TerminalRequest) -> TerminalResponse {
        let mut command = Command::new(&request.cmd);
        command.args(&request.args);
        
        if let Some(cwd) = &request.cwd {
            command.current_dir(cwd);
        }
        
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        match command.spawn() {
            Ok(child) => {
                match child.wait_with_output().await {
                    Ok(output) => TerminalResponse {
                        success: output.status.success(),
                        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                        exit_code: output.status.code(),
                        error: None,
                    },
                    Err(e) => TerminalResponse {
                        success: false,
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: None,
                        error: Some(format!("Failed to wait for process: {}", e)),
                    },
                }
            }
            Err(e) => TerminalResponse {
                success: false,
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                error: Some(format!("Failed to spawn process: {}", e)),
            },
        }
    }
}
