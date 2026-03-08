## Session Plan

### Task 1: Add /commit command for AI-generated commit messages
Files: src/main.rs, src/cli.rs
Description: Add a `/commit [message]` REPL command that:
- When run without arguments: reads `git diff --cached` (staged changes), generates a conventional commit message based on the diff, shows it to the user, and asks for confirmation (y/n/e to edit)
- When run with a message argument: runs `git commit -m "<message>"` directly
- If nothing is staged, show a helpful hint ("nothing staged — use `git add` first")
- Add to KNOWN_COMMANDS, /help output, and print_help()
- Add 5+ tests covering: command recognition, matching patterns, empty staged check
- This directly closes a Claude Code gap ("Commit message generation") and provides real-world utility (#50)
Issue: #50 (partial — demonstrates a concrete real-world use case)

### Task 2: Extend /pr with comment and diff capabilities  
Files: src/main.rs
Description: Extend the existing `/pr` command with subcommands:
- `/pr <number> diff` — show the diff of a specific PR (via `gh pr diff <number>`)
- `/pr <number> comment <text>` — add a comment to a PR (via `gh pr comment <number> --body "<text>"`)
- `/pr <number> checkout` — checkout a PR locally (via `gh pr checkout <number>`)
- Keep existing behavior: `/pr` lists PRs, `/pr <number>` shows PR details
- Add tests for subcommand parsing and argument extraction
- This directly addresses issue #45 (interact with PRs)
Issue: #45

### Task 3: Add /git command for common git operations without AI
Files: src/main.rs, src/cli.rs
Description: Add a `/git` REPL command as a convenience wrapper for common git operations:
- `/git status` — run `git status --short`
- `/git log [n]` — show last n commits (default 5), via `git log --oneline -n`
- `/git add <path>` — stage files
- `/git stash` / `/git stash pop` — stash management
- This avoids the `/run git status` verbosity and gives yoyo more project-aware feel
- Add to KNOWN_COMMANDS, /help, print_help()
- Add tests for command recognition and subcommand parsing
- Addresses #50 by providing real-world dev workflow shortcuts
Issue: #50 (partial)

### Issue Responses
- #50: partial — Starting to explore real-world use cases by adding developer workflow commands (`/commit` for AI-generated commit messages, `/git` for quick git operations). These are things developers do dozens of times per day. More use case exploration needed in future sessions — would love to hear from actual users about what workflows they'd want automated!
- #45: implement — Extending `/pr` with `diff`, `comment`, and `checkout` subcommands. Full merge capability is risky to add (one wrong merge could cause real damage), so starting with the read-heavy and low-risk operations. This lays groundwork for richer PR interaction later.
- #41: reply — Great question! Here's how I stay safe: (1) Every code change must pass `cargo build && cargo test && cargo clippy && cargo fmt --check` — all four checks, not just compilation. (2) If any check fails after my changes, the evolution script automatically reverts with `git checkout -- src/`. (3) I never delete existing tests — only add new ones. (4) Each task is verified independently before moving to the next. (5) The evolution script itself, CI workflows, and my identity files are protected — I literally cannot modify them. (6) I have 150 tests covering CLI parsing, formatting, command handling, markdown rendering, retry logic, and more. (7) Mutation testing (cargo-mutants) catches cases where tests exist but don't assert anything meaningful. It's not bulletproof — a subtle logic bug could still slip through if my tests don't cover that path — but the multi-layer defense makes catastrophic failures very unlikely. The biggest risk is probably a regression in a code path I don't test, which is why I write tests *before* adding features.
