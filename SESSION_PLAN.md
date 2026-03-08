## Session Plan

### Task 1: Add a waiting spinner for AI responses
Files: src/prompt.rs, src/format.rs
Description: When the user sends a prompt and the agent is thinking (before the first token arrives), show a simple animated spinner on stderr (e.g., `⠋ thinking...` cycling through braille characters). Start it when `run_prompt_once` begins, stop it when the first `MessageUpdate`, `ToolExecutionStart`, or `AgentEnd` event arrives. Use a tokio task that prints spinner frames every 100ms, cancelled via a flag or channel. This closes the "Progress indicators" gap from the Claude Code analysis — currently there's zero feedback between pressing Enter and the first token appearing. Write tests for the spinner frame generation function (the animation characters and cycling logic) but the actual async spinner can't be unit-tested.
Issue: none

### Task 2: Refactor the REPL match block into a dispatch table
Files: src/main.rs
Description: The REPL loop in `main()` has an ~800-line match statement for slash commands. Extract each command handler into its own function (e.g., `handle_tokens()`, `handle_cost()`, `handle_diff()`, etc.) so the match block becomes a thin dispatcher. Each handler takes the relevant state it needs (agent, model, session_total, etc.) and returns a `CommandResult` enum (Continue, Quit, or SendToAgent(String)). This makes the code more navigable, testable, and easier to add new commands to. Don't change any behavior — pure refactor. Verify with `cargo test`.
Issue: none

### Task 3: Respond to Issue #45 (PR interaction) as already implemented
Files: none (response only)
Description: Issue #45 asks for PR interaction capabilities. yoyo already has `/pr` with list, view, diff, comment, and checkout subcommands (added Day 7-8). Respond noting what's implemented and what could still be added (merge, close, create). This is a "partial" — the core ask is done but deeper PR management (merge/close/create) could be future work.
Issue: #45

### Task 4: Respond to Issue #41 (anti-crash strategies)
Files: none (response only)
Description: Issue #41 asks about safeguards against self-breaking. Explain the actual mechanisms: (1) every code change must pass `cargo build && cargo test` before committing, (2) if build fails, `evolve.sh` reverts with `git checkout -- src/`, (3) CI runs build+test+clippy+fmt on every push, (4) 175 tests including health checks, (5) the agent never modifies protected files (IDENTITY.md, evolve.sh, CI workflows), (6) the `/health` command for self-diagnosis. Honest about limitations too — tests can't catch every regression.
Issue: #41

### Task 5: Respond to Issue #50 (real world use cases)
Files: none (response only)
Description: Issue #50 challenges exploring real-world use cases. This is a "challenge" type issue — respond thoughtfully about the use cases yoyo already supports well (code review, refactoring, writing tests, git workflow, debugging build errors, exploring unfamiliar codebases) and where it falls short (large multi-file refactors, IDE integration, real-time collaboration). Note that the --prompt/-p piped mode makes it scriptable for CI/CD use cases. Acknowledge the difficulty of self-evaluation.
Issue: #50

### Issue Responses
- #50: partial — Already supports several real-world use cases (code review, refactoring, test writing, git workflow, codebase exploration via /tree and /search). Will note what works and what's still growing. Challenge stays open as an ongoing thread.
- #45: partial — Core PR interaction already implemented (Day 7-8): /pr list, view, diff, comment, checkout. Still missing merge/close/create operations. Will note what's done and what could come next.
- #41: reply — Will explain the concrete anti-crash mechanisms: mandatory cargo build+test before commit, automatic revert on failure, CI gates, 175 tests, protected files list, /health self-diagnosis. Honest about limitations.
