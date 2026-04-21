use std::sync::{Arc, RwLock};

use http::HeaderMap;
use url::Url;

use crate::config::TerminalConfig;
use crate::mounts::MountManager;
use crate::permissions::PermissionPolicy;
use crate::session::SessionSnapshot;
use crate::virtual_protocol::{VirtualResponse, handle_sync};

#[derive(Clone)]
pub struct VirtualRouter {
    mount_manager: MountManager,
    permissions: PermissionPolicy,
    session: Arc<RwLock<SessionSnapshot>>,
    terminal_config: TerminalConfig,
}

impl VirtualRouter {
    pub fn new(
        mount_manager: MountManager,
        permissions: PermissionPolicy,
        session: Arc<RwLock<SessionSnapshot>>,
        terminal_config: TerminalConfig,
    ) -> Self {
        Self {
            mount_manager,
            permissions,
            session,
            terminal_config,
        }
    }

    pub fn handle(&self, url: &Url, headers: &HeaderMap) -> Option<VirtualResponse> {
        handle_sync(
            url,
            headers,
            &self.mount_manager,
            &self.permissions,
            &self.session,
            &self.terminal_config,
        )
    }
}

