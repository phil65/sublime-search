// Locationless diff parsing and application module.
//
// This module provides functionality to parse unified diff format without
// line numbers (locationless diffs) and apply them to content using
// fuzzy matching strategies.

use crate::content_replacer::{replace_content, ReplaceError};
use regex::Regex;
use std::sync::LazyLock;

/// A single diff hunk representing one edit operation.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffHunk {
    /// The text to find/replace (context + removed lines).
    pub old_text: String,
    /// The replacement text (context + added lines).
    pub new_text: String,
    /// The raw diff text for this hunk.
    pub raw: String,
}

/// Result of applying a single hunk.
#[derive(Debug, Clone)]
pub struct HunkResult {
    /// Whether the hunk was successfully applied.
    pub success: bool,
    /// The strategy that succeeded (if any).
    pub strategy: Option<String>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Start line of the match (1-based).
    pub start_line: Option<usize>,
    /// End line of the match (1-based).
    pub end_line: Option<usize>,
}

/// Result of applying all diff hunks.
#[derive(Debug, Clone)]
pub struct ApplyDiffResult {
    /// The final content after applying hunks.
    pub content: String,
    /// Number of hunks successfully applied.
    pub applied_count: usize,
    /// Total number of hunks parsed.
    pub total_count: usize,
    /// Results for each hunk.
    pub hunk_results: Vec<HunkResult>,
    /// Whether all hunks were applied successfully.
    pub all_applied: bool,
}

/// Error types for diff application.
#[derive(Debug, Clone)]
pub enum ApplyDiffError {
    /// No diff hunks found in the input.
    NoHunksFound,
    /// No hunks could be applied.
    NoHunksApplied { message: String },
}

// Compiled regex patterns for diff extraction
static DIFF_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<diff>(.*?)</diff>").unwrap());
static CODE_BLOCK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```diff\n?(.*?)```").unwrap());

/// Parse a locationless unified diff into old/new text pairs.
///
/// Handles diff format without line numbers - the location is inferred
/// by matching context in the file.
///
/// Format expected:
/// ```text
///  context line (unchanged)
/// -removed line
/// +added line
///  more context
/// ```
///
/// Multiple hunks are separated by:
/// - Blank lines (empty line not starting with space)
/// - Non-diff content lines
///
/// # Arguments
/// * `diff_text` - The diff text (may contain multiple hunks)
///
/// # Returns
/// List of DiffHunk objects with old_text/new_text pairs
pub fn parse_locationless_diff(diff_text: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();

    // Extract content between <diff> tags if present
    let diff_text = if let Some(caps) = DIFF_TAG_REGEX.captures(diff_text) {
        caps.get(1).map_or(diff_text, |m| m.as_str())
    } else {
        diff_text
    };

    // Also handle ```diff ... ``` code blocks
    let diff_text = if let Some(caps) = CODE_BLOCK_REGEX.captures(diff_text) {
        caps.get(1).map_or(diff_text, |m| m.as_str())
    } else {
        diff_text
    };

    // Strip only leading/trailing newlines, not spaces (which are meaningful in diffs)
    let diff_text = diff_text.trim_matches(|c| c == '\n' || c == '\r');

    let mut current_hunk_lines: Vec<&str> = Vec::new();

    for line in diff_text.lines() {
        // Skip standard diff headers
        if line.starts_with("---")
            || line.starts_with("+++")
            || line.starts_with("@@")
            || line.starts_with("diff --git")
            || line.starts_with("index ")
        {
            continue;
        }

        // Check if this is a diff line (starts with +, -, or space for context)
        let is_diff_line = line.starts_with('+') || line.starts_with('-') || line.starts_with(' ');

        // Empty line (not starting with space) = hunk separator
        if line.is_empty() || (!is_diff_line && line.trim().is_empty()) {
            if !current_hunk_lines.is_empty() {
                if let Some(hunk) = parse_single_hunk(&current_hunk_lines) {
                    hunks.push(hunk);
                }
                current_hunk_lines.clear();
            }
            continue;
        }

        // Non-diff content line = hunk separator
        if !is_diff_line && !line.trim().is_empty() {
            if !current_hunk_lines.is_empty() {
                if let Some(hunk) = parse_single_hunk(&current_hunk_lines) {
                    hunks.push(hunk);
                }
                current_hunk_lines.clear();
            }
            continue;
        }

        // Accumulate diff lines
        current_hunk_lines.push(line);
    }

    // Don't forget the last hunk
    if !current_hunk_lines.is_empty() {
        if let Some(hunk) = parse_single_hunk(&current_hunk_lines) {
            hunks.push(hunk);
        }
    }

    hunks
}

/// Parse a single diff hunk into old/new text.
///
/// # Arguments
/// * `lines` - Lines of the hunk (each starting with +, -, or space)
///
/// # Returns
/// DiffHunk or None if the hunk is empty/invalid
fn parse_single_hunk(lines: &[&str]) -> Option<DiffHunk> {
    let mut old_lines: Vec<&str> = Vec::new();
    let mut new_lines: Vec<&str> = Vec::new();

    for line in lines {
        if let Some(rest) = line.strip_prefix('-') {
            // Removed line - only in old
            old_lines.push(rest);
        } else if let Some(rest) = line.strip_prefix('+') {
            // Added line - only in new
            new_lines.push(rest);
        } else if let Some(rest) = line.strip_prefix(' ') {
            // Context line - in both
            old_lines.push(rest);
            new_lines.push(rest);
        } else if line.is_empty() {
            // Empty context line (just a space that got trimmed)
            old_lines.push("");
            new_lines.push("");
        }
        // Skip lines that don't match the pattern
    }

    if old_lines.is_empty() && new_lines.is_empty() {
        return None;
    }

    let old_text = old_lines.join("\n");
    let new_text = new_lines.join("\n");
    let raw = lines.join("\n");

    Some(DiffHunk {
        old_text,
        new_text,
        raw,
    })
}

/// Apply locationless diff edits to content.
///
/// Parses diff format and applies each hunk using content matching
/// via the multi-strategy replace_content function.
///
/// # Arguments
/// * `content` - The original file content
/// * `diff_text` - The diff text containing hunks to apply
/// * `replace_all` - Whether to replace all occurrences of each pattern
///
/// # Returns
/// Result with the modified content or an error
pub fn apply_diff_hunks(
    content: &str,
    diff_text: &str,
    replace_all: bool,
) -> Result<ApplyDiffResult, ApplyDiffError> {
    let hunks = parse_locationless_diff(diff_text);

    if hunks.is_empty() {
        return Err(ApplyDiffError::NoHunksFound);
    }

    let total_count = hunks.len();
    let mut current_content = content.to_string();
    let mut applied_count = 0;
    let mut hunk_results = Vec::with_capacity(total_count);
    let mut failed_previews: Vec<String> = Vec::new();

    for hunk in &hunks {
        // Skip pure insertions without context
        if hunk.old_text.trim().is_empty() {
            hunk_results.push(HunkResult {
                success: false,
                strategy: None,
                error: Some("Pure insertion hunk (no context) - skipped".to_string()),
                start_line: None,
                end_line: None,
            });
            continue;
        }

        // Try to apply this hunk using replace_content
        match replace_content(
            &current_content,
            &hunk.old_text,
            &hunk.new_text,
            replace_all,
            None,
        ) {
            Ok(result) => {
                current_content = result.content;
                applied_count += 1;
                hunk_results.push(HunkResult {
                    success: true,
                    strategy: Some(result.strategy),
                    error: None,
                    start_line: Some(result.start_line),
                    end_line: Some(result.end_line),
                });
            }
            Err(e) => {
                let error_msg = match &e {
                    ReplaceError::NotFound { message, .. } => message.clone(),
                    ReplaceError::MultipleMatches { message, .. } => message.clone(),
                    ReplaceError::NoChange => "old_text and new_text are identical".to_string(),
                };

                // Collect preview of failed hunk for error reporting
                let preview: String = hunk.old_text.chars().take(50).collect();
                failed_previews.push(if hunk.old_text.len() > 50 {
                    format!("{}...", preview)
                } else {
                    preview
                });

                hunk_results.push(HunkResult {
                    success: false,
                    strategy: None,
                    error: Some(error_msg),
                    start_line: None,
                    end_line: None,
                });
            }
        }
    }

    let all_applied = applied_count == total_count;

    if applied_count == 0 {
        let message = format!(
            "None of the {} diff hunks could be applied. \
             The context lines don't match the current file content. \
             Please read the file again and provide accurate diff context.",
            total_count
        );
        return Err(ApplyDiffError::NoHunksApplied { message });
    }

    // Return success even for partial application - let caller decide how to handle
    Ok(ApplyDiffResult {
        content: current_content,
        applied_count,
        total_count,
        hunk_results,
        all_applied,
    })
}

/// Apply diff hunks with line hint for disambiguation.
///
/// Similar to apply_diff_hunks but accepts an optional line hint
/// to help disambiguate when multiple matches exist.
///
/// # Arguments
/// * `content` - The original file content
/// * `diff_text` - The diff text containing hunks to apply
/// * `line_hint` - Optional line number hint for disambiguation
///
/// # Returns
/// Result with the modified content or an error
pub fn apply_diff_hunks_with_hint(
    content: &str,
    diff_text: &str,
    line_hint: Option<u32>,
) -> Result<ApplyDiffResult, ApplyDiffError> {
    let hunks = parse_locationless_diff(diff_text);

    if hunks.is_empty() {
        return Err(ApplyDiffError::NoHunksFound);
    }

    let total_count = hunks.len();
    let mut current_content = content.to_string();
    let mut applied_count = 0;
    let mut hunk_results = Vec::with_capacity(total_count);
    let mut failed_previews: Vec<String> = Vec::new();

    for hunk in &hunks {
        // Skip pure insertions without context
        if hunk.old_text.trim().is_empty() {
            hunk_results.push(HunkResult {
                success: false,
                strategy: None,
                error: Some("Pure insertion hunk (no context) - skipped".to_string()),
                start_line: None,
                end_line: None,
            });
            continue;
        }

        // Try to apply this hunk using replace_content with line hint
        match replace_content(
            &current_content,
            &hunk.old_text,
            &hunk.new_text,
            false,
            line_hint,
        ) {
            Ok(result) => {
                current_content = result.content;
                applied_count += 1;
                hunk_results.push(HunkResult {
                    success: true,
                    strategy: Some(result.strategy),
                    error: None,
                    start_line: Some(result.start_line),
                    end_line: Some(result.end_line),
                });
            }
            Err(e) => {
                let error_msg = match &e {
                    ReplaceError::NotFound { message, .. } => message.clone(),
                    ReplaceError::MultipleMatches { message, .. } => message.clone(),
                    ReplaceError::NoChange => "old_text and new_text are identical".to_string(),
                };

                let preview: String = hunk.old_text.chars().take(50).collect();
                failed_previews.push(if hunk.old_text.len() > 50 {
                    format!("{}...", preview)
                } else {
                    preview
                });

                hunk_results.push(HunkResult {
                    success: false,
                    strategy: None,
                    error: Some(error_msg),
                    start_line: None,
                    end_line: None,
                });
            }
        }
    }

    let all_applied = applied_count == total_count;

    if applied_count == 0 {
        let message = format!(
            "None of the {} diff hunks could be applied. \
             The context lines don't match the current file content. \
             Please read the file again and provide accurate diff context.",
            total_count
        );
        return Err(ApplyDiffError::NoHunksApplied { message });
    }

    Ok(ApplyDiffResult {
        content: current_content,
        applied_count,
        total_count,
        hunk_results,
        all_applied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_hunk() {
        let diff = " context\n-old line\n+new line\n context2";
        let hunks = parse_locationless_diff(diff);

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_text, "context\nold line\ncontext2");
        assert_eq!(hunks[0].new_text, "context\nnew line\ncontext2");
    }

    #[test]
    fn test_parse_multiple_hunks() {
        let diff = " ctx1\n-old1\n+new1\n\n ctx2\n-old2\n+new2";
        let hunks = parse_locationless_diff(diff);

        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[0].old_text, "ctx1\nold1");
        assert_eq!(hunks[0].new_text, "ctx1\nnew1");
        assert_eq!(hunks[1].old_text, "ctx2\nold2");
        assert_eq!(hunks[1].new_text, "ctx2\nnew2");
    }

    #[test]
    fn test_parse_with_diff_tags() {
        let diff = "Some preamble\n<diff>\n context\n-old\n+new\n</diff>\nSome postamble";
        let hunks = parse_locationless_diff(diff);

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_text, "context\nold");
        assert_eq!(hunks[0].new_text, "context\nnew");
    }

    #[test]
    fn test_parse_with_code_block() {
        let diff = "```diff\n context\n-old\n+new\n```";
        let hunks = parse_locationless_diff(diff);

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_text, "context\nold");
        assert_eq!(hunks[0].new_text, "context\nnew");
    }

    #[test]
    fn test_parse_skips_diff_headers() {
        let diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n context\n-old\n+new";
        let hunks = parse_locationless_diff(diff);

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_text, "context\nold");
        assert_eq!(hunks[0].new_text, "context\nnew");
    }

    #[test]
    fn test_parse_pure_addition() {
        let diff = " context\n+new line\n context2";
        let hunks = parse_locationless_diff(diff);

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_text, "context\ncontext2");
        assert_eq!(hunks[0].new_text, "context\nnew line\ncontext2");
    }

    #[test]
    fn test_parse_pure_deletion() {
        let diff = " context\n-deleted line\n context2";
        let hunks = parse_locationless_diff(diff);

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_text, "context\ndeleted line\ncontext2");
        assert_eq!(hunks[0].new_text, "context\ncontext2");
    }

    #[test]
    fn test_parse_empty_returns_empty() {
        let diff = "";
        let hunks = parse_locationless_diff(diff);
        assert!(hunks.is_empty());
    }

    #[test]
    fn test_parse_no_diff_lines_returns_empty() {
        let diff = "Just some regular text\nwithout any diff markers";
        let hunks = parse_locationless_diff(diff);
        assert!(hunks.is_empty());
    }

    #[test]
    fn test_apply_simple_diff() {
        let content = "line1\nold line\nline3";
        let diff = " line1\n-old line\n+new line\n line3";

        let result = apply_diff_hunks(content, diff, false).unwrap();

        assert_eq!(result.content, "line1\nnew line\nline3");
        assert_eq!(result.applied_count, 1);
        assert_eq!(result.total_count, 1);
        assert!(result.all_applied);
    }

    #[test]
    fn test_apply_multiple_hunks() {
        let content = "aaa\nbbb\nccc\nddd\neee";
        let diff = " aaa\n-bbb\n+BBB\n ccc\n\n ccc\n-ddd\n+DDD\n eee";

        let result = apply_diff_hunks(content, diff, false).unwrap();

        assert_eq!(result.content, "aaa\nBBB\nccc\nDDD\neee");
        assert_eq!(result.applied_count, 2);
        assert!(result.all_applied);
    }

    #[test]
    fn test_apply_no_hunks_error() {
        let content = "some content";
        let diff = "no diff markers here";

        let result = apply_diff_hunks(content, diff, false);
        assert!(matches!(result, Err(ApplyDiffError::NoHunksFound)));
    }

    #[test]
    fn test_apply_no_match_error() {
        let content = "completely different content";
        let diff = " context\n-old\n+new";

        let result = apply_diff_hunks(content, diff, false);
        assert!(matches!(result, Err(ApplyDiffError::NoHunksApplied { .. })));
    }

    #[test]
    fn test_apply_partial_success() {
        let content = "line1\nmatch\nline3";
        let diff = " line1\n-match\n+MATCH\n\n nomatch\n-x\n+y";

        let result = apply_diff_hunks(content, diff, false).unwrap();

        // First hunk succeeds, second fails
        assert_eq!(result.applied_count, 1);
        assert_eq!(result.total_count, 2);
        assert!(!result.all_applied);
        assert_eq!(result.content, "line1\nMATCH\nline3");
    }

    #[test]
    fn test_hunk_results_detail() {
        let content = "ctx\nold\nctx2";
        let diff = " ctx\n-old\n+new\n ctx2";

        let result = apply_diff_hunks(content, diff, false).unwrap();

        assert_eq!(result.hunk_results.len(), 1);
        assert!(result.hunk_results[0].success);
        assert!(result.hunk_results[0].strategy.is_some());
        assert!(result.hunk_results[0].start_line.is_some());
    }

    #[test]
    fn test_pure_insertion_skipped() {
        // Pure insertion (only + lines, no context) should be skipped
        let content = "existing content";
        let diff = "+brand new line";

        // This should fail because the only hunk is a pure insertion
        let result = apply_diff_hunks(content, diff, false);
        assert!(matches!(result, Err(ApplyDiffError::NoHunksApplied { .. })));
    }

    #[test]
    fn test_preserves_indentation() {
        let content = "def foo():\n    old_code()\n    more()";
        let diff = " def foo():\n-    old_code()\n+    new_code()\n     more()";

        let result = apply_diff_hunks(content, diff, false).unwrap();

        assert!(result.content.contains("    new_code()"));
    }
}
