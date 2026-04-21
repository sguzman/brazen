# Virtual Resource Injection Roadmap

Tracks the mechanism for "mounting" local or synthetic resources
(filesystem, terminal, MCP tools) as virtual web resources accessible to
browser-resident AI agents.

## Current State

- [ ] Internal resource resolution logic exists for Servo-specific assets
- [ ] No virtual mount point (e.g., `brazen://`) is implemented
- [ ] No bridging between Local Connectors and Engine Protocol Handlers

## Protocol & Mounting

- [ ] Define the `brazen://` (or similar) internal URI scheme
- [ ] Implement a Virtual Protocol Handler in the engine abstraction
- [ ] Support for directory-like mounting of filesystem paths
- [ ] Support for dynamic mounting of terminal sessions as streamable
      resources
- [ ] Support for mounting MCP tool definitions as discoverable JSON resources
- [ ] CORS and security boundary enforcement for virtual resources

## Injection Mechanism

- [ ] Origin-specific mounting (only allow `chatgpt.com` to see certain mounts)
- [ ] Read-only vs Read-Write mount modes
- [ ] Streaming support for large files or terminal output
- [ ] Content-type sniffing for local resources mapped to web types
- [ ] Support for injecting resources into the DOM via virtual script tags or state objects

## Integration Workflows

- [ ] Bridging `AssetStore` queries as virtual resources
- [ ] Bridging `LocalConnectors` as virtual resources
- [ ] Multi-tab resource sharing (one tab's DOM as another tab's virtual file)
- [ ] Live-update triggers for virtual resources (filesystem watch -> browser event)

## Product Surfaces

- [ ] Mount-point manager UI (see what is currently shared)
- [ ] Per-site "Resource Gallery" for AI agents
- [ ] Debugger for virtual request/response cycles
