# Architecture

## Goal

Bridge MCP clients to LSP servers. One pathfinder instance = one LSP server.

## Design Principles

- **Simplicity**: Single LSP bridge, no pooling
- **Eager initialization**: LSP spawned at startup, not lazily
- **Retry-aware**: Handles LSP indexing delays transparently
- **Type-safe**: Rust with minimal unsafe code

## Components

### CLI Parser (`src/args.rs`)
- Uses `clap::Parser` derive macros
- `-e, --extension`: File extensions (repeatable)
- `-s, --server`: LSP command and args
- `-w, --workspace`: Project directory
- Produces `ServerSpec` with validated inputs

### Config (`src/config.rs`)
- Single `ServerConfig` (not Vec)
- Validates extensions and command non-empty
- Resolves workspace path

### LSP Bridge (`src/lsp_bridge.rs`)
- Spawns LSP subprocess via `tokio::process::Command`
- Manages stdin/stdout pipes
- Tracks request IDs for JSON-RPC
- 15s timeout per request
- Graceful shutdown: shutdown → exit → kill

### Document Manager (`src/documents.rs`)
- Tracks open documents by URI
- Sends didOpen/didChange/didClose to LSP
- Checks file mtime to avoid redundant syncs

### MCP Service (`src/service.rs`)
- Implements MCP server protocol
- Holds `Arc<Mutex<LspBridge>>` and `Arc<Mutex<DocumentManager>>`
- Exposes `definition` tool
- Handles document sync before LSP requests

### Tools (`src/tools/definition.rs`)
- Calls `textDocument/definition` on LSP
- Normalizes Location/LocationLink responses
- **Retry logic**: Up to 3 attempts with 150ms delay for empty results
- Handles LSP indexing delays transparently

### Transport (`src/transport.rs`)
- Content-Length framed JSON-RPC
- Used for LSP communication (stdin/stdout pipes)
- MCP transport is handled by the `rmcp` library

## Data Flow

```
MCP Client → pathfinder → LSP Server
     ↑                           ↓
     └─────── Response ──────────┘
```

1. MCP client calls `definition` tool
2. Service ensures document is synced (didOpen/didChange)
3. Tool sends `textDocument/definition` to LSP
4. Retry up to 3x if empty (handles indexing)
5. Normalize response to `[{uri, range}]`
6. Return to MCP client

## Request ID Management

- LSP Bridge: Increments `next_request_id` counter for each request
- Waits for response in a loop, matching by JSON-RPC id field
- Discards unrelated notifications while waiting
- Timeout mechanism: 15s per request

## Shutdown Sequence

1. MCP receives shutdown
2. Documents send didClose to LSP
3. LSP receives shutdown request
4. LSP receives exit notification
5. Process killed if timeout

## File Structure

```
src/
  args.rs          - CLI argument parsing
  config.rs        - Configuration validation
  documents.rs     - Document sync management
  lsp_bridge.rs    - LSP subprocess lifecycle
  main.rs          - Entry point, MCP server setup
  service.rs       - MCP protocol implementation
  transport.rs     - JSON-RPC framing
  utils.rs         - URI/path/languageId helpers
  tools/
    mod.rs         - Tool exports
    definition.rs  - Definition tool with retry
```

## Security Model

Current implementation has security gaps:

**Risks:**
- No command validation (arbitrary code execution)
- No workspace boundary checks
- No resource limits on LSP process
- Error messages leak file paths
- Unbounded memory allocation in transport

**Mitigations needed:**
- Whitelist allowed LSP executables
- Validate all file operations within workspace
- Set rlimits on spawned processes
- Sanitize error messages
- Add MAX_MESSAGE_SIZE constant

See security analysis for details.

## Extension Points

Add new tools:
1. Define request/response in `src/tools/`
2. Implement `execute(&mut LspBridge)` method
3. Add handler to `PathfinderService` with `#[tool]` macro
4. Consider if retry logic needed

Examples: hover, references, rename, codeAction
