use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    TerminalExec,
    TerminalOutputRead,
    DomRead,
    CacheRead,
    TabInspect,
    AiToolUse,
    VirtualResourceMount,
    FsRead,
    FsWrite,
    DomWrite,
    ScreenshotWindow,
}

impl Capability {
    pub fn label(&self) -> &'static str {
        match self {
            Self::TerminalExec => "terminal-exec",
            Self::TerminalOutputRead => "terminal-output-read",
            Self::DomRead => "dom-read",
            Self::CacheRead => "cache-read",
            Self::TabInspect => "tab-inspect",
            Self::AiToolUse => "ai-tool-use",
            Self::VirtualResourceMount => "virtual-resource-mount",
            Self::FsRead => "fs-read",
            Self::FsWrite => "fs-write",
            Self::DomWrite => "dom-write",
            Self::ScreenshotWindow => "screenshot-window",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "terminal-exec" => Some(Self::TerminalExec),
            "terminal-output-read" => Some(Self::TerminalOutputRead),
            "dom-read" => Some(Self::DomRead),
            "cache-read" => Some(Self::CacheRead),
            "tab-inspect" => Some(Self::TabInspect),
            "ai-tool-use" => Some(Self::AiToolUse),
            "virtual-resource-mount" => Some(Self::VirtualResourceMount),
            "fs-read" => Some(Self::FsRead),
            "fs-write" => Some(Self::FsWrite),
            "dom-write" => Some(Self::DomWrite),
            "screenshot-window" => Some(Self::ScreenshotWindow),
            _ => None,
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

impl PermissionDecision {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "allow" => Some(Self::Allow),
            "ask" => Some(Self::Ask),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PermissionPolicy {
    pub default: PermissionDecision,
    pub capabilities: BTreeMap<Capability, PermissionDecision>,
    pub domain_overrides: BTreeMap<String, BTreeMap<Capability, PermissionDecision>>,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        let mut capabilities = BTreeMap::new();
        capabilities.insert(Capability::TerminalExec, PermissionDecision::Deny);
        capabilities.insert(Capability::TerminalOutputRead, PermissionDecision::Ask);
        capabilities.insert(Capability::DomRead, PermissionDecision::Ask);
        capabilities.insert(Capability::CacheRead, PermissionDecision::Ask);
        capabilities.insert(Capability::TabInspect, PermissionDecision::Ask);
        capabilities.insert(Capability::AiToolUse, PermissionDecision::Ask);
        capabilities.insert(Capability::VirtualResourceMount, PermissionDecision::Deny);
        capabilities.insert(Capability::FsRead, PermissionDecision::Ask);
        capabilities.insert(Capability::FsWrite, PermissionDecision::Deny);

        Self {
            default: PermissionDecision::Ask,
            capabilities,
            domain_overrides: BTreeMap::new(),
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

    pub fn decision_for_domain(
        &self,
        domain: &str,
        capability: &Capability,
    ) -> PermissionDecision {
        if let Some(overrides) = self.domain_overrides.get(domain) {
            if let Some(decision) = overrides.get(capability) {
                return *decision;
            }
        }
        self.decision_for(capability)
    }
}
