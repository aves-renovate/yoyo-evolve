//! REPL command handlers — one function per slash command.
//!
//! Each `cmd_*` function handles a single REPL command, printing its output
//! and returning nothing. Control flow is managed by `dispatch_command`
//! which returns a `CommandResult`.

use crate::cli::*;
use crate::format::*;
use crate::{
    build_agent, compact_agent, is_unknown_command, run_health_check, run_shell_command,
    thinking_level_name,
};

use yoagent::agent::Agent;
use yoagent::context::total_tokens;
use yoagent::skills::SkillSet;
use yoagent::*;

/// Result of executing a REPL command.
pub enum CommandResult {
    /// Continue the REPL loop (don't send to AI).
    Continue,
    /// Break out of the REPL loop (quit).
    Quit,
    /// Not a command — pass input to the AI.
    NotACommand,
}

/// Shared REPL state passed to command handlers.
pub struct ReplState {
    pub model: String,
    pub api_key: String,
    pub skills: SkillSet,
    pub system_prompt: String,
    pub thinking: ThinkingLevel,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub max_turns: Option<usize>,
    pub auto_approve: bool,
    pub mcp_count: u32,
    pub cwd: String,
    pub continue_session: bool,
    pub session_total: Usage,
    pub last_input: Option<String>,
}

impl ReplState {
    /// Rebuild the agent with current settings (discards conversation).
    pub fn rebuild_agent(&self) -> Agent {
        build_agent(
            &self.model,
            &self.api_key,
            &self.skills,
            &self.system_prompt,
            self.thinking,
            self.max_tokens,
            self.temperature,
            self.max_turns,
            self.auto_approve,
        )
    }

    /// Rebuild the agent, preserving conversation history.
    pub fn rebuild_agent_preserving(&self, agent: &mut Agent) {
        let saved = agent.save_messages().ok();
        *agent = self.rebuild_agent();
        if let Some(json) = saved {
            let _ = agent.restore_messages(&json);
        }
    }
}

pub fn cmd_help() {
    println!("{DIM}  /help              Show this help");
    println!("  /quit, /exit       Exit yoyo");
    println!("  /clear             Clear conversation history");
    println!("  /compact           Compact conversation to save context space");
    println!("  /config            Show all current settings");
    println!("  /context           Show loaded project context files");
    println!("  /cost              Show estimated session cost");
    println!("  /init              Create a starter YOYO.md project context file");
    println!("  /model <name>      Switch model (preserves conversation)");
    println!("  /think [level]     Show or change thinking level (off/low/medium/high)");
    println!("  /status            Show session info");
    println!("  /tokens            Show token usage and context window");
    println!("  /save [path]       Save session to file (default: yoyo-session.json)");
    println!("  /load [path]       Load session from file");
    println!("  /diff              Show git diff summary of uncommitted changes");
    println!("  /undo              Revert all uncommitted changes (git checkout)");
    println!("  /health            Run health checks (build, test, clippy, fmt)");
    println!("  /retry             Re-send the last user input");
    println!("  /run <cmd>         Run a shell command directly (no AI, no tokens)");
    println!("  !<cmd>             Shortcut for /run");
    println!("  /history           Show summary of conversation messages");
    println!("  /version           Show yoyo version");
    println!();
    println!("  Multi-line input:");
    println!("  End a line with \\ to continue on the next line");
    println!("  Start with ``` to enter a fenced code block{RESET}\n");
}

pub fn cmd_version() {
    println!("{DIM}  yoyo v{VERSION}{RESET}\n");
}

pub fn cmd_status(state: &ReplState) {
    println!("{DIM}  model:   {}", state.model);
    if let Some(branch) = git_branch() {
        println!("  git:     {branch}");
    }
    println!("  cwd:     {}", state.cwd);
    println!(
        "  tokens:  {} in / {} out (session total){RESET}\n",
        state.session_total.input, state.session_total.output
    );
}

pub fn cmd_tokens(agent: &Agent, state: &ReplState) {
    let max_context = MAX_CONTEXT_TOKENS;

    // Estimate actual context window usage from message history
    let messages = agent.messages().to_vec();
    let context_used = total_tokens(&messages) as u64;
    let bar = context_bar(context_used, max_context);

    println!("{DIM}  Context window:");
    println!("    messages:    {}", messages.len());
    println!(
        "    context:     {} / {} tokens",
        format_token_count(context_used),
        format_token_count(max_context)
    );
    println!("    {bar}");
    if context_used as f64 / max_context as f64 > 0.75 {
        println!("    {YELLOW}⚠ Context is getting full. Consider /clear or /compact.{RESET}");
    }
    println!();
    println!("  Session totals:");
    println!(
        "    input:       {} tokens",
        format_token_count(state.session_total.input)
    );
    println!(
        "    output:      {} tokens",
        format_token_count(state.session_total.output)
    );
    println!(
        "    cache read:  {} tokens",
        format_token_count(state.session_total.cache_read)
    );
    println!(
        "    cache write: {} tokens",
        format_token_count(state.session_total.cache_write)
    );
    if let Some(cost) = estimate_cost(&state.session_total, &state.model) {
        println!("    est. cost:   {}", format_cost(cost));
    }
    println!("{RESET}");
}

pub fn cmd_cost(state: &ReplState) {
    if let Some(cost) = estimate_cost(&state.session_total, &state.model) {
        println!("{DIM}  Session cost: {}", format_cost(cost));
        println!(
            "    {} in / {} out",
            format_token_count(state.session_total.input),
            format_token_count(state.session_total.output)
        );
        if state.session_total.cache_read > 0 || state.session_total.cache_write > 0 {
            println!(
                "    cache: {} read / {} write",
                format_token_count(state.session_total.cache_read),
                format_token_count(state.session_total.cache_write)
            );
        }
        // Show cost breakdown by category
        if let Some((input_cost, cw_cost, cr_cost, output_cost)) =
            cost_breakdown(&state.session_total, &state.model)
        {
            println!();
            println!("    Breakdown:");
            println!("      input:       {}", format_cost(input_cost));
            println!("      output:      {}", format_cost(output_cost));
            if cw_cost > 0.0 {
                println!("      cache write: {}", format_cost(cw_cost));
            }
            if cr_cost > 0.0 {
                println!("      cache read:  {}", format_cost(cr_cost));
            }
        }
        println!("{RESET}");
    } else {
        println!(
            "{DIM}  Cost estimation not available for model '{}'.{RESET}\n",
            state.model
        );
    }
}

pub fn cmd_clear(agent: &mut Agent, state: &ReplState) {
    *agent = state.rebuild_agent();
    println!("{DIM}  (conversation cleared){RESET}\n");
}

pub fn cmd_model_show(state: &ReplState) {
    println!("{DIM}  current model: {}", state.model);
    println!("  usage: /model <name>{RESET}\n");
}

pub fn cmd_model_switch(new_model: &str, agent: &mut Agent, state: &mut ReplState) {
    if new_model.is_empty() {
        cmd_model_show(state);
        return;
    }
    state.model = new_model.to_string();
    state.rebuild_agent_preserving(agent);
    println!("{DIM}  (switched to {new_model}, conversation preserved){RESET}\n");
}

pub fn cmd_think_show(state: &ReplState) {
    let level_str = thinking_level_name(state.thinking);
    println!("{DIM}  thinking: {level_str}");
    println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
}

pub fn cmd_think_switch(level_str: &str, agent: &mut Agent, state: &mut ReplState) {
    if level_str.is_empty() {
        cmd_think_show(state);
        return;
    }
    let new_thinking = parse_thinking_level(level_str);
    if new_thinking == state.thinking {
        let current = thinking_level_name(state.thinking);
        println!("{DIM}  thinking already set to {current}{RESET}\n");
        return;
    }
    state.thinking = new_thinking;
    state.rebuild_agent_preserving(agent);
    let level_name = thinking_level_name(state.thinking);
    println!("{DIM}  (thinking set to {level_name}, conversation preserved){RESET}\n");
}

pub fn cmd_save(input: &str, agent: &Agent) {
    let path = input.strip_prefix("/save").unwrap_or("").trim();
    let path = if path.is_empty() {
        DEFAULT_SESSION_PATH
    } else {
        path
    };
    match agent.save_messages() {
        Ok(json) => match std::fs::write(path, &json) {
            Ok(_) => println!(
                "{DIM}  (session saved to {path}, {} messages){RESET}\n",
                agent.messages().len()
            ),
            Err(e) => eprintln!("{RED}  error saving: {e}{RESET}\n"),
        },
        Err(e) => eprintln!("{RED}  error serializing: {e}{RESET}\n"),
    }
}

pub fn cmd_load(input: &str, agent: &mut Agent) {
    let path = input.strip_prefix("/load").unwrap_or("").trim();
    let path = if path.is_empty() {
        DEFAULT_SESSION_PATH
    } else {
        path
    };
    match std::fs::read_to_string(path) {
        Ok(json) => match agent.restore_messages(&json) {
            Ok(_) => println!(
                "{DIM}  (session loaded from {path}, {} messages){RESET}\n",
                agent.messages().len()
            ),
            Err(e) => eprintln!("{RED}  error parsing: {e}{RESET}\n"),
        },
        Err(e) => eprintln!("{RED}  error reading {path}: {e}{RESET}\n"),
    }
}

pub fn cmd_diff() {
    // Use git status --short for a comprehensive view (modified, staged, untracked)
    match std::process::Command::new("git")
        .args(["status", "--short"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let status = String::from_utf8_lossy(&output.stdout);
            if status.trim().is_empty() {
                println!("{DIM}  (no uncommitted changes){RESET}\n");
            } else {
                println!("{DIM}  Changes:");
                for line in status.lines() {
                    println!("    {line}");
                }
                println!("{RESET}");
                // Also show diff stat for modified files
                if let Ok(diff) = std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output()
                {
                    let diff_text = String::from_utf8_lossy(&diff.stdout);
                    if !diff_text.trim().is_empty() {
                        println!("{DIM}{diff_text}{RESET}");
                    }
                }
            }
        }
        _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
    }
}

pub fn cmd_undo() {
    // Revert all uncommitted changes and remove untracked files
    let diff_output = std::process::Command::new("git")
        .args(["diff", "--stat"])
        .output();
    let untracked = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .output();

    let has_diff = diff_output
        .as_ref()
        .map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);
    let untracked_files: Vec<String> = untracked
        .as_ref()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();
    let has_untracked = !untracked_files.is_empty();

    if !has_diff && !has_untracked {
        println!("{DIM}  (nothing to undo — no uncommitted changes){RESET}\n");
    } else {
        if has_diff {
            if let Ok(ref output) = diff_output {
                let diff = String::from_utf8_lossy(&output.stdout);
                println!("{DIM}{diff}{RESET}");
            }
        }
        if has_untracked {
            println!("{DIM}  untracked files:");
            for f in &untracked_files {
                println!("    {f}");
            }
            println!("{RESET}");
        }

        // Revert modified files
        if has_diff {
            let _ = std::process::Command::new("git")
                .args(["checkout", "--", "."])
                .output();
        }
        // Remove untracked files
        if has_untracked {
            let _ = std::process::Command::new("git")
                .args(["clean", "-fd"])
                .output();
        }
        println!("{GREEN}  ✓ reverted all uncommitted changes{RESET}\n");
    }
}

pub fn cmd_health() {
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_check();
    let all_passed = results.iter().all(|(_, passed, _)| *passed);
    for (name, passed, detail) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        println!("  {icon} {name}: {detail}");
    }
    if all_passed {
        println!("\n{GREEN}  All checks passed ✓{RESET}\n");
    } else {
        println!("\n{RED}  Some checks failed ✗{RESET}\n");
    }
}

pub fn cmd_history(agent: &Agent) {
    use crate::prompt::summarize_message;

    let messages = agent.messages();
    if messages.is_empty() {
        println!("{DIM}  (no messages in conversation){RESET}\n");
    } else {
        println!("{DIM}  Conversation ({} messages):", messages.len());
        for (i, msg) in messages.iter().enumerate() {
            let (role, preview) = summarize_message(msg);
            let idx = i + 1;
            println!("    {idx:>3}. [{role}] {preview}");
        }
        println!("{RESET}");
    }
}

pub fn cmd_config(agent: &Agent, state: &ReplState) {
    println!("{DIM}  Configuration:");
    println!("    model:      {}", state.model);
    println!("    thinking:   {}", thinking_level_name(state.thinking));
    println!(
        "    max_tokens: {}",
        state
            .max_tokens
            .map(|m| m.to_string())
            .unwrap_or_else(|| "default (8192)".to_string())
    );
    println!(
        "    max_turns:  {}",
        state
            .max_turns
            .map(|m| m.to_string())
            .unwrap_or_else(|| "default (50)".to_string())
    );
    println!(
        "    temperature: {}",
        state
            .temperature
            .map(|t| format!("{t:.1}"))
            .unwrap_or_else(|| "default".to_string())
    );
    println!(
        "    skills:     {}",
        if state.skills.is_empty() {
            "none".to_string()
        } else {
            format!("{} loaded", state.skills.len())
        }
    );
    let system_preview =
        truncate_with_ellipsis(state.system_prompt.lines().next().unwrap_or("(empty)"), 60);
    println!("    system:     {system_preview}");
    if state.mcp_count > 0 {
        println!("    mcp:        {} server(s)", state.mcp_count);
    }
    println!(
        "    verbose:    {}",
        if is_verbose() { "on" } else { "off" }
    );
    if let Some(branch) = git_branch() {
        println!("    git:        {branch}");
    }
    println!("    cwd:        {}", state.cwd);
    println!(
        "    context:    {} max tokens",
        format_token_count(MAX_CONTEXT_TOKENS)
    );
    println!(
        "    auto-compact: at {:.0}%",
        AUTO_COMPACT_THRESHOLD * 100.0
    );
    println!("    messages:   {}", agent.messages().len());
    if state.continue_session {
        println!("    session:    auto-save on exit ({DEFAULT_SESSION_PATH})");
    }
    println!("{RESET}");
}

pub fn cmd_compact(agent: &mut Agent) {
    let messages = agent.messages();
    let before_count = messages.len();
    let before_tokens = total_tokens(messages) as u64;
    match compact_agent(agent) {
        Some((_, _, after_count, after_tokens)) => {
            println!(
                "{DIM}  compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}\n",
                format_token_count(before_tokens),
                format_token_count(after_tokens)
            );
        }
        None => {
            println!(
                "{DIM}  (nothing to compact — {before_count} messages, ~{} tokens){RESET}\n",
                format_token_count(before_tokens)
            );
        }
    }
}

pub fn cmd_context() {
    let files = list_project_context_files();
    if files.is_empty() {
        println!("{DIM}  No project context files found.");
        println!("  Searched for: {}", PROJECT_CONTEXT_FILES.join(", "));
        println!("  Create YOYO.md, CLAUDE.md, or .yoyo/instructions.md to add project context.");
        println!("  Or run /init to create a starter YOYO.md.{RESET}\n");
    } else {
        println!("{DIM}  Project context files:");
        for (name, lines) in &files {
            println!("    {name} ({lines} lines)");
        }
        println!("{RESET}");
    }
}

pub fn cmd_init() {
    let path = "YOYO.md";
    if std::path::Path::new(path).exists() {
        println!("{DIM}  {path} already exists — not overwriting.{RESET}\n");
    } else {
        let template = concat!(
            "# Project Context\n",
            "\n",
            "<!-- This file is read by yoyo at startup to understand your project. -->\n",
            "<!-- Customize it with project-specific instructions, conventions, and context. -->\n",
            "\n",
            "## About This Project\n",
            "\n",
            "<!-- Describe what this project does and its tech stack. -->\n",
            "\n",
            "## Coding Conventions\n",
            "\n",
            "<!-- List any coding standards, naming conventions, or patterns to follow. -->\n",
            "\n",
            "## Build & Test\n",
            "\n",
            "<!-- How to build, test, and run the project. -->\n",
            "\n",
            "## Important Files\n",
            "\n",
            "<!-- List key files and directories the agent should know about. -->\n",
        );
        match std::fs::write(path, template) {
            Ok(_) => {
                println!("{GREEN}  ✓ Created {path} — edit it to add project context.{RESET}\n")
            }
            Err(e) => eprintln!("{RED}  error creating {path}: {e}{RESET}\n"),
        }
    }
}

pub fn cmd_run(input: &str) {
    let cmd = if input.starts_with("/run ") {
        input.trim_start_matches("/run ").trim()
    } else if input.starts_with('!') && input.len() > 1 {
        input[1..].trim()
    } else {
        ""
    };
    if cmd.is_empty() {
        println!("{DIM}  usage: /run <command>  or  !<command>");
        println!("  Runs a shell command directly (no AI, no tokens).{RESET}\n");
    } else {
        run_shell_command(cmd);
    }
}

pub fn cmd_unknown(input: &str) {
    let cmd = input.split_whitespace().next().unwrap_or(input);
    eprintln!("{RED}  unknown command: {cmd}{RESET}");
    eprintln!("{DIM}  type /help for available commands{RESET}\n");
}

/// Dispatch a REPL command. Returns `CommandResult` to control the loop.
pub fn dispatch_command(input: &str, agent: &mut Agent, state: &mut ReplState) -> CommandResult {
    match input {
        "/quit" | "/exit" => CommandResult::Quit,
        "/help" => {
            cmd_help();
            CommandResult::Continue
        }
        "/version" => {
            cmd_version();
            CommandResult::Continue
        }
        "/status" => {
            cmd_status(state);
            CommandResult::Continue
        }
        "/tokens" => {
            cmd_tokens(agent, state);
            CommandResult::Continue
        }
        "/cost" => {
            cmd_cost(state);
            CommandResult::Continue
        }
        "/clear" => {
            cmd_clear(agent, state);
            CommandResult::Continue
        }
        "/model" => {
            cmd_model_show(state);
            CommandResult::Continue
        }
        s if s.starts_with("/model ") => {
            let new_model = s.trim_start_matches("/model ").trim();
            cmd_model_switch(new_model, agent, state);
            CommandResult::Continue
        }
        "/think" => {
            cmd_think_show(state);
            CommandResult::Continue
        }
        s if s.starts_with("/think ") => {
            let level_str = s.trim_start_matches("/think ").trim();
            cmd_think_switch(level_str, agent, state);
            CommandResult::Continue
        }
        s if s == "/save" || s.starts_with("/save ") => {
            cmd_save(s, agent);
            CommandResult::Continue
        }
        s if s == "/load" || s.starts_with("/load ") => {
            cmd_load(s, agent);
            CommandResult::Continue
        }
        "/diff" => {
            cmd_diff();
            CommandResult::Continue
        }
        "/undo" => {
            cmd_undo();
            CommandResult::Continue
        }
        "/health" => {
            cmd_health();
            CommandResult::Continue
        }
        "/history" => {
            cmd_history(agent);
            CommandResult::Continue
        }
        "/config" => {
            cmd_config(agent, state);
            CommandResult::Continue
        }
        "/compact" => {
            cmd_compact(agent);
            CommandResult::Continue
        }
        "/context" => {
            cmd_context();
            CommandResult::Continue
        }
        "/init" => {
            cmd_init();
            CommandResult::Continue
        }
        "/run" => {
            cmd_run(input);
            CommandResult::Continue
        }
        s if s.starts_with("/run ") || (s.starts_with('!') && s.len() > 1) => {
            cmd_run(s);
            CommandResult::Continue
        }
        s if s.starts_with('/') && is_unknown_command(s) => {
            cmd_unknown(s);
            CommandResult::Continue
        }
        _ => CommandResult::NotACommand,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_quit() {
        assert!(matches!(categorize_command("/quit"), CommandCategory::Quit));
        assert!(matches!(categorize_command("/exit"), CommandCategory::Quit));
    }

    #[test]
    fn test_dispatch_simple_commands() {
        assert!(matches!(
            categorize_command("/help"),
            CommandCategory::Simple
        ));
        assert!(matches!(
            categorize_command("/version"),
            CommandCategory::Simple
        ));
        assert!(matches!(
            categorize_command("/diff"),
            CommandCategory::Simple
        ));
        assert!(matches!(
            categorize_command("/undo"),
            CommandCategory::Simple
        ));
        assert!(matches!(
            categorize_command("/health"),
            CommandCategory::Simple
        ));
        assert!(matches!(
            categorize_command("/context"),
            CommandCategory::Simple
        ));
        assert!(matches!(
            categorize_command("/init"),
            CommandCategory::Simple
        ));
    }

    #[test]
    fn test_dispatch_state_commands() {
        assert!(matches!(
            categorize_command("/status"),
            CommandCategory::NeedsState
        ));
        assert!(matches!(
            categorize_command("/tokens"),
            CommandCategory::NeedsState
        ));
        assert!(matches!(
            categorize_command("/cost"),
            CommandCategory::NeedsState
        ));
        assert!(matches!(
            categorize_command("/config"),
            CommandCategory::NeedsState
        ));
        assert!(matches!(
            categorize_command("/model"),
            CommandCategory::NeedsState
        ));
        assert!(matches!(
            categorize_command("/model claude-opus-4-6"),
            CommandCategory::NeedsState
        ));
    }

    #[test]
    fn test_dispatch_not_a_command() {
        assert!(matches!(
            categorize_command("hello world"),
            CommandCategory::NotACommand
        ));
        assert!(matches!(
            categorize_command("what is rust?"),
            CommandCategory::NotACommand
        ));
    }

    #[test]
    fn test_dispatch_run_commands() {
        assert!(matches!(
            categorize_command("/run echo hello"),
            CommandCategory::Simple
        ));
        assert!(matches!(categorize_command("!ls"), CommandCategory::Simple));
        assert!(matches!(
            categorize_command("/run"),
            CommandCategory::Simple
        ));
    }

    #[test]
    fn test_dispatch_unknown_command() {
        assert!(matches!(
            categorize_command("/foobar"),
            CommandCategory::Simple
        ));
        assert!(matches!(
            categorize_command("/xyz something"),
            CommandCategory::Simple
        ));
    }

    /// Lightweight category for testing dispatch logic without Agent.
    #[derive(Debug)]
    enum CommandCategory {
        Quit,
        Simple,
        NeedsState,
        NotACommand,
    }

    /// Categorize a command without executing it — mirrors `dispatch_command` logic.
    fn categorize_command(input: &str) -> CommandCategory {
        match input {
            "/quit" | "/exit" => CommandCategory::Quit,
            "/help" | "/version" | "/diff" | "/undo" | "/health" | "/context" | "/init" => {
                CommandCategory::Simple
            }
            "/status" | "/tokens" | "/cost" | "/clear" | "/model" | "/think" | "/config"
            | "/compact" | "/history" => CommandCategory::NeedsState,
            s if s.starts_with("/model ") => CommandCategory::NeedsState,
            s if s.starts_with("/think ") => CommandCategory::NeedsState,
            s if s == "/save" || s.starts_with("/save ") => CommandCategory::NeedsState,
            s if s == "/load" || s.starts_with("/load ") => CommandCategory::NeedsState,
            "/retry" => CommandCategory::NeedsState,
            "/run" => CommandCategory::Simple,
            s if s.starts_with("/run ") || (s.starts_with('!') && s.len() > 1) => {
                CommandCategory::Simple
            }
            s if s.starts_with('/') && is_unknown_command(s) => CommandCategory::Simple,
            _ => CommandCategory::NotACommand,
        }
    }
}
