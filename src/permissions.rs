use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    TerminalExec,
    DomRead,
    CacheRead,
    TabInspect,
    AiToolUse,
    VirtualResourceMount,
}

impl Capability {
    pub fn label(&self) -> &'static str {
        match self {
            Self::TerminalExec => "terminal-exec",
            Self::DomRead => "dom-read",
            Self::CacheRead => "cache-read",
            Self::TabInspect => "tab-inspect",
            Self::AiToolUse => "ai-tool-use",
            Self::VirtualResourceMount => "virtual-resource-mount",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionDecision {
    Allow,
    #[default]
    Ask,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PermissionPolicy {
    pub default: PermissionDecision,
    pub capabilities: BTreeMap<Capability, PermissionDecision>,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        let mut capabilities = BTreeMap::new();
        capabilities.insert(Capability::TerminalExec, PermissionDecision::Deny);
        capabilities.insert(Capability::DomRead, PermissionDecision::Ask);
        capabilities.insert(Capability::CacheRead, PermissionDecision::Ask);
        capabilities.insert(Capability::TabInspect, PermissionDecision::Ask);
        capabilities.insert(Capability::AiToolUse, PermissionDecision::Ask);
        capabilities.insert(Capability::VirtualResourceMount, PermissionDecision::Deny);

        Self {
            default: PermissionDecision::Ask,
            capabilities,
        }
    }
}

impl PermissionPolicy {
    pub fn decision_for(&self, capability: &Capability) -> PermissionDecision {
        self.capabilities
            .get(capability)
            .copied()
            .unwrap_or(self.default)
    }
}
