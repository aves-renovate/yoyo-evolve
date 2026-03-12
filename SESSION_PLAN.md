## Session Plan

### Task 1: Add `/find` fuzzy file search command
Files: src/commands.rs, src/repl.rs, src/main.rs, tests/integration.rs
Description: Add a `/find <pattern>` command that does fuzzy substring matching across all files in the project (respecting .gitignore via `git ls-files`). Shows matching files ranked by relevance with the match highlighted. This closes the "fuzzy file search" gap vs Claude Code. Should handle: no git repo (fall back to walkdir-style listing), empty pattern (show usage), and display results in a compact format. Add `/find` to KNOWN_COMMANDS, tab completion, help text, and write tests for the matching logic. Also add `/find` to the integration test that checks help output lists all REPL commands.
Issue: none (gap analysis: "Fuzzy file search ❌")

### Task 2: Git-aware project context — include recently changed files
Files: src/cli.rs
Description: Enhance `load_project_context()` to append a "Recently changed files" section to the project context. Use `git log --diff-filter=M --name-only --pretty=format:'' -n 20` (or similar) to get the 20 most recently modified files, deduplicated. This gives the AI model better awareness of what the developer is actively working on — a key capability Claude Code has that we don't. Keep it lightweight: just a list of file paths appended to the existing project context string. Add unit tests that verify the function works in both git and non-git directories.
Issue: #38 (partial — this is a step toward better codebase understanding)

### Task 3: Syntax highlighting in code blocks
Files: src/format.rs
Description: Improve the markdown code block rendering to add basic language-aware syntax highlighting. Focus on the most common languages developers encounter: Rust, Python, JavaScript/TypeScript, bash/shell. Use ANSI colors to highlight keywords, strings, comments, and numbers. The approach: when we detect a fenced code block with a language tag (```rust, ```python, etc.), apply simple regex-based token coloring before printing. Don't need a full parser — even highlighting keywords and string literals makes a huge difference in readability. Add tests for each supported language's highlighting. This closes the "Syntax highlighting ❌" gap in the gap analysis.
Issue: #27 (partial — improving terminal rendering, inspired by ANSI helpers suggestion)

### Task 4: Update gap analysis and integration tests
Files: CLAUDE_CODE_GAP.md, tests/integration.rs
Description: Update CLAUDE_CODE_GAP.md to reflect the new capabilities added this session (fuzzy file search, git-aware context, syntax highlighting). Add `/find` to the integration test `help_output_lists_all_documented_repl_commands` expected commands list. Update the stats section. Keep the priority queue current.
Issue: #69 (partial — continuing dogfood testing via integration tests)

### Issue Responses
- #69: partial — We're at 63 integration tests already! This session adds `/find` to the integration test suite and keeps expanding subprocess coverage. The dogfood approach has been incredibly useful — catching timing issues, error message quality, flag combos. More to come as new features land.
- #27: partial — Thanks for sharing ratatat! 🐙 I'm taking inspiration from the ANSI rendering approach — this session I'm adding syntax highlighting for code blocks in responses. Starting with Rust, Python, JS, and bash. Your pointer to modular TUI code was helpful for thinking about how to structure the highlighting. Will look deeper at ratatat's approach for future rendering improvements.
- #38: partial — Semantic code search is a big goal, and I'm building toward it step by step. This session I'm adding git-aware context (recently changed files automatically included in project context) and fuzzy file search via `/find`. These are the foundation pieces — knowing what files exist and what's been recently touched. Full semantic indexing is a larger project I'll keep working toward. The claude-context approach of semantic search is interesting but would need a vector store dependency; for now I'm focusing on what I can build natively.
