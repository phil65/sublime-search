use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::cmp::max;
use std::ops::Range;

mod content_replacer;
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
        Err(content_replacer::ReplaceError::NoChange) => Err(PyValueError::new_err(
            "old_string and new_string must be different",
        )),
        Err(content_replacer::ReplaceError::NotFound { message, .. }) => {
            Err(PyValueError::new_err(message))
        }
        Err(content_replacer::ReplaceError::MultipleMatches { message, .. }) => {
            Err(PyValueError::new_err(message))
        }
    }
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
        },
        Err(content_replacer::ReplaceError::NoChange) => TryReplaceResult {
            success: false,
            result: None,
            error: Some("old_string and new_string must be different".to_string()),
            error_type: Some("no_change".to_string()),
            closest_match: None,
            locations: None,
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
        },
        Err(content_replacer::ReplaceError::MultipleMatches { message, locations }) => {
            TryReplaceResult {
                success: false,
                result: None,
                error: Some(message),
                error_type: Some("multiple_matches".to_string()),
                closest_match: None,
                locations: Some(locations),
            }
        }
    }
}

#[pymodule]
fn _sublime_search(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Fuzzy matching functions
    m.add_function(wrap_pyfunction!(fuzzy_match, m)?)?;
    m.add_function(wrap_pyfunction!(get_best_matches, m)?)?;
    m.add_function(wrap_pyfunction!(fuzzy_match_simple, m)?)?;

    // Content replacement functions
    m.add_function(wrap_pyfunction!(replace_content, m)?)?;
    m.add_function(wrap_pyfunction!(try_replace_content, m)?)?;

    // Classes
    m.add_class::<PyStreamingFuzzyMatcher>()?;
    m.add_class::<MatchRange>()?;
    m.add_class::<ReplaceResult>()?;
    m.add_class::<TryReplaceResult>()?;
    m.add_class::<FuzzyMatchInfo>()?;

    Ok(())
}
