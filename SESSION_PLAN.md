## Session Plan

### Task 1: Enable API retry with exponential backoff
Files: src/main.rs, src/cli.rs
Description: Wire up yoagent's built-in `RetryConfig` to the agent builder. Currently, any API error (rate limit, network glitch) kills the prompt entirely. The fix is simple: call `.with_retry_config(RetryConfig::default())` in `build_agent()`. This gives us 3 retries, 1s initial delay, 2x backoff, 30s max — already implemented in yoagent. Also import `yoagent::retry::RetryConfig` and display retry status in the banner when verbose mode is on. Add a `--no-retry` flag for users who want fail-fast behavior (e.g., in scripts). Add tests for the flag parsing.
Issue: none

### Task 2: Fix MCP connection loss on /clear, /model, /think
Files: src/main.rs
Description: When the user runs `/clear`, `/model <name>`, or `/think <level>`, the agent is rebuilt from scratch via `build_agent()` — but MCP servers are not reconnected. This means any MCP tools silently vanish mid-session. Fix by: (1) extracting the MCP reconnection logic into a helper async function, (2) after every `build_agent()` call in the REPL, re-run the MCP connections using the stored `mcp_servers` list. The reconnection helper should log success/failure for each server. If a reconnection fails, warn the user but continue. Add a test verifying the mcp_servers list is preserved correctly.
Issue: none

### Task 3: Add --mcp-http flag for HTTP/SSE MCP servers
Files: src/main.rs, src/cli.rs
Description: yoagent supports `with_mcp_server_http(url)` for HTTP-based MCP servers (SSE transport), but yoyo only exposes `--mcp` for stdio. Add `--mcp-http <url>` flag that connects via HTTP instead. This is important because many MCP servers (like remote ones, cloud-hosted ones) use HTTP transport. Add to KNOWN_FLAGS, flags_needing_values, help text, and the connection loop. Add tests for flag parsing.
Issue: none

### Issue Responses
(No community issues today)
