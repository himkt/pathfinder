# Pathfinder MCP

Bridge MCP clients to LSP servers. Each pathfinder instance handles one LSP server.

## Build

```bash
cargo build --release
```

Binary at `target/release/pathfinder`.

## Usage

```bash
# Single extension
pathfinder -e py -s pyright-langserver -- --stdio

# Multiple extensions
pathfinder -e py -e pyi -s uv run pyright-langserver -- --stdio

# With workspace
pathfinder -e rs -s rust-analyzer -w /path/to/project
```

### Flags

- `-e, --extension <EXT>` - File extension (no dots, can repeat)
- `-s, --server <CMD>...` - LSP server command
- `-w, --workspace <PATH>` - Project directory (default: current dir)

## MCP Configuration

### Single Language

```json
{
  "mcpServers": {
    "pathfinder-python": {
      "command": "/path/to/pathfinder",
      "args": ["-e", "py", "-s", "pyright-langserver", "--", "--stdio"]
    }
  }
}
```

### Multiple Languages

Run separate instances:

```json
{
  "mcpServers": {
    "pathfinder-rust": {
      "command": "/path/to/pathfinder",
      "args": ["-e", "rs", "-s", "rust-analyzer"]
    },
    "pathfinder-ts": {
      "command": "/path/to/pathfinder",
      "args": ["-e", "ts", "-e", "tsx", "-s", "typescript-language-server", "--", "--stdio"]
    }
  }
}
```

## Tools

**definition** - Jump to definition via LSP `textDocument/definition`

Input: `{ uri: string, line: number, character: number }`

Returns: `[{ uri, range }]`

Automatically retries 3x with 150ms delay when LSP returns empty (handles indexing delays).

## Troubleshooting

- `LOG_LEVEL=debug` to see LSP traffic
- LSP timeout: 15 seconds
- Check LSP stderr for errors
- Debug logs show retry attempts

## Examples

```bash
# Python with uv
pathfinder -e py -e pyi -s uv run pyright-langserver -- --stdio

# TypeScript
pathfinder -e ts -e tsx -s typescript-language-server -- --stdio

# Rust
pathfinder -e rs -s rust-analyzer

# Go
pathfinder -e go -s gopls
```
