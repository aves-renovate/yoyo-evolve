//! Prompt execution and agent interaction.

use crate::cli::is_verbose;
use crate::format::*;
use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Instant;
use yoagent::agent::Agent;
use yoagent::*;

/// Extract a preview of tool result content for display.
/// Returns an empty string if there's nothing meaningful to show.
fn tool_result_preview(result: &ToolResult, max_chars: usize) -> String {
    let text: String = result
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let text = text.trim();
    if text.is_empty() {
        return String::new();
    }
    // Take first line only, truncated
    let first_line = text.lines().next().unwrap_or("");
    truncate_with_ellipsis(first_line, max_chars)
}

/// Write response text to a file if --output was specified.
pub fn write_output_file(path: &Option<String>, text: &str) {
    if let Some(path) = path {
        match std::fs::write(path, text) {
            Ok(_) => eprintln!("{DIM}  wrote response to {path}{RESET}"),
            Err(e) => eprintln!("{RED}  error writing to {path}: {e}{RESET}"),
        }
    }
}

/// A search result from conversation history.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// 1-based message index
    pub index: usize,
    /// Role: "user", "assistant", "tool", "ext"
    pub role: &'static str,
    /// Context window around the match (truncated)
    pub excerpt: String,
}

/// Extract all searchable text from a message.
pub fn extract_message_text(msg: &AgentMessage) -> String {
    match msg {
        AgentMessage::Llm(Message::User { content, .. }) => content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
        AgentMessage::Llm(Message::Assistant { content, .. }) => content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                Content::ToolCall { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" "),
        AgentMessage::Llm(Message::ToolResult {
            content, tool_name, ..
        }) => {
            let mut parts = vec![tool_name.as_str()];
            for c in content {
                if let Content::Text { text } = c {
                    parts.push(text.as_str());
                }
            }
            parts.join(" ")
        }
        AgentMessage::Extension(ext) => ext.role.clone(),
    }
}

/// Get the role label for a message.
fn message_role(msg: &AgentMessage) -> &'static str {
    match msg {
        AgentMessage::Llm(Message::User { .. }) => "user",
        AgentMessage::Llm(Message::Assistant { .. }) => "assistant",
        AgentMessage::Llm(Message::ToolResult { .. }) => "tool",
        AgentMessage::Extension(_) => "ext",
    }
}

/// Build an excerpt around the first match of `query_lower` in `text`.
/// Shows up to `context_chars` characters around the match.
fn build_excerpt(text: &str, query_lower: &str, context_chars: usize) -> String {
    let text_lower = text.to_lowercase();
    if let Some(pos) = text_lower.find(query_lower) {
        let start = pos.saturating_sub(context_chars);
        let end = (pos + query_lower.len() + context_chars).min(text.len());
        // Adjust to char boundaries
        let start = text
            .char_indices()
            .map(|(i, _)| i)
            .find(|&i| i >= start)
            .unwrap_or(0);
        let end = text
            .char_indices()
            .map(|(i, c)| i + c.len_utf8())
            .find(|&i| i >= end)
            .unwrap_or(text.len());
        let mut excerpt = String::new();
        if start > 0 {
            excerpt.push('…');
        }
        // Take the slice and collapse newlines to spaces
        let slice = &text[start..end];
        let collapsed: String = slice
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .collect();
        excerpt.push_str(collapsed.trim());
        if end < text.len() {
            excerpt.push('…');
        }
        excerpt
    } else {
        String::new()
    }
}

/// Search conversation messages for a query string (case-insensitive).
/// Returns up to `max_results` matches with message index, role, and excerpt.
pub fn search_messages(
    messages: &[AgentMessage],
    query: &str,
    max_results: usize,
) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        let text = extract_message_text(msg);
        if text.to_lowercase().contains(&query_lower) {
            let excerpt = build_excerpt(&text, &query_lower, 40);
            results.push(SearchResult {
                index: i + 1,
                role: message_role(msg),
                excerpt,
            });
            if results.len() >= max_results {
                break;
            }
        }
    }

    results
}

/// Summarize a message for /history display.
pub fn summarize_message(msg: &AgentMessage) -> (&str, String) {
    match msg {
        AgentMessage::Llm(Message::User { content, .. }) => {
            let text = content
                .iter()
                .filter_map(|c| match c {
                    Content::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ");
            ("user", truncate_with_ellipsis(&text, 80))
        }
        AgentMessage::Llm(Message::Assistant { content, .. }) => {
            let mut parts = Vec::new();
            let mut tool_calls = 0;
            for c in content {
                match c {
                    Content::Text { text } if !text.is_empty() => {
                        parts.push(truncate_with_ellipsis(text, 60));
                    }
                    Content::ToolCall { name, .. } => {
                        tool_calls += 1;
                        if tool_calls <= 3 {
                            parts.push(format!("→{name}"));
                        }
                    }
                    _ => {}
                }
            }
            if tool_calls > 3 {
                parts.push(format!("(+{} more tools)", tool_calls - 3));
            }
            let preview = if parts.is_empty() {
                "(empty)".to_string()
            } else {
                parts.join("  ")
            };
            ("assistant", preview)
        }
        AgentMessage::Llm(Message::ToolResult {
            tool_name,
            is_error,
            ..
        }) => {
            let status = if *is_error { "✗" } else { "✓" };
            ("tool", format!("{tool_name} {status}"))
        }
        AgentMessage::Extension(ext) => ("ext", truncate_with_ellipsis(&ext.role, 60)),
    }
}

pub async fn run_prompt(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) -> String {
    let prompt_start = Instant::now();
    let mut rx = agent.prompt(input).await;
    let mut last_usage = Usage::default();
    let mut in_text = false;
    let mut tool_timers: HashMap<String, Instant> = HashMap::new();
    let mut collected_text = String::new();
    let mut turn_count: usize = 0;

    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    AgentEvent::TurnStart => {
                        turn_count += 1;
                        if turn_count >= 2 {
                            if in_text {
                                println!();
                                in_text = false;
                            }
                            println!("{DIM}  turn {turn_count}{RESET}");
                        }
                    }
                    AgentEvent::ToolExecutionStart {
                        tool_call_id, tool_name, args, ..
                    } => {
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        tool_timers.insert(tool_call_id.clone(), Instant::now());
                        let summary = format_tool_summary(&tool_name, &args);
                        print!("{YELLOW}  ▶ {summary}{RESET}");
                        if is_verbose() {
                            // Show full tool args in verbose mode
                            println!();
                            let args_str = serde_json::to_string_pretty(&args).unwrap_or_default();
                            for line in args_str.lines() {
                                println!("{DIM}    {line}{RESET}");
                            }
                        }
                        io::stdout().flush().ok();
                    }
                    AgentEvent::ToolExecutionEnd { tool_call_id, is_error, result, .. } => {
                        let duration = tool_timers
                            .remove(&tool_call_id)
                            .map(|start| format_duration(start.elapsed()));
                        let dur_str = duration
                            .map(|d| format!(" {DIM}({d}){RESET}"))
                            .unwrap_or_default();
                        if is_error {
                            println!(" {RED}✗{RESET}{dur_str}");
                            // Show error preview so user can see what went wrong
                            let preview = tool_result_preview(&result, 200);
                            if !preview.is_empty() {
                                println!("{DIM}    {preview}{RESET}");
                            }
                        } else {
                            println!(" {GREEN}✓{RESET}{dur_str}");
                            // In verbose mode, show a preview of successful results too
                            if is_verbose() {
                                let preview = tool_result_preview(&result, 200);
                                if !preview.is_empty() {
                                    println!("{DIM}    {preview}{RESET}");
                                }
                            }
                        }
                    }
                    AgentEvent::ToolExecutionUpdate { partial_result, .. } => {
                        // Stream partial results from tools (MCP servers, sub-agents)
                        let preview = tool_result_preview(&partial_result, 500);
                        if !preview.is_empty() {
                            print!("{DIM}{preview}{RESET}");
                            io::stdout().flush().ok();
                        }
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Text { delta },
                        ..
                    } => {
                        if !in_text {
                            println!();
                            in_text = true;
                        }
                        collected_text.push_str(&delta);
                        print!("{}", delta);
                        io::stdout().flush().ok();
                    }
                    AgentEvent::MessageUpdate {
                        delta: StreamDelta::Thinking { delta },
                        ..
                    } => {
                        // Show thinking output dimmed so user can follow the reasoning
                        print!("{DIM}{delta}{RESET}");
                        io::stdout().flush().ok();
                    }
                    AgentEvent::AgentEnd { messages } => {
                        // Sum usage across ALL assistant messages in this turn
                        // (a single prompt can trigger multiple LLM calls via tool loops)
                        for msg in &messages {
                            if let AgentMessage::Llm(Message::Assistant { usage, stop_reason, error_message, .. }) = msg {
                                last_usage.input += usage.input;
                                last_usage.output += usage.output;
                                last_usage.cache_read += usage.cache_read;
                                last_usage.cache_write += usage.cache_write;

                                // Show error stop reasons to the user
                                if *stop_reason == StopReason::Error {
                                    if let Some(err_msg) = error_message {
                                        if in_text {
                                            println!();
                                            in_text = false;
                                        }
                                        eprintln!("\n{RED}  error: {err_msg}{RESET}");
                                    }
                                }
                            }
                        }
                    }
                    AgentEvent::InputRejected { reason } => {
                        eprintln!("{RED}  input rejected: {reason}{RESET}");
                    }
                    AgentEvent::ProgressMessage { text, .. } => {
                        if in_text {
                            println!();
                            in_text = false;
                        }
                        println!("{DIM}  {text}{RESET}");
                    }
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                // Cancel the agent's background work (tool execution, API calls)
                agent.abort();
                if in_text {
                    println!();
                }
                println!("\n{DIM}  (interrupted — press Ctrl+C again to exit){RESET}");
                break;
            }
        }
    }

    if in_text {
        println!();
    }
    session_total.input += last_usage.input;
    session_total.output += last_usage.output;
    session_total.cache_read += last_usage.cache_read;
    session_total.cache_write += last_usage.cache_write;
    print_usage(&last_usage, session_total, model, prompt_start.elapsed());
    println!();
    collected_text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_message_user() {
        let msg = AgentMessage::Llm(Message::user("hello world, this is a test"));
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "user");
        assert!(preview.contains("hello world"));
    }

    #[test]
    fn test_summarize_message_tool_result() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_1".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "output".into(),
            }],
            is_error: false,
            timestamp: 0,
        });
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "tool");
        assert!(preview.contains("bash"));
        assert!(preview.contains("✓"));
    }

    #[test]
    fn test_summarize_message_tool_result_error() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_2".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "error".into(),
            }],
            is_error: true,
            timestamp: 0,
        });
        let (role, preview) = summarize_message(&msg);
        assert_eq!(role, "tool");
        assert!(preview.contains("✗"));
    }

    #[test]
    fn test_write_output_file_none() {
        write_output_file(&None, "test content");
        // No assertion needed — just verify it doesn't panic
    }

    #[test]
    fn test_write_output_file_some() {
        let dir = std::env::temp_dir().join("yoyo_test_output");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_output.txt");
        let path_str = path.to_string_lossy().to_string();
        write_output_file(&Some(path_str), "hello from yoyo");
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello from yoyo");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_tool_result_preview_empty() {
        let result = ToolResult {
            content: vec![],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "");
    }

    #[test]
    fn test_tool_result_preview_text() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "error: file not found".into(),
            }],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "error: file not found");
    }

    #[test]
    fn test_tool_result_preview_truncated() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "a".repeat(200),
            }],
            details: serde_json::json!(null),
        };
        let preview = tool_result_preview(&result, 50);
        assert!(preview.len() < 100);
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn test_tool_result_preview_multiline() {
        let result = ToolResult {
            content: vec![Content::Text {
                text: "first line\nsecond line\nthird line".into(),
            }],
            details: serde_json::json!(null),
        };
        assert_eq!(tool_result_preview(&result, 100), "first line");
    }

    #[test]
    fn test_search_messages_basic() {
        let messages = vec![
            AgentMessage::Llm(Message::user("hello world")),
            AgentMessage::Llm(Message::user("goodbye world")),
            AgentMessage::Llm(Message::user("hello again")),
        ];
        let results = search_messages(&messages, "hello", 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].index, 1);
        assert_eq!(results[0].role, "user");
        assert!(results[0].excerpt.contains("hello"));
        assert_eq!(results[1].index, 3);
    }

    #[test]
    fn test_search_messages_case_insensitive() {
        let messages = vec![
            AgentMessage::Llm(Message::user("Hello World")),
            AgentMessage::Llm(Message::user("HELLO AGAIN")),
        ];
        let results = search_messages(&messages, "hello", 10);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_messages_no_match() {
        let messages = vec![AgentMessage::Llm(Message::user("hello world"))];
        let results = search_messages(&messages, "xyz", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_messages_max_results() {
        let messages: Vec<AgentMessage> = (0..20)
            .map(|i| AgentMessage::Llm(Message::user(format!("message {i} with keyword"))))
            .collect();
        let results = search_messages(&messages, "keyword", 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_search_messages_empty() {
        let results = search_messages(&[], "test", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_messages_tool_result() {
        let messages = vec![AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_1".into(),
            tool_name: "bash".into(),
            content: vec![Content::Text {
                text: "error: file not found".into(),
            }],
            is_error: true,
            timestamp: 0,
        })];
        let results = search_messages(&messages, "file not found", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].role, "tool");
        assert!(results[0].excerpt.contains("file not found"));
    }

    #[test]
    fn test_extract_message_text_user() {
        let msg = AgentMessage::Llm(Message::user("test content"));
        let text = extract_message_text(&msg);
        assert_eq!(text, "test content");
    }

    #[test]
    fn test_extract_message_text_tool_result() {
        let msg = AgentMessage::Llm(Message::ToolResult {
            tool_call_id: "tc_1".into(),
            tool_name: "read_file".into(),
            content: vec![Content::Text {
                text: "file contents here".into(),
            }],
            is_error: false,
            timestamp: 0,
        });
        let text = extract_message_text(&msg);
        assert!(text.contains("read_file"));
        assert!(text.contains("file contents here"));
    }

    #[test]
    fn test_build_excerpt_basic() {
        let text = "The quick brown fox jumps over the lazy dog";
        let excerpt = build_excerpt(text, "fox", 10);
        assert!(excerpt.contains("fox"));
        // Should have ellipsis since match is in the middle
        assert!(excerpt.contains("…"));
    }

    #[test]
    fn test_build_excerpt_at_start() {
        let text = "hello world test";
        let excerpt = build_excerpt(text, "hello", 40);
        assert!(excerpt.starts_with("hello"));
    }

    #[test]
    fn test_build_excerpt_no_match() {
        let text = "hello world";
        let excerpt = build_excerpt(text, "xyz", 40);
        assert!(excerpt.is_empty());
    }

    /// Format a turn indicator string, returning None for turn 1 (skip it).
    /// This mirrors the logic in run_prompt's TurnStart handler.
    #[cfg(test)]
    fn format_turn_indicator(turn_count: usize) -> Option<String> {
        if turn_count >= 2 {
            Some(format!("{DIM}  turn {turn_count}{RESET}"))
        } else {
            None
        }
    }

    #[test]
    fn test_turn_counter_skips_turn_1() {
        // Turn 1 should produce no output (it's obvious)
        assert!(format_turn_indicator(1).is_none());
    }

    #[test]
    fn test_turn_counter_shows_turn_2() {
        let indicator = format_turn_indicator(2);
        assert!(indicator.is_some());
        let text = indicator.unwrap();
        assert!(text.contains("turn 2"));
    }

    #[test]
    fn test_turn_counter_shows_turn_5() {
        let indicator = format_turn_indicator(5);
        assert!(indicator.is_some());
        let text = indicator.unwrap();
        assert!(text.contains("turn 5"));
    }

    #[test]
    fn test_turn_counter_zero_skipped() {
        // Edge case: turn 0 should also be skipped
        assert!(format_turn_indicator(0).is_none());
    }

    #[test]
    fn test_turn_counter_increments_correctly() {
        // Simulate the event loop logic
        let mut turn_count: usize = 0;
        let mut displayed = Vec::new();

        for _ in 0..4 {
            turn_count += 1;
            if let Some(indicator) = format_turn_indicator(turn_count) {
                displayed.push(indicator);
            }
        }

        // Turn 1 skipped, turns 2-4 displayed
        assert_eq!(displayed.len(), 3);
        assert!(displayed[0].contains("turn 2"));
        assert!(displayed[1].contains("turn 3"));
        assert!(displayed[2].contains("turn 4"));
    }
}
