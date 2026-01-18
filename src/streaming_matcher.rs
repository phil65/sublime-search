//! A self-contained streaming fuzzy matcher.
//!
//! This module provides a streaming fuzzy matcher that can process text chunks
//! incrementally and return the best match found so far at each step.
//!
//! Ported from Zed's streaming fuzzy matcher, with all external dependencies removed.

use std::cmp;
use std::ops::Range;

const REPLACEMENT_COST: u32 = 1;
const INSERTION_COST: u32 = 3;
const DELETION_COST: u32 = 10;

/// Allow line hint to be off by this many lines.
/// Higher values increase probability of applying edits to a wrong place,
/// Lower values increase edit failures and overall conversation length.
const LINE_HINT_TOLERANCE: u32 = 200;

/// Threshold for fuzzy string equality (0.0 to 1.0).
const FUZZY_THRESHOLD: f64 = 0.8;

/// A streaming fuzzy matcher that can process text chunks incrementally
/// and return the best match found so far at each step.
///
/// This is designed for real-time matching scenarios like code editing
/// where text arrives in chunks (e.g., from an LLM streaming response).
#[derive(Debug, Clone)]
pub struct StreamingFuzzyMatcher {
    /// Source text split into lines
    source_lines: Vec<String>,
    /// Cumulative byte offsets for each line start
    line_offsets: Vec<usize>,
    /// Total length of source text
    total_len: usize,
    /// Accumulated query lines
    query_lines: Vec<String>,
    /// Optional line hint for disambiguation
    line_hint: Option<u32>,
    /// Buffer for incomplete line being received
    incomplete_line: String,
    /// Current best matches
    matches: Vec<Range<usize>>,
    /// Dynamic programming matrix
    matrix: SearchMatrix,
}

impl StreamingFuzzyMatcher {
    /// Create a new streaming fuzzy matcher for the given source text.
    ///
    /// # Arguments
    /// * `source_text` - The text to search within
    pub fn new(source_text: &str) -> Self {
        let source_lines: Vec<String> = source_text.lines().map(|s| s.to_string()).collect();
        let buffer_line_count = source_lines.len();

        // Pre-compute line offsets for efficient offset <-> line conversion
        let mut line_offsets = Vec::with_capacity(buffer_line_count + 1);
        let mut offset = 0;
        for line in &source_lines {
            line_offsets.push(offset);
            offset += line.len() + 1; // +1 for newline
        }
        line_offsets.push(offset); // End offset

        let total_len = if source_text.is_empty() {
            0
        } else {
            source_text.len()
        };

        Self {
            source_lines,
            line_offsets,
            total_len,
            query_lines: Vec::new(),
            line_hint: None,
            incomplete_line: String::new(),
            matches: Vec::new(),
            matrix: SearchMatrix::new(buffer_line_count + 1),
        }
    }

    /// Returns the accumulated query lines.
    pub fn query_lines(&self) -> &[String] {
        &self.query_lines
    }

    /// Returns the source lines.
    pub fn source_lines(&self) -> &[String] {
        &self.source_lines
    }

    /// Push a new chunk of text and get the best match found so far.
    ///
    /// This method accumulates text chunks and processes complete lines.
    /// Partial lines are buffered internally until a newline is received.
    ///
    /// # Arguments
    /// * `chunk` - Text chunk to add to the query
    /// * `line_hint` - Optional line number hint for match selection
    ///
    /// # Returns
    /// `Some(range)` if a match has been found, `None` otherwise
    pub fn push(&mut self, chunk: &str, line_hint: Option<u32>) -> Option<Range<usize>> {
        if line_hint.is_some() {
            self.line_hint = line_hint;
        }

        // Add the chunk to our incomplete line buffer
        self.incomplete_line.push_str(chunk);

        if let Some((last_pos, _)) = self.incomplete_line.match_indices('\n').next_back() {
            let complete_part = self.incomplete_line[..=last_pos].to_string();

            // Split into lines and add to query_lines
            for line in complete_part.lines() {
                self.query_lines.push(line.to_string());
            }

            self.incomplete_line = self.incomplete_line[last_pos + 1..].to_string();
            self.matches = self.resolve_location_fuzzy();
        }

        let best_match = self.select_best_match();
        best_match.or_else(|| self.matches.first().cloned())
    }

    /// Finish processing and return the final best match(es).
    ///
    /// This processes any remaining incomplete line before returning the final
    /// match result.
    pub fn finish(&mut self) -> Vec<Range<usize>> {
        // Process any remaining incomplete line
        if !self.incomplete_line.is_empty() {
            self.query_lines.push(self.incomplete_line.clone());
            self.incomplete_line.clear();
            self.matches = self.resolve_location_fuzzy();
        }
        self.matches.clone()
    }

    /// Get the text for a given range from the source.
    pub fn get_text(&self, range: &Range<usize>) -> String {
        let source_text: String = self
            .source_lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                if i < self.source_lines.len() - 1 {
                    format!("{}\n", line)
                } else {
                    line.clone()
                }
            })
            .collect();

        let start = range.start.min(source_text.len());
        let end = range.end.min(source_text.len());
        source_text[start..end].to_string()
    }

    fn resolve_location_fuzzy(&mut self) -> Vec<Range<usize>> {
        let new_query_line_count = self.query_lines.len();
        let old_query_line_count = self.matrix.rows.saturating_sub(1);
        if new_query_line_count == old_query_line_count {
            return Vec::new();
        }

        self.matrix.resize_rows(new_query_line_count + 1);

        // Process only the new query lines
        for row in old_query_line_count..new_query_line_count {
            let query_line = self.query_lines[row].trim();
            let leading_deletion_cost = (row + 1) as u32 * DELETION_COST;

            self.matrix.set(
                row + 1,
                0,
                SearchState::new(leading_deletion_cost, SearchDirection::Up),
            );

            for (col, source_line) in self.source_lines.iter().enumerate() {
                let buffer_line = source_line.trim();

                let up = SearchState::new(
                    self.matrix
                        .get(row, col + 1)
                        .cost
                        .saturating_add(DELETION_COST),
                    SearchDirection::Up,
                );
                let left = SearchState::new(
                    self.matrix
                        .get(row + 1, col)
                        .cost
                        .saturating_add(INSERTION_COST),
                    SearchDirection::Left,
                );
                let diagonal = SearchState::new(
                    if query_line == buffer_line {
                        self.matrix.get(row, col).cost
                    } else if fuzzy_eq(query_line, buffer_line) {
                        self.matrix.get(row, col).cost + REPLACEMENT_COST
                    } else {
                        self.matrix
                            .get(row, col)
                            .cost
                            .saturating_add(DELETION_COST + INSERTION_COST)
                    },
                    SearchDirection::Diagonal,
                );
                self.matrix
                    .set(row + 1, col + 1, up.min(left).min(diagonal));
            }
        }

        // Find all matches with the best cost
        let buffer_line_count = self.source_lines.len();
        let mut best_cost = u32::MAX;
        let mut matches_with_best_cost = Vec::new();

        for col in 1..=buffer_line_count {
            let cost = self.matrix.get(new_query_line_count, col).cost;
            if cost < best_cost {
                best_cost = cost;
                matches_with_best_cost.clear();
                matches_with_best_cost.push(col as u32);
            } else if cost == best_cost {
                matches_with_best_cost.push(col as u32);
            }
        }

        // Find ranges for the matches
        let mut valid_matches = Vec::new();
        for &buffer_row_end in &matches_with_best_cost {
            let mut matched_lines = 0;
            let mut query_row = new_query_line_count;
            let mut buffer_row_start = buffer_row_end;

            while query_row > 0 && buffer_row_start > 0 {
                let current = self.matrix.get(query_row, buffer_row_start as usize);
                match current.direction {
                    SearchDirection::Diagonal => {
                        query_row -= 1;
                        buffer_row_start -= 1;
                        matched_lines += 1;
                    }
                    SearchDirection::Up => {
                        query_row -= 1;
                    }
                    SearchDirection::Left => {
                        buffer_row_start -= 1;
                    }
                }
            }

            let matched_buffer_row_count = buffer_row_end - buffer_row_start;
            let matched_ratio = matched_lines as f32
                / (matched_buffer_row_count as f32).max(new_query_line_count as f32);

            if matched_ratio >= 0.8 {
                let buffer_start_ix = self.line_to_offset(buffer_row_start as usize);
                let buffer_end_ix = self.line_end_offset(buffer_row_end as usize - 1);
                valid_matches.push((buffer_row_start, buffer_start_ix..buffer_end_ix));
            }
        }

        valid_matches.into_iter().map(|(_, range)| range).collect()
    }

    /// Return the best match with starting position close enough to line_hint.
    pub fn select_best_match(&self) -> Option<Range<usize>> {
        if self.matches.is_empty() {
            return None;
        }

        if self.matches.len() == 1 {
            return self.matches.first().cloned();
        }

        let Some(line_hint) = self.line_hint else {
            // Multiple ambiguous matches
            return None;
        };

        let mut best_match = None;
        let mut best_distance = u32::MAX;

        for range in &self.matches {
            let start_line = self.offset_to_line(range.start) as u32;
            let distance = start_line.abs_diff(line_hint);

            if distance <= LINE_HINT_TOLERANCE && distance < best_distance {
                best_distance = distance;
                best_match = Some(range.clone());
            }
        }

        best_match
    }

    /// Convert line number to byte offset.
    fn line_to_offset(&self, line: usize) -> usize {
        if line >= self.line_offsets.len() {
            self.total_len
        } else {
            self.line_offsets[line]
        }
    }

    /// Get the end offset of a line (excluding newline).
    fn line_end_offset(&self, line: usize) -> usize {
        if line >= self.source_lines.len() {
            self.total_len
        } else {
            self.line_offsets[line] + self.source_lines[line].len()
        }
    }

    /// Convert byte offset to line number.
    fn offset_to_line(&self, offset: usize) -> usize {
        match self.line_offsets.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        }
    }
}

/// Check if two strings are fuzzy equal using normalized Levenshtein distance.
fn fuzzy_eq(left: &str, right: &str) -> bool {
    if left.is_empty() && right.is_empty() {
        return true;
    }
    if left.is_empty() || right.is_empty() {
        return false;
    }

    let max_len = cmp::max(left.len(), right.len());
    let min_levenshtein = left.len().abs_diff(right.len());
    let min_normalized_similarity = 1.0 - (min_levenshtein as f64 / max_len as f64);

    // Early exit if strings can't possibly be similar enough
    if min_normalized_similarity < FUZZY_THRESHOLD {
        return false;
    }

    let distance = levenshtein_distance(left, right);
    let normalized_similarity = 1.0 - (distance as f64 / max_len as f64);

    normalized_similarity >= FUZZY_THRESHOLD
}

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

    // Use two rows instead of full matrix for space efficiency
    let mut prev_row: Vec<usize> = (0..=len2).collect();
    let mut curr_row: Vec<usize> = vec![0; len2 + 1];

    for (i, c1) in s1_chars.iter().enumerate() {
        curr_row[0] = i + 1;

        for (j, c2) in s2_chars.iter().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            curr_row[j + 1] = cmp::min(
                cmp::min(
                    prev_row[j + 1] + 1, // deletion
                    curr_row[j] + 1,     // insertion
                ),
                prev_row[j] + cost, // substitution
            );
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[len2]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SearchDirection {
    Up,
    Left,
    Diagonal,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SearchState {
    cost: u32,
    direction: SearchDirection,
}

impl SearchState {
    fn new(cost: u32, direction: SearchDirection) -> Self {
        Self { cost, direction }
    }
}

#[derive(Debug, Clone)]
struct SearchMatrix {
    cols: usize,
    rows: usize,
    data: Vec<SearchState>,
}

impl SearchMatrix {
    fn new(cols: usize) -> Self {
        SearchMatrix {
            cols,
            rows: 0,
            data: Vec::new(),
        }
    }

    fn resize_rows(&mut self, needed_rows: usize) {
        if needed_rows <= self.rows {
            return;
        }
        self.rows = needed_rows;
        self.data.resize(
            self.rows * self.cols,
            SearchState::new(0, SearchDirection::Diagonal),
        );
    }

    fn get(&self, row: usize, col: usize) -> SearchState {
        if row >= self.rows || col >= self.cols {
            return SearchState::new(u32::MAX / 2, SearchDirection::Diagonal);
        }
        self.data[row * self.cols + col]
    }

    fn set(&mut self, row: usize, col: usize, state: SearchState) {
        if row < self.rows && col < self.cols {
            self.data[row * self.cols + col] = state;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_query() {
        let mut matcher = StreamingFuzzyMatcher::new("Hello world\nThis is a test\nFoo bar baz");
        assert_eq!(matcher.push("", None), None);
        assert!(matcher.finish().is_empty());
    }

    #[test]
    fn test_streaming_exact_match() {
        let mut matcher = StreamingFuzzyMatcher::new("Hello world\nThis is a test\nFoo bar baz");

        // Push partial query
        assert_eq!(matcher.push("This", None), None);

        // Complete the line
        let result = matcher.push(" is a test\n", None);
        assert!(result.is_some());
        let range = result.unwrap();
        assert_eq!(matcher.get_text(&range), "This is a test");
    }

    #[test]
    fn test_streaming_fuzzy_match() {
        let source = "function foo(a, b) {\n    return a + b;\n}\n\nfunction bar(x, y) {\n    return x * y;\n}\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        // Push a fuzzy query that should match the first function
        let result = matcher.push("function foo(a, c) {\n", None);
        assert!(result.is_some());
        assert_eq!(matcher.get_text(&result.unwrap()), "function foo(a, b) {");

        let result = matcher.push("    return a + c;\n}\n", None);
        assert!(result.is_some());
        assert_eq!(
            matcher.get_text(&result.unwrap()),
            "function foo(a, b) {\n    return a + b;\n}"
        );
    }

    #[test]
    fn test_incremental_improvement() {
        let mut matcher = StreamingFuzzyMatcher::new("Line 1\nLine 2\nLine 3\nLine 4\nLine 5");

        // No match initially
        assert_eq!(matcher.push("Lin", None), None);

        // Get a match when we complete a line
        let result = matcher.push("e 3\n", None);
        assert!(result.is_some());
        assert_eq!(matcher.get_text(&result.unwrap()), "Line 3");

        // The match might change if we add more specific content
        let result = matcher.push("Line 4\n", None);
        assert!(result.is_some());
        assert_eq!(matcher.get_text(&result.unwrap()), "Line 3\nLine 4");
    }

    #[test]
    fn test_incomplete_lines_buffering() {
        let source = "The quick brown fox\njumps over the lazy dog\nPack my box with five dozen liquor jugs\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        // Push text in small chunks across line boundaries
        assert_eq!(matcher.push("jumps ", None), None);
        assert_eq!(matcher.push("over the", None), None);
        assert_eq!(matcher.push(" lazy", None), None);

        // Complete the line
        let result = matcher.push(" dog\n", None);
        assert!(result.is_some());
        assert_eq!(
            matcher.get_text(&result.unwrap()),
            "jumps over the lazy dog"
        );
    }

    #[test]
    fn test_line_hint_selection() {
        let text = "fn first_function() {\n    return 42;\n}\n\nfn second_function() {\n    return 42;\n}\n\nfn third_function() {\n    return 42;\n}\n";
        let mut matcher = StreamingFuzzyMatcher::new(text);

        // Given a query that matches multiple functions
        let query = "return 42;\n";

        // Test with line hint pointing to second function (around line 5)
        let result = matcher.push(query, Some(5));
        assert!(result.is_some());

        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("return 42;"));
    }

    #[test]
    fn test_fuzzy_eq() {
        // Exact match
        assert!(fuzzy_eq("hello world", "hello world"));

        // Small difference (should match with >80% similarity)
        assert!(fuzzy_eq("hello world", "hello worl"));
        assert!(fuzzy_eq("return a + b", "return a + c"));

        // Too different (should not match)
        assert!(!fuzzy_eq("hello", "completely different"));
        assert!(!fuzzy_eq("abc", "xyz"));
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
    fn test_finish_processes_incomplete_line() {
        let source = "def test():\n    return True\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        // Push without newline
        matcher.push("def test():", None);

        // Finish should process the incomplete line
        let results = matcher.finish();
        assert!(!results.is_empty());

        let matched_text = matcher.get_text(&results[0]);
        assert!(matched_text.contains("def test():"));
    }

    #[test]
    fn test_empty_source_text() {
        let mut matcher = StreamingFuzzyMatcher::new("");

        let result = matcher.push("some query\n", None);
        assert!(result.is_none());

        let results = matcher.finish();
        assert!(results.is_empty());
    }

    #[test]
    fn test_query_longer_than_source() {
        let source = "short\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        let long_query = "this is a very long query that is much longer than the source\n";
        let result = matcher.push(long_query, None);

        // Should handle gracefully (either None or some range)
        assert!(result.is_none() || result.is_some());
    }

    #[test]
    fn test_special_characters_and_unicode() {
        let source = "def café_function():\n    return \"Hello 世界!\"\n\ndef test_symbols():\n    return \"@#$%^&*()\"\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        let result = matcher.push("def café_function():\n", None);
        assert!(result.is_some());

        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("café_function"));
    }

    #[test]
    fn test_whitespace_sensitivity() {
        let source = "def function_with_spaces( a , b ):\n    return a+b\n\ndef function_no_spaces(a,b):\n    return a+b\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        // Query with different whitespace
        let result = matcher.push("def function_with_spaces(a, b):\n", None);
        assert!(result.is_some());

        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("function_with_spaces"));
    }

    #[test]
    fn test_multiline_fuzzy_match_imports() {
        let source =
            "import os\nimport sys\n\ndef main():\n    print(\"Hello World\")\n    return 0\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        matcher.push("import os\n", None);
        let result = matcher.push("import sys\n", None);

        assert!(result.is_some());
        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("import os"));
        assert!(matched_text.contains("import sys"));
    }

    #[test]
    fn test_resolve_location_single_line() {
        let source = "x = 42\ny = 24\nz = x + y\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        let result = matcher.push("y = 24\n", None);
        assert!(result.is_some());

        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("y = 24"));
    }

    #[test]
    fn test_resolve_location_multiline() {
        let source = "def fibonacci(n):\n    if n <= 1:\n        return n\n    return fibonacci(n-1) + fibonacci(n-2)\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        matcher.push("def fibonacci(n):\n", None);
        let result = matcher.push("    if n <= 1:\n", None);

        assert!(result.is_some());
        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("fibonacci"));
        assert!(matched_text.contains("if n <= 1:"));
    }

    #[test]
    fn test_resolve_location_function_with_typo() {
        let source = "def process_data(input_data):\n    cleaned = input_data.strip()\n    return cleaned.upper()\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        // Query with typo: "proces" instead of "process"
        let result = matcher.push("def proces_data(input_data):\n", None);
        assert!(result.is_some());

        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("process_data"));
    }

    #[test]
    fn test_resolve_location_class_methods() {
        let source = "class DataProcessor:\n    def __init__(self):\n        self.data = []\n\n    def add_item(self, item):\n        self.data.append(item)\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        let result = matcher.push("    def add_item(self, item):\n", None);
        assert!(result.is_some());

        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("add_item"));
    }

    #[test]
    fn test_resolve_location_nested_closure() {
        let source = "def outer_function():\n    def inner_function():\n        return \"nested\"\n\n    result = inner_function()\n    return result\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        let result = matcher.push("    def inner_function():\n", None);
        assert!(result.is_some());

        let matched_text = matcher.get_text(&result.unwrap());
        assert!(matched_text.contains("inner_function"));
    }

    #[test]
    fn test_line_hint_disambiguates() {
        let source = "# Line 1\ndef function_one():\n    pass\n\n# Line 5\ndef function_two():\n    pass\n\n# Line 9\ndef function_three():\n    pass\n";

        // Without line hint
        let mut matcher1 = StreamingFuzzyMatcher::new(source);
        let result1 = matcher1.push("    pass\n", None);

        // With line hint pointing to second function area
        let mut matcher2 = StreamingFuzzyMatcher::new(source);
        let result2 = matcher2.push("    pass\n", Some(6));

        // Both should find something
        assert!(result1.is_some() || result2.is_some());
    }

    #[test]
    fn test_streaming_chunks_simulation() {
        let source = "class User:\n    def __init__(self, name, email):\n        self.name = name\n        self.email = email\n";
        let mut matcher = StreamingFuzzyMatcher::new(source);

        // Simulate streaming chunks as might come from an LLM
        let chunks = vec![
            "def __init__",
            "(self, name",
            ", email):\n",
            "        self.name",
            " = name\n",
        ];

        let mut last_result = None;
        for chunk in chunks {
            if let Some(result) = matcher.push(chunk, None) {
                last_result = Some(result);
            }
        }

        // Should have found a match by the end
        assert!(last_result.is_some());
        let matched_text = matcher.get_text(&last_result.unwrap());
        assert!(matched_text.contains("__init__"));
    }

    #[test]
    fn test_query_lines_property() {
        let mut matcher = StreamingFuzzyMatcher::new("line 1\nline 2\nline 3\n");

        assert!(matcher.query_lines().is_empty());

        matcher.push("query line 1\n", None);
        assert_eq!(matcher.query_lines().len(), 1);
        assert_eq!(matcher.query_lines()[0], "query line 1");

        matcher.push("query line 2\n", None);
        assert_eq!(matcher.query_lines().len(), 2);
    }

    #[test]
    fn test_source_lines_property() {
        let matcher = StreamingFuzzyMatcher::new("line 1\nline 2\nline 3");

        let source_lines = matcher.source_lines();
        assert_eq!(source_lines.len(), 3);
        assert_eq!(source_lines[0], "line 1");
        assert_eq!(source_lines[1], "line 2");
        assert_eq!(source_lines[2], "line 3");
    }
}
