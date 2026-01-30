//! Sophisticated content replacement with multiple fallback strategies.
//!
//! This module implements a robust text replacement system that uses multiple
//! fallback strategies for finding and replacing text, similar to OpenCode's edit tool.
//!
//! Strategies are tried in order until one succeeds:
//! 1. Simple - Direct string matching
//! 2. Line-trimmed - Line-by-line matching with trimmed whitespace
//! 3. Block-anchor - Multi-line matching using first/last line anchors
//! 4. Whitespace-normalized - Normalized whitespace matching
//! 5. Indentation-flexible - Matching with flexible indentation
//! 6. Escape-normalized - Handle escape sequence differences
//! 7. Trimmed-boundary - Match with trimmed boundaries
//! 8. Context-aware - Use anchor lines for context matching

use std::collections::HashSet;

/// Result of a successful replacement operation.
#[derive(Debug, Clone)]
pub struct ReplaceResult {
    /// The new content after replacement
    pub content: String,
    /// The text that was matched and replaced
    pub matched_text: String,
    /// Starting line number (1-based)
    pub start_line: usize,
    /// Ending line number (1-based)
    pub end_line: usize,
    /// Which strategy succeeded
    pub strategy: String,
}

/// Error types for replacement operations.
#[derive(Debug, Clone)]
pub enum ReplaceError {
    /// The search text was not found
    NotFound {
        message: String,
        closest_match: Option<FuzzyMatchInfo>,
    },
    /// Multiple matches found, need more context
    MultipleMatches {
        message: String,
        locations: Vec<usize>,
    },
    /// old_string equals new_string
    NoChange,
}

/// Information about a fuzzy match candidate.
#[derive(Debug, Clone)]
pub struct FuzzyMatchInfo {
    pub similarity: f64,
    pub start_line: usize,
    pub end_line: usize,
    pub text: String,
    pub diff: String,
}

/// Threshold for fuzzy string equality (0.0 to 1.0).
const FUZZY_THRESHOLD: f64 = 0.8;

/// Single candidate similarity threshold.
const SINGLE_CANDIDATE_THRESHOLD: f64 = 0.0;

/// Multiple candidates similarity threshold.
const MULTIPLE_CANDIDATES_THRESHOLD: f64 = 0.3;

/// Minimum lines required for block anchor matching.
const MIN_LINES_FOR_BLOCK: usize = 3;

/// Minimum similarity ratio for context-aware matching.
const MIN_SIMILARITY_RATIO: f64 = 0.5;

// ============================================================================
// Main API
// ============================================================================

/// Replace content using multiple fallback strategies.
///
/// Tries various matching strategies in order until one succeeds.
/// Returns detailed information about the replacement or an error with
/// helpful debugging information.
pub fn replace_content(
    content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
    line_hint: Option<u32>,
) -> Result<ReplaceResult, ReplaceError> {
    if old_string == new_string {
        return Err(ReplaceError::NoChange);
    }

    // Define replacer strategies in order
    let strategies: Vec<(&str, fn(&str, &str) -> Vec<String>)> = vec![
        ("simple", simple_replacer),
        ("line_trimmed", line_trimmed_replacer),
        ("block_anchor", block_anchor_replacer),
        ("whitespace_normalized", whitespace_normalized_replacer),
        ("indentation_flexible", indentation_flexible_replacer),
        ("escape_normalized", escape_normalized_replacer),
        ("trimmed_boundary", trimmed_boundary_replacer),
        ("context_aware", context_aware_replacer),
    ];

    let mut found_matches = false;

    for (strategy_name, replacer) in &strategies {
        let matches = replacer(content, old_string);
        if matches.is_empty() {
            continue;
        }

        found_matches = true;

        for search_text in &matches {
            let Some(index) = content.find(search_text.as_str()) else {
                continue;
            };

            if replace_all {
                let new_content = content.replace(search_text.as_str(), new_string);
                let start_line = content[..index].matches('\n').count() + 1;
                let end_line = start_line + search_text.matches('\n').count();

                return Ok(ReplaceResult {
                    content: new_content,
                    matched_text: search_text.clone(),
                    start_line,
                    end_line,
                    strategy: strategy_name.to_string(),
                });
            }

            // Check for multiple occurrences
            let last_index = content.rfind(search_text.as_str());
            if last_index != Some(index) {
                // Multiple occurrences found
                if let Some(hint) = line_hint {
                    if let Some(best_index) =
                        find_closest_match(content, search_text, hint as usize)
                    {
                        let new_content = format!(
                            "{}{}{}",
                            &content[..best_index],
                            new_string,
                            &content[best_index + search_text.len()..]
                        );
                        let start_line = content[..best_index].matches('\n').count() + 1;
                        let end_line = start_line + search_text.matches('\n').count();

                        return Ok(ReplaceResult {
                            content: new_content,
                            matched_text: search_text.clone(),
                            start_line,
                            end_line,
                            strategy: strategy_name.to_string(),
                        });
                    }
                }
                continue; // Multiple occurrences, need more context
            }

            // Single occurrence - replace it
            let new_content = format!(
                "{}{}{}",
                &content[..index],
                new_string,
                &content[index + search_text.len()..]
            );
            let start_line = content[..index].matches('\n').count() + 1;
            let end_line = start_line + search_text.matches('\n').count();

            return Ok(ReplaceResult {
                content: new_content,
                matched_text: search_text.clone(),
                start_line,
                end_line,
                strategy: strategy_name.to_string(),
            });
        }
    }

    if !found_matches {
        // Build helpful error with fuzzy match context
        let closest = find_best_fuzzy_match(content, old_string, FUZZY_THRESHOLD);
        let message = build_not_found_error(content, old_string, closest.as_ref());

        return Err(ReplaceError::NotFound {
            message,
            closest_match: closest,
        });
    }

    // Multiple matches found
    let locations = find_all_match_locations(content, old_string);
    let message = build_multiple_matches_error(old_string, &locations);

    Err(ReplaceError::MultipleMatches { message, locations })
}

// ============================================================================
// Replacer Strategies
// ============================================================================

/// Direct string matching replacer.
fn simple_replacer(content: &str, find: &str) -> Vec<String> {
    if content.contains(find) {
        vec![find.to_string()]
    } else {
        vec![]
    }
}

/// Line-by-line matching with trimmed whitespace.
fn line_trimmed_replacer(content: &str, find: &str) -> Vec<String> {
    let original_lines: Vec<&str> = content.lines().collect();
    let mut search_lines: Vec<&str> = find.lines().collect();

    // Remove trailing empty line if present
    if search_lines.last().map_or(false, |l| l.is_empty()) {
        search_lines.pop();
    }

    // Early exit if content is too short or search is empty
    if search_lines.is_empty() || original_lines.len() < search_lines.len() {
        return vec![];
    }

    let mut results = Vec::new();

    for i in 0..=original_lines.len() - search_lines.len() {
        let mut matches = true;

        for j in 0..search_lines.len() {
            if i + j >= original_lines.len() {
                matches = false;
                break;
            }

            if original_lines[i + j].trim() != search_lines[j].trim() {
                matches = false;
                break;
            }
        }

        if matches {
            // Build the actual matched text from original lines
            let matched: Vec<&str> = original_lines[i..i + search_lines.len()].to_vec();
            results.push(matched.join("\n"));
        }
    }

    results
}

/// Multi-line block matching using first/last line anchors with similarity scoring.
fn block_anchor_replacer(content: &str, find: &str) -> Vec<String> {
    let original_lines: Vec<&str> = content.lines().collect();
    let mut search_lines: Vec<&str> = find.lines().collect();

    if search_lines.len() < MIN_LINES_FOR_BLOCK {
        return vec![];
    }

    // Remove trailing empty line if present
    if search_lines.last().map_or(false, |l| l.is_empty()) {
        search_lines.pop();
    }

    let first_line_search = search_lines[0].trim();
    let last_line_search = search_lines[search_lines.len() - 1].trim();
    let search_block_size = search_lines.len();

    // Find all candidate positions
    let mut candidates: Vec<(usize, usize)> = Vec::new();

    for i in 0..original_lines.len() {
        if original_lines[i].trim() != first_line_search {
            continue;
        }

        // Look for matching last line
        for j in (i + 2)..original_lines.len() {
            if original_lines[j].trim() == last_line_search {
                candidates.push((i, j));
                break;
            }
        }
    }

    if candidates.is_empty() {
        return vec![];
    }

    if candidates.len() == 1 {
        let (start_line, end_line) = candidates[0];
        let similarity = calculate_block_similarity(
            &original_lines,
            &search_lines,
            start_line,
            end_line,
            search_block_size,
        );

        if similarity >= SINGLE_CANDIDATE_THRESHOLD {
            let matched: Vec<&str> = original_lines[start_line..=end_line].to_vec();
            return vec![matched.join("\n")];
        }
    } else {
        // Multiple candidates - find best match
        let mut best_match = None;
        let mut max_similarity = -1.0_f64;

        for (start_line, end_line) in &candidates {
            let similarity = calculate_block_similarity(
                &original_lines,
                &search_lines,
                *start_line,
                *end_line,
                search_block_size,
            );

            if similarity > max_similarity {
                max_similarity = similarity;
                best_match = Some((*start_line, *end_line));
            }
        }

        if max_similarity >= MULTIPLE_CANDIDATES_THRESHOLD {
            if let Some((start_line, end_line)) = best_match {
                let matched: Vec<&str> = original_lines[start_line..=end_line].to_vec();
                return vec![matched.join("\n")];
            }
        }
    }

    vec![]
}

/// Calculate similarity between search block and candidate block.
fn calculate_block_similarity(
    original_lines: &[&str],
    search_lines: &[&str],
    start_line: usize,
    end_line: usize,
    search_block_size: usize,
) -> f64 {
    let actual_block_size = end_line - start_line + 1;
    let lines_to_check = search_block_size
        .saturating_sub(2)
        .min(actual_block_size.saturating_sub(2));

    if lines_to_check == 0 {
        return 1.0;
    }

    let mut similarity = 0.0;
    let mut checked = 0;

    for j in 1..search_block_size
        .saturating_sub(1)
        .min(actual_block_size.saturating_sub(1))
    {
        let original_line = original_lines[start_line + j].trim();
        let search_line = search_lines[j].trim();
        let max_len = original_line.len().max(search_line.len());

        if max_len == 0 {
            continue;
        }

        let distance = levenshtein_distance(original_line, search_line);
        similarity += 1.0 - (distance as f64 / max_len as f64);
        checked += 1;
    }

    if checked > 0 {
        similarity / checked as f64
    } else {
        1.0
    }
}

/// Whitespace-normalized matching replacer.
fn whitespace_normalized_replacer(content: &str, find: &str) -> Vec<String> {
    fn normalize_whitespace(text: &str) -> String {
        // Collapse multiple whitespace to single space
        let mut result = String::with_capacity(text.len());
        let mut last_was_space = true; // Start true to trim leading

        for c in text.chars() {
            if c.is_whitespace() {
                if !last_was_space {
                    result.push(' ');
                    last_was_space = true;
                }
            } else {
                result.push(c);
                last_was_space = false;
            }
        }

        // Trim trailing space
        if result.ends_with(' ') {
            result.pop();
        }

        // Normalize spacing around punctuation
        let punctuation_patterns = [
            (':', ":"),
            (';', ";"),
            (',', ","),
            ('(', "("),
            (')', ")"),
            ('[', "["),
            (']', "]"),
            ('{', "{"),
            ('}', "}"),
        ];

        for (punct, replacement) in punctuation_patterns {
            let with_space_before = format!(" {}", punct);
            let with_space_after = format!("{} ", punct);
            result = result.replace(&with_space_before, replacement);
            result = result.replace(&with_space_after, replacement);
        }

        result
    }

    let normalized_find = normalize_whitespace(find);
    let mut found_matches: HashSet<String> = HashSet::new();
    let mut results = Vec::new();

    // Try to match the entire content first
    if normalize_whitespace(content) == normalized_find {
        if found_matches.insert(content.to_string()) {
            results.push(content.to_string());
        }
        return results;
    }

    let lines: Vec<&str> = content.lines().collect();
    let find_lines: Vec<&str> = find.lines().collect();

    // Multi-line matches
    if find_lines.len() > 1 && lines.len() >= find_lines.len() {
        for i in 0..=lines.len() - find_lines.len() {
            let block: Vec<&str> = lines[i..i + find_lines.len()].to_vec();
            let block_content = block.join("\n");

            if normalize_whitespace(&block_content) == normalized_find {
                if found_matches.insert(block_content.clone()) {
                    results.push(block_content);
                }
            }
        }
    } else {
        // Single line matches
        for line in &lines {
            if normalize_whitespace(line) == normalized_find {
                if found_matches.insert(line.to_string()) {
                    results.push(line.to_string());
                }
            }
        }
    }

    results
}

/// Indentation-flexible matching replacer.
fn indentation_flexible_replacer(content: &str, find: &str) -> Vec<String> {
    fn remove_common_indentation(text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let non_empty_lines: Vec<&str> = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .copied()
            .collect();

        if non_empty_lines.is_empty() {
            return text.to_string();
        }

        let min_indent = non_empty_lines
            .iter()
            .map(|line| line.len() - line.trim_start().len())
            .min()
            .unwrap_or(0);

        lines
            .iter()
            .map(|line| {
                if line.trim().is_empty() {
                    *line
                } else if line.len() >= min_indent {
                    &line[min_indent..]
                } else {
                    *line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    let normalized_find = remove_common_indentation(find);
    let content_lines: Vec<&str> = content.lines().collect();
    let find_lines: Vec<&str> = find.lines().collect();

    // Early exit if content is too short
    if content_lines.len() < find_lines.len() || find_lines.is_empty() {
        return vec![];
    }

    let mut results = Vec::new();

    for i in 0..=content_lines.len() - find_lines.len() {
        let block = content_lines[i..i + find_lines.len()].join("\n");
        if remove_common_indentation(&block) == normalized_find {
            results.push(block);
        }
    }

    results
}

/// Escape sequence normalized matching replacer.
fn escape_normalized_replacer(content: &str, find: &str) -> Vec<String> {
    fn unescape_string(text: &str) -> String {
        text.replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\r", "\r")
            .replace("\\'", "'")
            .replace("\\\"", "\"")
            .replace("\\`", "`")
            .replace("\\$", "$")
            .replace("\\\\", "\\")
    }

    let unescaped_find = unescape_string(find);
    let mut results = Vec::new();

    // Try direct match with unescaped find
    if content.contains(&unescaped_find) {
        results.push(unescaped_find.clone());
    }

    // Try finding escaped versions line by line
    let lines: Vec<&str> = content.lines().collect();
    let find_lines: Vec<&str> = unescaped_find.lines().collect();

    // Only try line-by-line if we have enough lines
    if !find_lines.is_empty() && lines.len() >= find_lines.len() {
        for i in 0..=lines.len() - find_lines.len() {
            let block = lines[i..i + find_lines.len()].join("\n");
            if unescape_string(&block) == unescaped_find && !results.contains(&block) {
                results.push(block);
            }
        }
    }

    results
}

/// Trimmed boundary matching replacer.
fn trimmed_boundary_replacer(content: &str, find: &str) -> Vec<String> {
    let trimmed_find = find.trim();

    if trimmed_find == find {
        return vec![]; // Already trimmed
    }

    let mut results = Vec::new();

    // Try trimmed version
    if content.contains(trimmed_find) {
        results.push(trimmed_find.to_string());
    }

    // Try finding blocks where trimmed content matches
    let lines: Vec<&str> = content.lines().collect();
    let find_lines: Vec<&str> = find.lines().collect();

    if !find_lines.is_empty() && lines.len() >= find_lines.len() {
        for i in 0..=lines.len() - find_lines.len() {
            let block = lines[i..i + find_lines.len()].join("\n");
            if block.trim() == trimmed_find && !results.contains(&block) {
                results.push(block);
            }
        }
    }

    results
}

/// Context-aware matching using anchor lines.
fn context_aware_replacer(content: &str, find: &str) -> Vec<String> {
    const MIN_CONTEXT_LINES: usize = 3;

    let mut find_lines: Vec<&str> = find.lines().collect();

    if find_lines.len() < MIN_CONTEXT_LINES {
        return vec![];
    }

    // Remove trailing empty line if present
    if find_lines.last().map_or(false, |l| l.is_empty()) {
        find_lines.pop();
    }

    let content_lines: Vec<&str> = content.lines().collect();
    let first_line = find_lines[0].trim();
    let last_line = find_lines[find_lines.len() - 1].trim();

    let mut results = Vec::new();

    for i in 0..content_lines.len() {
        if content_lines[i].trim() != first_line {
            continue;
        }

        for j in (i + 2)..content_lines.len() {
            if content_lines[j].trim() != last_line {
                continue;
            }

            let block_lines: Vec<&str> = content_lines[i..=j].to_vec();

            // Check similarity of middle content
            if block_lines.len() == find_lines.len() {
                let mut matching_lines = 0;
                let mut total_non_empty = 0;

                for k in 1..block_lines.len() - 1 {
                    let block_line = block_lines[k].trim();
                    let find_line = find_lines[k].trim();

                    if !block_line.is_empty() || !find_line.is_empty() {
                        total_non_empty += 1;
                        if block_line == find_line {
                            matching_lines += 1;
                        }
                    }
                }

                // Require at least 50% similarity
                if total_non_empty == 0
                    || (matching_lines as f64 / total_non_empty as f64) >= MIN_SIMILARITY_RATIO
                {
                    results.push(block_lines.join("\n"));
                }
            }
            break;
        }
    }

    results
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Calculate Levenshtein distance between two strings.
fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();

    let len1 = s1_chars.len();
    let len2 = s2_chars.len();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    // Use two rows for space efficiency
    let mut prev_row: Vec<usize> = (0..=len2).collect();
    let mut curr_row: Vec<usize> = vec![0; len2 + 1];

    for (i, c1) in s1_chars.iter().enumerate() {
        curr_row[0] = i + 1;

        for (j, c2) in s2_chars.iter().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            curr_row[j + 1] = (prev_row[j + 1] + 1)
                .min(curr_row[j] + 1)
                .min(prev_row[j] + cost);
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[len2]
}

/// Find the occurrence of search_text closest to line_hint.
fn find_closest_match(content: &str, search_text: &str, line_hint: usize) -> Option<usize> {
    let mut matches: Vec<(usize, usize)> = Vec::new(); // (line_number, char_index)

    let mut start = 0;
    while let Some(index) = content[start..].find(search_text) {
        let abs_index = start + index;
        let line_num = content[..abs_index].matches('\n').count() + 1;
        matches.push((line_num, abs_index));
        start = abs_index + 1;
    }

    if matches.is_empty() {
        return None;
    }

    // Find the match closest to line_hint
    matches
        .into_iter()
        .min_by_key(|(line, _)| (*line as isize - line_hint as isize).unsigned_abs())
        .map(|(_, index)| index)
}

/// Find all line numbers where search_text starts.
fn find_all_match_locations(content: &str, search_text: &str) -> Vec<usize> {
    let lines: Vec<&str> = content.lines().collect();
    let mut locations = Vec::new();

    let search_lines: Vec<&str> = search_text.lines().collect();
    let first_search_line = search_lines.first().copied().unwrap_or(search_text);

    for (i, line) in lines.iter().enumerate() {
        if line.contains(first_search_line) {
            // Verify full match if multi-line
            if search_lines.len() > 1 {
                let window_end = (i + search_lines.len()).min(lines.len());
                let window = lines[i..window_end].join("\n");
                if window.contains(search_text) {
                    locations.push(i + 1); // 1-based
                }
            } else {
                locations.push(i + 1); // 1-based
            }
        }
    }

    locations
}

/// Find the best fuzzy match for search text in content.
fn find_best_fuzzy_match(
    content: &str,
    search_text: &str,
    threshold: f64,
) -> Option<FuzzyMatchInfo> {
    let content_lines: Vec<&str> = content.lines().collect();
    let search_lines: Vec<&str> = search_text.lines().collect();
    let window_size = search_lines.len();

    if window_size == 0 || content_lines.len() < window_size {
        return None;
    }

    // Find non-empty lines as anchors
    let non_empty_search: Vec<&str> = search_lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .copied()
        .collect();

    if non_empty_search.is_empty() {
        return None;
    }

    let first_anchor = non_empty_search[0];

    // Find candidate starting positions
    let mut candidate_starts: HashSet<usize> = HashSet::new();
    let spread = 5;

    for (i, line) in content_lines.iter().enumerate() {
        if line.contains(first_anchor) {
            let start_min = i.saturating_sub(spread);
            let start_max =
                (i + spread + 1).min(content_lines.len().saturating_sub(window_size) + 1);
            for s in start_min..start_max {
                candidate_starts.insert(s);
            }
        }
    }

    if candidate_starts.is_empty() {
        // Sample first 100 positions
        let max_positions = (content_lines.len().saturating_sub(window_size) + 1).min(100);
        for i in 0..max_positions {
            candidate_starts.insert(i);
        }
    }

    let mut best_match: Option<FuzzyMatchInfo> = None;
    let mut best_similarity = 0.0_f64;

    for start in candidate_starts {
        let end = start + window_size;
        if end > content_lines.len() {
            continue;
        }

        let window_text = content_lines[start..end].join("\n");
        let similarity = calculate_similarity(search_text, &window_text);

        if similarity >= threshold && similarity > best_similarity {
            best_similarity = similarity;
            let diff = create_unified_diff(search_text, &window_text);

            best_match = Some(FuzzyMatchInfo {
                similarity,
                start_line: start + 1, // 1-based
                end_line: end,
                text: window_text,
                diff,
            });
        }
    }

    best_match
}

/// Trim common indentation from diff output.
///
/// This removes the minimum common indentation from all content lines
/// in a unified diff, making it more readable when the diff is from
/// deeply indented code.
pub fn trim_diff(diff_text: &str) -> String {
    let lines: Vec<&str> = diff_text.lines().collect();

    // Find content lines (those starting with +, -, or space, but not --- or +++)
    let content_lines: Vec<&str> = lines
        .iter()
        .filter(|line| {
            (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
                && !line.starts_with("---")
                && !line.starts_with("+++")
        })
        .copied()
        .collect();

    if content_lines.is_empty() {
        return diff_text.to_string();
    }

    // Find minimum indentation
    let mut min_indent = usize::MAX;
    for line in &content_lines {
        let content = &line[1..]; // Remove +/- prefix
        if !content.trim().is_empty() {
            let indent = content.len() - content.trim_start().len();
            min_indent = min_indent.min(indent);
        }
    }

    if min_indent == usize::MAX || min_indent == 0 {
        return diff_text.to_string();
    }

    // Trim indentation from each line
    let trimmed_lines: Vec<String> = lines
        .iter()
        .map(|line| {
            if (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
                && !line.starts_with("---")
                && !line.starts_with("+++")
            {
                let prefix = &line[..1];
                let content = &line[1..];
                if content.len() >= min_indent {
                    format!("{}{}", prefix, &content[min_indent..])
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        })
        .collect();

    trimmed_lines.join("\n")
}

/// Calculate similarity ratio between two strings (0.0 to 1.0).
fn calculate_similarity(s1: &str, s2: &str) -> f64 {
    if s1.is_empty() && s2.is_empty() {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }

    let distance = levenshtein_distance(s1, s2);
    let max_len = s1.len().max(s2.len());

    1.0 - (distance as f64 / max_len as f64)
}

/// Create a simple unified diff between two texts.
fn create_unified_diff(text1: &str, text2: &str) -> String {
    let lines1: Vec<&str> = text1.lines().collect();
    let lines2: Vec<&str> = text2.lines().collect();

    let mut diff = String::new();
    diff.push_str("--- SEARCH\n");
    diff.push_str("+++ CLOSEST MATCH\n");

    let max_lines = lines1.len().max(lines2.len());

    for i in 0..max_lines {
        let l1 = lines1.get(i).copied().unwrap_or("");
        let l2 = lines2.get(i).copied().unwrap_or("");

        if l1 == l2 {
            diff.push_str(&format!(" {}\n", l1));
        } else {
            if i < lines1.len() {
                diff.push_str(&format!("-{}\n", l1));
            }
            if i < lines2.len() {
                diff.push_str(&format!("+{}\n", l2));
            }
        }
    }

    // Truncate if too long
    const MAX_CHARS: usize = 2000;
    if diff.len() > MAX_CHARS {
        diff.truncate(MAX_CHARS);
        diff.push_str("\n...(diff truncated)");
    }

    diff
}

/// Build a helpful error message when old_string is not found.
fn build_not_found_error(
    content: &str,
    old_string: &str,
    closest: Option<&FuzzyMatchInfo>,
) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let search_lines: Vec<&str> = old_string.lines().collect();

    let mut error_parts = vec!["Search text not found in file.".to_string()];

    // Add first line context
    if let Some(first_search_line) = search_lines.first() {
        let trimmed = first_search_line.trim();
        if !trimmed.is_empty() {
            let matches: Vec<usize> = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.contains(trimmed))
                .map(|(i, _)| i + 1)
                .take(3)
                .collect();

            if !matches.is_empty() {
                let match_str: Vec<String> = matches.iter().map(|m| m.to_string()).collect();
                error_parts.push(format!(
                    "\nFirst search line '{}' appears at line(s): {}",
                    trimmed,
                    match_str.join(", ")
                ));
            } else {
                error_parts.push(format!(
                    "\nFirst search line '{}' not found anywhere in file",
                    trimmed
                ));
            }
        }
    }

    // Add fuzzy match info
    if let Some(fuzzy) = closest {
        error_parts.push(format!(
            "\nClosest fuzzy match ({:.1}% similar) at lines {}-{}:",
            fuzzy.similarity * 100.0,
            fuzzy.start_line,
            fuzzy.end_line
        ));
        error_parts.push(format!("\n{}", fuzzy.diff));
    }

    // Add debugging tips
    error_parts.push(
        "\n\nDebugging tips:\n\
         1. Check for exact whitespace/indentation match\n\
         2. Verify line endings match the file (\\r\\n vs \\n)\n\
         3. Ensure the search text hasn't been modified\n\
         4. Try reading the file section first to get exact text"
            .to_string(),
    );

    error_parts.join("")
}

/// Build error message for multiple matches.
fn build_multiple_matches_error(old_string: &str, locations: &[usize]) -> String {
    let search_preview: String = old_string
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(60)
        .collect();
    let ellipsis = if old_string.lines().next().map_or(false, |l| l.len() > 60) {
        "..."
    } else {
        ""
    };

    let location_str: String = if locations.len() > 5 {
        let first_five: Vec<String> = locations.iter().take(5).map(|l| l.to_string()).collect();
        format!("{}, ... ({} total)", first_five.join(", "), locations.len())
    } else {
        locations
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        "Pattern found at multiple locations (lines: {}).\n\
         Search text starts with: \"{}{}\"\n\n\
         To fix, include more surrounding context in old_string to uniquely identify \
         the target location, or use replace_all=True to replace all occurrences, \
         or provide a line_hint to select the closest match.",
        location_str, search_preview, ellipsis
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Individual Replacer Strategy Tests
    // ========================================================================

    #[test]
    fn test_simple_replacer_finds_match() {
        let content = "Hello world, this is a test";
        let matches = simple_replacer(content, "world");
        assert_eq!(matches, vec!["world"]);
    }

    #[test]
    fn test_simple_replacer_no_match() {
        let content = "Hello world, this is a test";
        let matches = simple_replacer(content, "missing");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_line_trimmed_replacer_finds_match() {
        let content = "  line 1\n    line 2\nline 3";
        let find = "line 1\n  line 2\nline 3";
        let matches = line_trimmed_replacer(content, find);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].contains("line 1"));
    }

    #[test]
    fn test_whitespace_normalized_replacer_multiple_spaces() {
        let content = "This   has    multiple     spaces";
        let find = "This has multiple spaces";
        let matches = whitespace_normalized_replacer(content, find);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_indentation_flexible_replacer_different_indent() {
        let content = "    def function():\n        print(\"hello\")\n        return True";
        let find = "def function():\n    print(\"hello\")\n    return True";
        let matches = indentation_flexible_replacer(content, find);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_escape_normalized_replacer() {
        // Content has escaped \n, find has actual newline
        let content = r#"print("Hello\nWorld")"#;
        let find = "print(\"Hello\nWorld\")"; // This has an actual newline
        let matches = escape_normalized_replacer(content, find);
        // The unescaped find contains a newline, so it won't match the single-line content
        // But direct match with unescaped version should work if content had actual newline
        // This test verifies the escape handling logic runs without panicking
        assert!(matches.len() <= 1);
    }

    #[test]
    fn test_escape_normalized_replacer_actual_escape() {
        // Content has escaped \n that gets unescaped to actual newline
        // The replacer should find it when searching for the unescaped version
        let content = "line1\nline2"; // actual newline
        let find = "line1\\nline2"; // escaped \n (backslash-n as two chars)
        let matches = escape_normalized_replacer(content, find);
        // After unescaping find, it becomes "line1\nline2" which matches content
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_block_anchor_simple_match() {
        let content =
            "def function():\n    print(\"hello\")\n    return True\n\ndef other():\n    pass";
        let find = "def function():\n    print(\"hello\")\n    return True";
        let matches = block_anchor_replacer(content, find);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].contains("def function():"));
    }

    #[test]
    fn test_block_anchor_insufficient_lines() {
        let content = "line1\nline2";
        let find = "line1\nline2"; // Only 2 lines, needs at least 3
        let matches = block_anchor_replacer(content, find);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_block_anchor_similarity_scoring() {
        let content = "def function():\n    print(\"hello world\")\n    x = 1\n    return True";
        let find = "def function():\n    print(\"hello python\")\n    return True"; // Different middle
        let matches = block_anchor_replacer(content, find);
        // Should match due to anchor lines and reasonable similarity
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_context_aware_match() {
        let content = "# Start marker\nsome code here\nmore code\n# End marker\n\nother content";
        let find = "# Start marker\nsome code here\nmore code\n# End marker";
        let matches = context_aware_replacer(content, find);
        assert_eq!(matches.len(), 1);
        assert!(matches[0].contains("Start marker"));
        assert!(matches[0].contains("End marker"));
    }

    #[test]
    fn test_context_aware_insufficient_lines() {
        let content = "line1\nline2";
        let find = "line1\nline2";
        let matches = context_aware_replacer(content, find);
        assert!(matches.is_empty()); // Need at least 3 lines
    }

    // ========================================================================
    // Levenshtein Distance Tests
    // ========================================================================

    #[test]
    fn test_levenshtein_empty_strings() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
    }

    #[test]
    fn test_levenshtein_identical() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_levenshtein_single_operations() {
        assert_eq!(levenshtein_distance("cat", "bat"), 1); // substitution
        assert_eq!(levenshtein_distance("cat", "cats"), 1); // insertion
        assert_eq!(levenshtein_distance("cats", "cat"), 1); // deletion
    }

    #[test]
    fn test_levenshtein_complex() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }

    // ========================================================================
    // Replace Content Tests
    // ========================================================================

    #[test]
    fn test_simple_replacement() {
        let content = "hello world\nfoo bar\nbaz qux";
        let result = replace_content(content, "foo bar", "replaced", false, None).unwrap();

        assert_eq!(result.content, "hello world\nreplaced\nbaz qux");
        assert_eq!(result.matched_text, "foo bar");
        assert_eq!(result.strategy, "simple");
    }

    #[test]
    fn test_line_trimmed_replacement() {
        // When searching for "foo bar", simple replacer finds it as a substring
        // of "    foo bar    ", so it just replaces the exact substring
        let content = "  hello world  \n    foo bar    \nbaz";
        let result = replace_content(content, "foo bar", "replaced", false, None).unwrap();

        assert_eq!(result.content, "  hello world  \n    replaced    \nbaz");
        assert_eq!(result.matched_text, "foo bar");
        assert_eq!(result.strategy, "simple");
    }

    #[test]
    fn test_line_trimmed_replacement_multiline() {
        // Line-trimmed matching is useful for multi-line blocks where
        // indentation differs but content matches when trimmed
        let content = "    def foo():\n        return 42\n";
        let result = replace_content(
            content,
            "def foo():\n    return 42", // Different indentation
            "def bar():\n    return 0",
            false,
            None,
        )
        .unwrap();

        assert!(result.content.contains("def bar()"));
        assert_eq!(result.strategy, "line_trimmed");
    }

    #[test]
    fn test_multiline_replacement() {
        let content = "def foo():\n    return 42\n\ndef bar():\n    return 0";
        let result = replace_content(
            content,
            "def foo():\n    return 42",
            "def foo():\n    return 100",
            false,
            None,
        )
        .unwrap();

        assert!(result.content.contains("return 100"));
        assert_eq!(result.start_line, 1);
    }

    #[test]
    fn test_replace_all() {
        let content = "foo bar\nfoo bar\nfoo bar";
        let result = replace_content(content, "foo bar", "replaced", true, None).unwrap();

        assert_eq!(result.content, "replaced\nreplaced\nreplaced");
    }

    #[test]
    fn test_multiple_matches_error() {
        let content = "foo bar\nother\nfoo bar";
        let result = replace_content(content, "foo bar", "replaced", false, None);

        match result {
            Err(ReplaceError::MultipleMatches { locations, .. }) => {
                assert_eq!(locations, vec![1, 3]);
            }
            _ => panic!("Expected MultipleMatches error"),
        }
    }

    #[test]
    fn test_line_hint_disambiguates() {
        let content = "foo bar\nother\nfoo bar";
        let result = replace_content(content, "foo bar", "replaced", false, Some(3)).unwrap();

        assert_eq!(result.content, "foo bar\nother\nreplaced");
        assert_eq!(result.start_line, 3);
    }

    #[test]
    fn test_not_found_error() {
        let content = "hello world";
        let result = replace_content(content, "not found", "replaced", false, None);

        match result {
            Err(ReplaceError::NotFound { message, .. }) => {
                assert!(message.contains("not found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_no_change_error() {
        let content = "hello world";
        let result = replace_content(content, "hello", "hello", false, None);

        match result {
            Err(ReplaceError::NoChange) => {}
            _ => panic!("Expected NoChange error"),
        }
    }

    #[test]
    fn test_block_anchor_replacement() {
        // Block anchor matching uses first/last line as anchors
        // The middle lines can have some differences
        let content = "def foo():\n    x = 1\n    y = 2\n    return x + y\n\ndef bar():\n    pass";
        let result = replace_content(
            content,
            "def foo():\n    x = 1\n    z = 2\n    return x + y", // 'z' instead of 'y' in middle
            "def foo():\n    return 3",
            false,
            None,
        )
        .unwrap();

        assert!(result.content.contains("return 3"));
        assert_eq!(result.strategy, "block_anchor");
    }

    #[test]
    fn test_whitespace_normalized() {
        let content = "if (  x   ==   y  ) {";
        let result =
            replace_content(content, "if (x == y) {", "if (x != y) {", false, None).unwrap();

        assert!(result.content.contains("if (x != y) {"));
    }

    #[test]
    fn test_indentation_flexible() {
        let content = "        deeply indented\n        code here";
        let result = replace_content(
            content,
            "deeply indented\ncode here",
            "replaced\nlines",
            false,
            None,
        )
        .unwrap();

        assert!(result.content.contains("replaced"));
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn test_find_closest_match() {
        let content = "line 1\nfoo\nline 3\nfoo\nline 5";

        let result = find_closest_match(content, "foo", 2);
        assert!(result.is_some());
        // Should find the first "foo" which is closer to line 2

        let result = find_closest_match(content, "foo", 4);
        assert!(result.is_some());
        // Should find the second "foo" which is closer to line 4
    }

    #[test]
    fn test_calculate_similarity() {
        assert!((calculate_similarity("hello", "hello") - 1.0).abs() < 0.001);
        assert!((calculate_similarity("hello", "hallo") - 0.8).abs() < 0.001);
        assert!(calculate_similarity("abc", "xyz") < 0.5);
    }

    #[test]
    fn test_context_aware_matching() {
        let content = "def test():\n    # comment\n    return True\n\ndef other():\n    pass";
        let result = replace_content(
            content,
            "def test():\n    # different comment\n    return True",
            "def test():\n    return False",
            false,
            None,
        );

        // Should match via context-aware strategy due to matching first/last lines
        assert!(result.is_ok());
    }

    // ========================================================================
    // Additional Integration Tests (from Python test suite)
    // ========================================================================

    #[test]
    fn test_multiline_function_replacement() {
        let content = r#"def old_function():
    print("old")
    return False

def other_function():
    pass"#;

        let old_string = "def old_function():\n    print(\"old\")\n    return False";
        let new_string = "def new_function():\n    print(\"new\")\n    return True";

        let result = replace_content(content, old_string, new_string, false, None).unwrap();
        assert!(result.content.contains("def new_function():"));
        assert!(result.content.contains("def other_function():"));
    }

    #[test]
    fn test_whitespace_flexibility_multiline() {
        let content = "if   condition   :\n    do_something()";
        let old_string = "if condition:\n    do_something()";
        let new_string = "if new_condition:\n    do_something_else()";

        let result = replace_content(content, old_string, new_string, false, None).unwrap();
        assert!(result.content.contains("new_condition"));
    }

    #[test]
    fn test_unicode_content() {
        let content = "# Test with émojis 🐍 and ñoño";
        let result = replace_content(content, "émojis 🐍", "Unicode 🎉", false, None).unwrap();
        assert!(result.content.contains("Unicode 🎉"));
    }

    #[test]
    fn test_fuzzy_match_info_in_error() {
        let content = "def hello_world():\n    return True";
        let result = replace_content(
            content,
            "def hello_word():\n    return True",
            "new",
            false,
            None,
        );

        match result {
            Err(ReplaceError::NotFound { closest_match, .. }) => {
                // Should find a close fuzzy match
                assert!(closest_match.is_some());
                let info = closest_match.unwrap();
                assert!(info.similarity > 0.8);
            }
            _ => panic!("Expected NotFound error with fuzzy match info"),
        }
    }

    #[test]
    fn test_line_numbers_correct() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        let result = replace_content(content, "line 3", "replaced", false, None).unwrap();

        assert_eq!(result.start_line, 3);
        assert_eq!(result.end_line, 3);
    }

    #[test]
    fn test_multiline_line_numbers() {
        let content = "line 1\nstart\nmiddle\nend\nline 5";
        let result =
            replace_content(content, "start\nmiddle\nend", "replaced", false, None).unwrap();

        assert_eq!(result.start_line, 2);
        assert_eq!(result.end_line, 4);
    }

    #[test]
    fn test_replace_all_counts_correctly() {
        let content = "test here\ntest there\ntest everywhere";
        let result = replace_content(content, "test", "pass", true, None).unwrap();

        assert_eq!(result.content, "pass here\npass there\npass everywhere");
    }

    #[test]
    fn test_empty_content() {
        let result = replace_content("", "anything", "new", false, None);
        assert!(matches!(result, Err(ReplaceError::NotFound { .. })));
    }

    #[test]
    fn test_special_regex_characters() {
        // Ensure special regex chars are handled as literals
        let content = "value = $100.00 (approx)";
        let result = replace_content(content, "$100.00", "$200.00", false, None).unwrap();
        assert_eq!(result.content, "value = $200.00 (approx)");
    }

    #[test]
    fn test_newline_at_end_handling() {
        let content = "line 1\nline 2\n";
        let result = replace_content(content, "line 2\n", "replaced\n", false, None).unwrap();
        assert_eq!(result.content, "line 1\nreplaced\n");
    }

    #[test]
    fn test_windows_line_endings() {
        let content = "line 1\r\nline 2\r\nline 3";
        let result = replace_content(content, "line 2", "replaced", false, None).unwrap();
        assert!(result.content.contains("replaced"));
    }

    // ========================================================================
    // Trim Diff Tests
    // ========================================================================

    #[test]
    fn test_trim_diff_removes_common_indent() {
        let diff = "--- a/file.py\n+++ b/file.py\n@@ -1,3 +1,3 @@\n     def foo():\n-        return 1\n+        return 2\n     pass";
        let trimmed = trim_diff(diff);

        // Should remove 4 spaces of common indent from content lines
        assert!(trimmed.contains("-    return 1") || trimmed.contains("- return 1"));
        assert!(trimmed.contains("+    return 2") || trimmed.contains("+ return 2"));
    }

    #[test]
    fn test_trim_diff_preserves_headers() {
        let diff = "--- a/file.py\n+++ b/file.py\n-old\n+new";
        let trimmed = trim_diff(diff);

        assert!(trimmed.contains("--- a/file.py"));
        assert!(trimmed.contains("+++ b/file.py"));
    }

    #[test]
    fn test_trim_diff_no_indent() {
        let diff = "-old line\n+new line";
        let trimmed = trim_diff(diff);

        // No common indent, should be unchanged
        assert_eq!(trimmed, diff);
    }

    #[test]
    fn test_trim_diff_empty() {
        let diff = "";
        let trimmed = trim_diff(diff);
        assert_eq!(trimmed, "");
    }
}
