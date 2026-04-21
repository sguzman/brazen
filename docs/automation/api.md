# Brazen Automation API

The Brazen automation API allows external applications and scripts to control the browser and introspect its state via a WebSocket connection.

## Connecting

By default, Brazen listens for WebSocket connections on `ws://127.0.0.1:7942/ws`.

The bind address can be configured in `brazen.toml`:
```toml
[automation]
enabled = true
bind = "ws://127.0.0.1:7942"
# auth_token = "optional-secret-token"
```

If an `auth_token` is set, you must provide it in the query string:
`ws://127.0.0.1:7942/ws?token=your-secret-token`

## Message Format

All messages are JSON-encoded.

### Requests

Requests are sent as an "envelope" containing an `id` and the `payload`.

```json
{
  "id": "optional-unique-id",
  "type": "command-type",
  "param1": "value1",
  ...
}
```

### Responses

Responses follow a standard format:

```json
{
  "id": "matches-request-id",
  "ok": true,
  "result": { ... },
  "error": null
}
```

If `ok` is `false`, the `error` field will contain a description of the failure.

## Commands

### Navigation

- `tab-list`: Get a list of all open tabs.
- `tab-new`: Open a new tab. Parameters: `url` (optional).
- `tab-activate`: Switch to a specific tab. Parameters: `index` or `tab_id`.
- `tab-close`: Close a tab. Parameters: `index` or `tab_id`.
- `tab-navigate`: Navigate the active tab to a new URL. Parameters: `url`.
- `tab-reload`: Reload the active tab.
- `tab-stop`: Stop loading the active tab.
- `tab-back`: Go back in history.
- `tab-forward`: Go forward in history.

### Inspection

- `snapshot`: Get a full state dump of the browser (tabs, cache stats, etc.).
- `dom-query`: Query the DOM of the active tab using a CSS selector. Parameters: `selector`.
- `rendered-text`: Get the full text content of the active tab.
- `article_text`: Get the extracted article text (Reader Mode content).
- `screenshot`: Capture a screenshot of the active tab (returns base64).
- `screenshot-window`: Capture a screenshot of the entire browser window.

### System

- `cache-stats`: Get statistics about the asset cache.
- `cache-query`: Search for cached assets. Parameters: `query` (AssetQuery object), `limit`.
- `terminal-exec`: Execute a command in the local terminal (requires capability). Parameters: `cmd`, `args`, `cwd`.
- `mount-add`: Mount a local directory as a virtual resource. Parameters: `name`, `local_path`, `read_only`, `allowed_domains`.
- `shutdown`: Close the browser.

## Events

You can subscribe to asynchronous events using the `subscribe` command.

```json
{
  "type": "subscribe",
  "topics": ["navigation", "capability", "terminal-output"]
}
```

### Navigation Events

Emitted when a tab navigates or changes load status.

```json
{
  "topic": "navigation",
  "url": "https://example.com",
  "title": "Example",
  "load_status": "complete",
  "load_progress": 1.0
}
```

### Terminal Output

Emitted during `terminal-exec-stream`.

```json
{
  "topic": "terminal-output",
  "session_id": "...",
  "stream": "stdout",
  "chunk": "hello world\n"
}
```

## Schema

A formal JSON Schema is available at [schema.json](./schema.json).
