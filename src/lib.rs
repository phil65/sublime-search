use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::cmp::max;
use std::ops::Range;

mod content_replacer;

// Create a custom exception for retryable errors.
// These are errors where the operation failed due to content mismatch,
// and the caller (e.g., an AI agent) should retry with corrected input.
pyo3::create_exception!(
    _sublime_search,
    RetryableError,
    pyo3::exceptions::PyException,
    "Error that indicates the operation should be retried with corrected input.\n\nThis is raised when:\n- Content was not found (agent should re-read the file)\n- Multiple matches exist (agent should add more context)\n\nUnlike ValueError, this signals that retry is appropriate."
);
mod diff_parser;
mod streaming_matcher;
use streaming_matcher::StreamingFuzzyMatcher;

#[pyfunction]
#[pyo3(signature = (pattern, instring, adj_bonus=5, sep_bonus=10, camel_bonus=10, lead_penalty=-3, max_lead_penalty=-9, unmatched_penalty=-1))]
fn fuzzy_match(
    pattern: &str,
    instring: &str,
    adj_bonus: i32,
    sep_bonus: i32,
    camel_bonus: i32,
    lead_penalty: i32,
    max_lead_penalty: i32,
    unmatched_penalty: i32,
) -> (bool, i32) {
    // Handle empty pattern explicitly
    if pattern.is_empty() {
        return (false, 0);
    }

    let mut score = 0;
    let mut p_idx = 0;
    let p_len = pattern.chars().count();

    let mut prev_match = false;
    let mut prev_lower = false;
    // matching first letter gets sep_bonus
    let mut prev_sep = true;

    let mut best_letter = None;
    let mut best_lower = None;
    let mut best_letter_idx = None;
    let mut best_letter_score = 0;
    let mut matched_indices = Vec::new();

    // Convert pattern and instring to chars for proper Unicode handling
    let pattern_chars: Vec<char> = pattern.chars().collect();

    for (s_idx, s_char) in instring.chars().enumerate() {
        let p_char = if p_idx != p_len {
            Some(pattern_chars[p_idx])
        } else {
            None
        };

        // Improve Unicode handling with better lowercase/uppercase conversion
        let p_lower = p_char.map(|c| c.to_lowercase().next().unwrap_or(c));

        let s_lower = s_char.to_lowercase().next().unwrap_or(s_char);

        let s_upper = s_char.to_uppercase().next().unwrap_or(s_char);

        let next_match = p_char.is_some() && p_lower == Some(s_lower);
        let rematch = best_letter.is_some() && best_lower == Some(s_lower);

        let advanced = next_match && best_letter.is_some();
        let p_repeat = best_letter.is_some() && p_char.is_some() && best_lower == p_lower;

        if advanced || p_repeat {
            score += best_letter_score;
            matched_indices.push(best_letter_idx);
            best_letter = None;
            best_lower = None;
            best_letter_idx = None;
            best_letter_score = 0;
        }

        if next_match || rematch {
            let mut new_score = 0;

            // apply penalty for each letter before the first match
            if p_idx == 0 {
                score += max(s_idx as i32 * lead_penalty, max_lead_penalty);
            }

            // apply bonus for consecutive matches
            if prev_match {
                new_score += adj_bonus;
            }

            // apply bonus for matches after a separator
            if prev_sep {
                new_score += sep_bonus;
            }

            // apply bonus across camelCase boundaries
            if prev_lower && s_char == s_upper && s_lower != s_upper {
                new_score += camel_bonus;
            }

            // update pattern index iff the next pattern letter was matched
            if next_match {
                p_idx += 1;
            }

            // update best letter match (may be next or rematch)
            if new_score >= best_letter_score {
                // apply penalty for now-skipped letter
                if best_letter.is_some() {
                    score += unmatched_penalty;
                }
                best_letter = Some(s_char);
                best_lower = Some(s_lower);
                best_letter_idx = Some(s_idx);
                best_letter_score = new_score;
            }

            prev_match = true;
        } else {
            score += unmatched_penalty;
            prev_match = false;
        }

        prev_lower = s_char == s_lower && s_lower != s_upper;
        prev_sep = s_char == '_' || s_char == ' ';
    }

    if best_letter.is_some() {
        score += best_letter_score;
        matched_indices.push(best_letter_idx);
    }

    (p_idx == p_len, score)
}

#[pyfunction]
fn get_best_matches(search_string: &str, candidates: Vec<String>) -> PyResult<Vec<(String, i32)>> {
    // Special case for empty search string
    if search_string.is_empty() {
        return Ok(Vec::new()); // Return empty results for empty search string
    }

    let mut results = Vec::new();

    for candidate in candidates {
        let (matched, score) = fuzzy_match(search_string, &candidate, 5, 10, 10, -3, -9, -1);
        if matched {
            results.push((candidate, score));
        }
    }

    // Sort by score in descending order
    results.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(results)
}

#[pyfunction]
fn fuzzy_match_simple(pattern: &str, instring: &str, case_sensitive: bool) -> bool {
    // Handle empty pattern explicitly (matches everything)
    if pattern.is_empty() {
        return true;
    }

    // Handle empty instring (can't match anything)
    if instring.is_empty() {
        return false;
    }

    let (pattern, instring) = if case_sensitive {
        (pattern.to_string(), instring.to_string())
    } else {
        (pattern.to_lowercase(), instring.to_lowercase())
    };

    let mut p_idx = 0;
    let mut s_idx = 0;
    let p_chars: Vec<char> = pattern.chars().collect();
    let s_chars: Vec<char> = instring.chars().collect();
    let p_len = p_chars.len();
    let s_len = s_chars.len();

    while p_idx < p_len && s_idx < s_len {
        if p_chars[p_idx] == s_chars[s_idx] {
            p_idx += 1;
        }
        s_idx += 1;
    }

    p_idx == p_len
}

/// Python wrapper for a match range.
#[pyclass]
#[derive(Clone)]
struct MatchRange {
    #[pyo3(get)]
    start: usize,
    #[pyo3(get)]
    end: usize,
}

#[pymethods]
impl MatchRange {
    fn __repr__(&self) -> String {
        format!("MatchRange(start={}, end={})", self.start, self.end)
    }

    fn __eq__(&self, other: &MatchRange) -> bool {
        self.start == other.start && self.end == other.end
    }
}

impl From<Range<usize>> for MatchRange {
    fn from(range: Range<usize>) -> Self {
        MatchRange {
            start: range.start,
            end: range.end,
        }
    }
}

/// A streaming fuzzy matcher that processes text chunks incrementally.
///
/// This matcher is designed for real-time matching scenarios like code editing
/// where text arrives in chunks (e.g., from an LLM streaming response).
///
/// Example:
///     >>> matcher = StreamingFuzzyMatcher("function foo() {\n    return 42;\n}")
///     >>> result = matcher.push("function foo() {\n", None)
///     >>> print(result)  # MatchRange with the match location
#[pyclass(name = "StreamingFuzzyMatcher")]
struct PyStreamingFuzzyMatcher {
    inner: StreamingFuzzyMatcher,
}

#[pymethods]
impl PyStreamingFuzzyMatcher {
    /// Create a new streaming fuzzy matcher for the given source text.
    ///
    /// Args:
    ///     source_text: The text to search within
    #[new]
    fn new(source_text: &str) -> Self {
        PyStreamingFuzzyMatcher {
            inner: StreamingFuzzyMatcher::new(source_text),
        }
    }

    /// Push a new chunk of text and get the best match found so far.
    ///
    /// This method accumulates text chunks and processes complete lines.
    /// Partial lines are buffered internally until a newline is received.
    ///
    /// Args:
    ///     chunk: Text chunk to add to the query
    ///     line_hint: Optional line number hint for match selection
    ///
    /// Returns:
    ///     MatchRange if a match has been found, None otherwise
    #[pyo3(signature = (chunk, line_hint=None))]
    fn push(&mut self, chunk: &str, line_hint: Option<u32>) -> Option<MatchRange> {
        self.inner.push(chunk, line_hint).map(MatchRange::from)
    }

    /// Finish processing and return all final matches.
    ///
    /// This processes any remaining incomplete line before returning
    /// the final match results.
    ///
    /// Returns:
    ///     List of all found MatchRange objects
    fn finish(&mut self) -> Vec<MatchRange> {
        self.inner
            .finish()
            .into_iter()
            .map(MatchRange::from)
            .collect()
    }

    /// Return the best match considering line hints.
    ///
    /// Returns:
    ///     Best MatchRange, or None if no suitable match found
    fn select_best_match(&self) -> Option<MatchRange> {
        self.inner.select_best_match().map(MatchRange::from)
    }

    /// Get the text for a given range from the source.
    ///
    /// Args:
    ///     match_range: The MatchRange to extract text for
    ///
    /// Returns:
    ///     The matched text as a string
    fn get_text(&self, match_range: &MatchRange) -> String {
        self.inner.get_text(&(match_range.start..match_range.end))
    }

    /// Returns the accumulated query lines.
    #[getter]
    fn query_lines(&self) -> Vec<String> {
        self.inner.query_lines().to_vec()
    }

    /// Returns the source lines.
    #[getter]
    fn source_lines(&self) -> Vec<String> {
        self.inner.source_lines().to_vec()
    }
}

// ============================================================================
// Content Replacer Bindings
// ============================================================================

/// Result of a successful replacement operation.
#[pyclass]
#[derive(Clone)]
struct ReplaceResult {
    #[pyo3(get)]
    content: String,
    #[pyo3(get)]
    matched_text: String,
    #[pyo3(get)]
    start_line: usize,
    #[pyo3(get)]
    end_line: usize,
    #[pyo3(get)]
    strategy: String,
}

#[pymethods]
impl ReplaceResult {
    fn __repr__(&self) -> String {
        format!(
            "ReplaceResult(start_line={}, end_line={}, strategy='{}', matched_text='{}')",
            self.start_line,
            self.end_line,
            self.strategy,
            if self.matched_text.len() > 50 {
                format!("{}...", &self.matched_text[..50])
            } else {
                self.matched_text.clone()
            }
        )
    }
}

impl From<content_replacer::ReplaceResult> for ReplaceResult {
    fn from(r: content_replacer::ReplaceResult) -> Self {
        ReplaceResult {
            content: r.content,
            matched_text: r.matched_text,
            start_line: r.start_line,
            end_line: r.end_line,
            strategy: r.strategy,
        }
    }
}

/// Information about a fuzzy match when replacement fails.
#[pyclass]
#[derive(Clone)]
struct FuzzyMatchInfo {
    #[pyo3(get)]
    similarity: f64,
    #[pyo3(get)]
    start_line: usize,
    #[pyo3(get)]
    end_line: usize,
    #[pyo3(get)]
    text: String,
    #[pyo3(get)]
    diff: String,
}

#[pymethods]
impl FuzzyMatchInfo {
    fn __repr__(&self) -> String {
        format!(
            "FuzzyMatchInfo(similarity={:.2}, lines={}-{})",
            self.similarity, self.start_line, self.end_line
        )
    }
}

impl From<content_replacer::FuzzyMatchInfo> for FuzzyMatchInfo {
    fn from(f: content_replacer::FuzzyMatchInfo) -> Self {
        FuzzyMatchInfo {
            similarity: f.similarity,
            start_line: f.start_line,
            end_line: f.end_line,
            text: f.text,
            diff: f.diff,
        }
    }
}

/// Replace content using multiple fallback strategies.
///
/// Tries various matching strategies in order until one succeeds:
/// 1. Simple - Direct string matching
/// 2. Line-trimmed - Line-by-line matching with trimmed whitespace
/// 3. Block-anchor - Multi-line matching using first/last line anchors
/// 4. Whitespace-normalized - Normalized whitespace matching
/// 5. Indentation-flexible - Matching with flexible indentation
/// 6. Escape-normalized - Handle escape sequence differences
/// 7. Trimmed-boundary - Match with trimmed boundaries
/// 8. Context-aware - Use anchor lines for context matching
///
/// Args:
///     content: The text content to search within
///     old_string: Text to find and replace
///     new_string: Replacement text
///     replace_all: If True, replace all occurrences (default: False)
///     line_hint: Optional line number hint for disambiguation when multiple matches exist
///
/// Returns:
///     ReplaceResult with the new content and match information
///
/// Raises:
///     ValueError: If old_string equals new_string, not found, or multiple matches without line_hint
#[pyfunction]
#[pyo3(signature = (content, old_string, new_string, replace_all=false, line_hint=None))]
fn replace_content(
    content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
    line_hint: Option<u32>,
) -> PyResult<ReplaceResult> {
    match content_replacer::replace_content(content, old_string, new_string, replace_all, line_hint)
    {
        Ok(result) => Ok(result.into()),
        // NoChange is a programming error - use ValueError
        Err(content_replacer::ReplaceError::NoChange) => Err(PyValueError::new_err(
            "old_string and new_string must be different",
        )),
        // NotFound is retryable - agent should re-read the file
        Err(content_replacer::ReplaceError::NotFound { message, .. }) => {
            Err(RetryableError::new_err(message))
        }
        // MultipleMatches is retryable - agent should add more context
        Err(content_replacer::ReplaceError::MultipleMatches { message, .. }) => {
            Err(RetryableError::new_err(message))
        }
    }
}

/// Trim common indentation from diff output.
///
/// This removes the minimum common indentation from all content lines
/// in a unified diff, making it more readable when the diff is from
/// deeply indented code.
///
/// Args:
///     diff_text: A unified diff string
///
/// Returns:
///     The diff with common indentation removed from content lines
#[pyfunction]
fn trim_diff(diff_text: &str) -> String {
    content_replacer::trim_diff(diff_text)
}

/// Result from try_replace_content - either success with result, or failure with error details.
#[pyclass]
#[derive(Clone)]
struct TryReplaceResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    result: Option<ReplaceResult>,
    #[pyo3(get)]
    error: Option<String>,
    #[pyo3(get)]
    error_type: Option<String>,
    #[pyo3(get)]
    closest_match: Option<FuzzyMatchInfo>,
    #[pyo3(get)]
    locations: Option<Vec<usize>>,
    #[pyo3(get)]
    retryable: bool,
}

#[pymethods]
impl TryReplaceResult {
    fn __repr__(&self) -> String {
        if self.success {
            format!(
                "TryReplaceResult(success=True, result={:?})",
                self.result.as_ref().map(|r| &r.strategy)
            )
        } else {
            format!(
                "TryReplaceResult(success=False, error_type={:?})",
                self.error_type
            )
        }
    }

    fn __bool__(&self) -> bool {
        self.success
    }
}

/// Try to replace content, returning a result object instead of raising.
///
/// This is useful when you want to handle errors programmatically rather than
/// catching exceptions. The returned object is falsy on failure, truthy on success.
///
/// Args:
///     content: The text content to search within
///     old_string: Text to find and replace
///     new_string: Replacement text
///     replace_all: If True, replace all occurrences (default: False)
///     line_hint: Optional line number hint for disambiguation
///
/// Returns:
///     TryReplaceResult with:
///     - success: bool
///     - result: ReplaceResult if success, None otherwise
///     - error: Error message if failed, None otherwise
///     - error_type: "not_found", "multiple_matches", or "no_change" if failed
///     - closest_match: FuzzyMatchInfo if a close match was found
///     - locations: List of line numbers if multiple matches found
#[pyfunction]
#[pyo3(signature = (content, old_string, new_string, replace_all=false, line_hint=None))]
fn try_replace_content(
    content: &str,
    old_string: &str,
    new_string: &str,
    replace_all: bool,
    line_hint: Option<u32>,
) -> TryReplaceResult {
    match content_replacer::replace_content(content, old_string, new_string, replace_all, line_hint)
    {
        Ok(result) => TryReplaceResult {
            success: true,
            result: Some(ReplaceResult::from(result)),
            error: None,
            error_type: None,
            closest_match: None,
            locations: None,
            retryable: false,
        },
        Err(content_replacer::ReplaceError::NoChange) => TryReplaceResult {
            success: false,
            result: None,
            error: Some("old_string and new_string must be different".to_string()),
            error_type: Some("no_change".to_string()),
            closest_match: None,
            locations: None,
            retryable: false, // Programming error, not retryable
        },
        Err(content_replacer::ReplaceError::NotFound {
            message,
            closest_match,
        }) => TryReplaceResult {
            success: false,
            result: None,
            error: Some(message),
            error_type: Some("not_found".to_string()),
            closest_match: closest_match.map(FuzzyMatchInfo::from),
            locations: None,
            retryable: true, // Agent should re-read file
        },
        Err(content_replacer::ReplaceError::MultipleMatches { message, locations }) => {
            TryReplaceResult {
                success: false,
                result: None,
                error: Some(message),
                error_type: Some("multiple_matches".to_string()),
                closest_match: None,
                locations: Some(locations),
                retryable: true, // Agent should add more context
            }
        }
    }
}

// ============================================================================
// Diff Parser Bindings
// ============================================================================

/// A single diff hunk representing one edit operation.
#[pyclass(name = "DiffHunk")]
#[derive(Clone)]
struct PyDiffHunk {
    #[pyo3(get)]
    old_text: String,
    #[pyo3(get)]
    new_text: String,
    #[pyo3(get)]
    raw: String,
}

#[pymethods]
impl PyDiffHunk {
    fn __repr__(&self) -> String {
        let old_preview: String = self.old_text.chars().take(30).collect();
        let new_preview: String = self.new_text.chars().take(30).collect();
        format!(
            "DiffHunk(old='{}{}', new='{}{}')",
            old_preview,
            if self.old_text.len() > 30 { "..." } else { "" },
            new_preview,
            if self.new_text.len() > 30 { "..." } else { "" }
        )
    }
}

impl From<diff_parser::DiffHunk> for PyDiffHunk {
    fn from(h: diff_parser::DiffHunk) -> Self {
        PyDiffHunk {
            old_text: h.old_text,
            new_text: h.new_text,
            raw: h.raw,
        }
    }
}

/// Result of applying a single hunk.
#[pyclass(name = "HunkResult")]
#[derive(Clone)]
struct PyHunkResult {
    #[pyo3(get)]
    success: bool,
    #[pyo3(get)]
    strategy: Option<String>,
    #[pyo3(get)]
    error: Option<String>,
    #[pyo3(get)]
    start_line: Option<usize>,
    #[pyo3(get)]
    end_line: Option<usize>,
}

#[pymethods]
impl PyHunkResult {
    fn __repr__(&self) -> String {
        if self.success {
            format!(
                "HunkResult(success=True, strategy={:?}, lines={:?}-{:?})",
                self.strategy, self.start_line, self.end_line
            )
        } else {
            format!("HunkResult(success=False, error={:?})", self.error)
        }
    }

    fn __bool__(&self) -> bool {
        self.success
    }
}

impl From<diff_parser::HunkResult> for PyHunkResult {
    fn from(r: diff_parser::HunkResult) -> Self {
        PyHunkResult {
            success: r.success,
            strategy: r.strategy,
            error: r.error,
            start_line: r.start_line,
            end_line: r.end_line,
        }
    }
}

/// Result of applying all diff hunks.
#[pyclass(name = "ApplyDiffResult")]
#[derive(Clone)]
struct PyApplyDiffResult {
    #[pyo3(get)]
    content: String,
    #[pyo3(get)]
    applied_count: usize,
    #[pyo3(get)]
    total_count: usize,
    #[pyo3(get)]
    hunk_results: Vec<PyHunkResult>,
    #[pyo3(get)]
    all_applied: bool,
}

#[pymethods]
impl PyApplyDiffResult {
    fn __repr__(&self) -> String {
        format!(
            "ApplyDiffResult(applied={}/{}, all_applied={})",
            self.applied_count, self.total_count, self.all_applied
        )
    }

    fn __bool__(&self) -> bool {
        self.all_applied
    }
}

impl From<diff_parser::ApplyDiffResult> for PyApplyDiffResult {
    fn from(r: diff_parser::ApplyDiffResult) -> Self {
        PyApplyDiffResult {
            content: r.content,
            applied_count: r.applied_count,
            total_count: r.total_count,
            hunk_results: r.hunk_results.into_iter().map(PyHunkResult::from).collect(),
            all_applied: r.all_applied,
        }
    }
}

/// Parse a locationless unified diff into DiffHunk objects.
///
/// Handles diff format without line numbers - the location is inferred
/// by matching context in the file.
///
/// Format expected:
/// ```
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
/// Also handles:
/// - Content wrapped in <diff>...</diff> tags
/// - Content wrapped in ```diff...``` code blocks
/// - Standard diff headers (---, +++, @@, etc.) which are skipped
///
/// Args:
///     diff_text: The diff text (may contain multiple hunks)
///
/// Returns:
///     List of DiffHunk objects with old_text/new_text pairs
#[pyfunction]
fn parse_locationless_diff(diff_text: &str) -> Vec<PyDiffHunk> {
    diff_parser::parse_locationless_diff(diff_text)
        .into_iter()
        .map(PyDiffHunk::from)
        .collect()
}

/// Apply locationless diff edits to content.
///
/// Parses diff format and applies each hunk using content matching
/// via multi-strategy fuzzy replacement.
///
/// Args:
///     content: The original file content
///     diff_text: The diff text containing hunks to apply
///     replace_all: Whether to replace all occurrences of each pattern (default: False)
///
/// Returns:
///     ApplyDiffResult with the modified content and application details
///
/// Raises:
///     ValueError: If no diff hunks found or none could be applied
#[pyfunction]
#[pyo3(signature = (content, diff_text, replace_all=false))]
fn apply_diff_hunks(
    content: &str,
    diff_text: &str,
    replace_all: bool,
) -> PyResult<PyApplyDiffResult> {
    match diff_parser::apply_diff_hunks(content, diff_text, replace_all) {
        Ok(result) => Ok(PyApplyDiffResult::from(result)),
        // NoHunksFound is a format error - use ValueError
        Err(diff_parser::ApplyDiffError::NoHunksFound) => {
            Err(PyValueError::new_err("No diff hunks found in input"))
        }
        // NoHunksApplied is retryable - agent should re-read and provide accurate diff
        Err(diff_parser::ApplyDiffError::NoHunksApplied { message }) => {
            Err(RetryableError::new_err(message))
        }
    }
}

/// Apply locationless diff edits with line hint for disambiguation.
///
/// Similar to apply_diff_hunks but accepts an optional line hint
/// to help disambiguate when multiple matches exist.
///
/// Args:
///     content: The original file content
///     diff_text: The diff text containing hunks to apply
///     line_hint: Optional line number hint for disambiguation
///
/// Returns:
///     ApplyDiffResult with the modified content and application details
///
/// Raises:
///     ValueError: If no diff hunks found or none could be applied
#[pyfunction]
#[pyo3(signature = (content, diff_text, line_hint=None))]
fn apply_diff_hunks_with_hint(
    content: &str,
    diff_text: &str,
    line_hint: Option<u32>,
) -> PyResult<PyApplyDiffResult> {
    match diff_parser::apply_diff_hunks_with_hint(content, diff_text, line_hint) {
        Ok(result) => Ok(PyApplyDiffResult::from(result)),
        // NoHunksFound is a format error - use ValueError
        Err(diff_parser::ApplyDiffError::NoHunksFound) => {
            Err(PyValueError::new_err("No diff hunks found in input"))
        }
        // NoHunksApplied is retryable - agent should re-read and provide accurate diff
        Err(diff_parser::ApplyDiffError::NoHunksApplied { message }) => {
            Err(RetryableError::new_err(message))
        }
    }
}

/// Try to apply diff hunks, returning a result object instead of raising.
///
/// This is useful when you want to handle partial application or errors
/// programmatically rather than catching exceptions.
///
/// Args:
///     content: The original file content
///     diff_text: The diff text containing hunks to apply
///     replace_all: Whether to replace all occurrences (default: False)
///     line_hint: Optional line number hint for disambiguation
///
/// Returns:
///     ApplyDiffResult with success info, or None if no hunks found.
#[pyfunction]
#[pyo3(signature = (content, diff_text, replace_all=false, line_hint=None))]
fn try_apply_diff_hunks(
    content: &str,
    diff_text: &str,
    replace_all: bool,
    line_hint: Option<u32>,
) -> Option<PyApplyDiffResult> {
    let result = if line_hint.is_some() {
        diff_parser::apply_diff_hunks_with_hint(content, diff_text, line_hint)
    } else {
        diff_parser::apply_diff_hunks(content, diff_text, replace_all)
    };

    match result {
        Ok(r) => Some(PyApplyDiffResult::from(r)),
        Err(diff_parser::ApplyDiffError::NoHunksFound) => None,
        Err(diff_parser::ApplyDiffError::NoHunksApplied { .. }) => {
            // Return a result with 0 applied for inspection
            Some(PyApplyDiffResult {
                content: content.to_string(),
                applied_count: 0,
                total_count: diff_parser::parse_locationless_diff(diff_text).len(),
                hunk_results: Vec::new(),
                all_applied: false,
            })
        }
    }
}

#[pymodule]
fn _sublime_search(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Exceptions
    m.add("RetryableError", m.py().get_type::<RetryableError>())?;

    // Fuzzy matching functions
    m.add_function(wrap_pyfunction!(fuzzy_match, m)?)?;
    m.add_function(wrap_pyfunction!(get_best_matches, m)?)?;
    m.add_function(wrap_pyfunction!(fuzzy_match_simple, m)?)?;

    // Content replacement functions
    m.add_function(wrap_pyfunction!(replace_content, m)?)?;
    m.add_function(wrap_pyfunction!(try_replace_content, m)?)?;
    m.add_function(wrap_pyfunction!(trim_diff, m)?)?;

    // Diff parsing functions
    m.add_function(wrap_pyfunction!(parse_locationless_diff, m)?)?;
    m.add_function(wrap_pyfunction!(apply_diff_hunks, m)?)?;
    m.add_function(wrap_pyfunction!(apply_diff_hunks_with_hint, m)?)?;
    m.add_function(wrap_pyfunction!(try_apply_diff_hunks, m)?)?;

    // Classes
    m.add_class::<PyStreamingFuzzyMatcher>()?;
    m.add_class::<MatchRange>()?;
    m.add_class::<ReplaceResult>()?;
    m.add_class::<TryReplaceResult>()?;
    m.add_class::<FuzzyMatchInfo>()?;
    m.add_class::<PyDiffHunk>()?;
    m.add_class::<PyHunkResult>()?;
    m.add_class::<PyApplyDiffResult>()?;

    Ok(())
}
