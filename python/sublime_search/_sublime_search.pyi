"""Type stubs for the Rust extension module."""

from __future__ import annotations

class RetryableError(Exception):
    """Error that indicates the operation should be retried with corrected input.

    This is raised when:
    - Content was not found (agent should re-read the file)
    - Multiple matches exist (agent should add more context)

    Unlike ValueError, this signals that retry is appropriate.
    This exception is designed to be caught and converted to framework-specific
    retry mechanisms (e.g., pydantic-ai's ModelRetry).

    Example:
        >>> from sublime_search import replace_content, RetryableError
        >>> try:
        ...     result = replace_content(content, "not found", "new")
        ... except RetryableError as e:
        ...     # Signal agent to retry
        ...     raise ModelRetry(str(e)) from e
        ... except ValueError as e:
        ...     # Programming error, don't retry
        ...     raise
    """

    ...

class MatchRange:
    """Represents a text range with start and end byte offsets."""

    @property
    def start(self) -> int:
        """Start byte offset."""
        ...

    @property
    def end(self) -> int:
        """End byte offset."""
        ...

    def __repr__(self) -> str: ...
    def __eq__(self, other: MatchRange) -> bool: ...

class StreamingFuzzyMatcher:
    """A streaming fuzzy matcher that processes text chunks incrementally.

    This matcher is designed for real-time matching scenarios like code editing
    where text arrives in chunks (e.g., from an LLM streaming response).

    Example:
        >>> matcher = StreamingFuzzyMatcher("function foo() {\\n    return 42;\\n}")
        >>> result = matcher.push("function foo() {\\n", None)
        >>> print(result)  # MatchRange with the match location
    """

    def __init__(self, source_text: str) -> None:
        """Create a new streaming fuzzy matcher for the given source text.

        Args:
            source_text: The text to search within
        """
        ...

    def push(self, chunk: str, line_hint: int | None = None) -> MatchRange | None:
        """Push a new chunk of text and get the best match found so far.

        This method accumulates text chunks and processes complete lines.
        Partial lines are buffered internally until a newline is received.

        Args:
            chunk: Text chunk to add to the query
            line_hint: Optional line number hint for match selection

        Returns:
            MatchRange if a match has been found, None otherwise
        """
        ...

    def finish(self) -> list[MatchRange]:
        """Finish processing and return all final matches.

        This processes any remaining incomplete line before returning
        the final match results.

        Returns:
            List of all found MatchRange objects
        """
        ...

    def select_best_match(self) -> MatchRange | None:
        """Return the best match considering line hints.

        Returns:
            Best MatchRange, or None if no suitable match found
        """
        ...

    def get_text(self, match_range: MatchRange) -> str:
        """Get the text for a given range from the source.

        Args:
            match_range: The MatchRange to extract text for

        Returns:
            The matched text as a string
        """
        ...

    @property
    def query_lines(self) -> list[str]:
        """Returns the accumulated query lines."""
        ...

    @property
    def source_lines(self) -> list[str]:
        """Returns the source lines."""
        ...

class ReplaceResult:
    """Result of a successful replacement operation."""

    @property
    def content(self) -> str:
        """The new content after replacement."""
        ...

    @property
    def matched_text(self) -> str:
        """The text that was matched and replaced."""
        ...

    @property
    def start_line(self) -> int:
        """Starting line number (1-based)."""
        ...

    @property
    def end_line(self) -> int:
        """Ending line number (1-based)."""
        ...

    @property
    def strategy(self) -> str:
        """Which strategy succeeded."""
        ...

    def __repr__(self) -> str: ...

class FuzzyMatchInfo:
    """Information about a fuzzy match when replacement fails."""

    @property
    def similarity(self) -> float:
        """Similarity score (0.0 to 1.0)."""
        ...

    @property
    def start_line(self) -> int:
        """Start line of the match."""
        ...

    @property
    def end_line(self) -> int:
        """End line of the match."""
        ...

    @property
    def text(self) -> str:
        """The matched text."""
        ...

    @property
    def diff(self) -> str:
        """Unified diff between search and match."""
        ...

    def __repr__(self) -> str: ...

class TryReplaceResult:
    """Result from try_replace_content - success or failure with details."""

    @property
    def success(self) -> bool:
        """Whether replacement succeeded."""
        ...

    @property
    def result(self) -> ReplaceResult | None:
        """ReplaceResult if success, None otherwise."""
        ...

    @property
    def error(self) -> str | None:
        """Error message if failed, None otherwise."""
        ...

    @property
    def error_type(self) -> str | None:
        """Error type: 'not_found', 'multiple_matches', or 'no_change'."""
        ...

    @property
    def closest_match(self) -> FuzzyMatchInfo | None:
        """FuzzyMatchInfo if a close match was found."""
        ...

    @property
    def locations(self) -> list[int] | None:
        """List of line numbers if multiple matches found."""
        ...

    @property
    def retryable(self) -> bool:
        """Whether this error is suitable for retry.

        True for 'not_found' and 'multiple_matches' errors where
        the caller (e.g., an AI agent) should retry with corrected input.
        False for 'no_change' which is a programming error.
        """
        ...

    def __repr__(self) -> str: ...
    def __bool__(self) -> bool: ...

class DiffHunk:
    """A single diff hunk representing one edit operation."""

    @property
    def old_text(self) -> str:
        """The text to find/replace (context + removed lines)."""
        ...

    @property
    def new_text(self) -> str:
        """The replacement text (context + added lines)."""
        ...

    @property
    def raw(self) -> str:
        """The raw diff text for this hunk."""
        ...

    def __repr__(self) -> str: ...

class HunkResult:
    """Result of applying a single hunk."""

    @property
    def success(self) -> bool:
        """Whether the hunk was successfully applied."""
        ...

    @property
    def strategy(self) -> str | None:
        """The strategy that succeeded (if any)."""
        ...

    @property
    def error(self) -> str | None:
        """Error message if failed."""
        ...

    @property
    def start_line(self) -> int | None:
        """Start line of the match (1-based)."""
        ...

    @property
    def end_line(self) -> int | None:
        """End line of the match (1-based)."""
        ...

    def __repr__(self) -> str: ...
    def __bool__(self) -> bool: ...

class ApplyDiffResult:
    """Result of applying all diff hunks."""

    @property
    def content(self) -> str:
        """The final content after applying hunks."""
        ...

    @property
    def applied_count(self) -> int:
        """Number of hunks successfully applied."""
        ...

    @property
    def total_count(self) -> int:
        """Total number of hunks parsed."""
        ...

    @property
    def hunk_results(self) -> list[HunkResult]:
        """Results for each hunk."""
        ...

    @property
    def all_applied(self) -> bool:
        """Whether all hunks were applied successfully."""
        ...

    def __repr__(self) -> str: ...
    def __bool__(self) -> bool: ...

def fuzzy_match(
    pattern: str,
    instring: str,
    adj_bonus: int = 5,
    sep_bonus: int = 10,
    camel_bonus: int = 10,
    lead_penalty: int = -3,
    max_lead_penalty: int = -9,
    unmatched_penalty: int = -1,
) -> tuple[bool, int]:
    """Return match boolean and match score.

    Args:
        pattern: the pattern to be matched
        instring: the containing string to search against
        adj_bonus: bonus for adjacent matches
        sep_bonus: bonus if match occurs after a separator
        camel_bonus: bonus if match is uppercase
        lead_penalty: penalty applied for each letter before 1st match
        max_lead_penalty: maximum total lead_penalty
        unmatched_penalty: penalty for each unmatched letter

    Returns:
        A tuple with match truthiness and score
    """
    ...

def get_best_matches(
    search_string: str, candidates: list[str]
) -> list[tuple[str, int]]:
    """Return sorted list of all matches.

    Args:
        search_string: The pattern to search for
        candidates: List of strings to search through

    Returns:
        List of tuples containing (candidate, score) sorted by descending score
    """
    ...

def fuzzy_match_simple(
    pattern: str, instring: str, case_sensitive: bool = False
) -> bool:
    """Return True if each character in pattern is found in order in instring.

    Args:
        pattern: the pattern to be matched
        instring: the containing string to search against
        case_sensitive: whether to match case-sensitively

    Returns:
        True if there is a match, False otherwise
    """
    ...

def replace_content(
    content: str,
    old_string: str,
    new_string: str,
    replace_all: bool = False,
    line_hint: int | None = None,
) -> ReplaceResult:
    """Replace content using multiple fallback strategies.

    Tries various matching strategies in order until one succeeds:
    1. Simple - Direct string matching
    2. Line-trimmed - Line-by-line matching with trimmed whitespace
    3. Block-anchor - Multi-line matching using first/last line anchors
    4. Whitespace-normalized - Normalized whitespace matching
    5. Indentation-flexible - Matching with flexible indentation
    6. Escape-normalized - Handle escape sequence differences
    7. Trimmed-boundary - Match with trimmed boundaries
    8. Context-aware - Use anchor lines for context matching

    Args:
        content: The text content to search within
        old_string: Text to find and replace
        new_string: Replacement text
        replace_all: If True, replace all occurrences
        line_hint: Optional line number hint for disambiguation

    Returns:
        ReplaceResult with the new content and match information

    Raises:
        RetryableError: If old_string not found or multiple matches exist.
            These are recoverable - caller should retry with corrected input.
        ValueError: If old_string is empty or equals new_string.
            These are programming errors that should not be retried.
    """
    ...

def try_replace_content(
    content: str,
    old_string: str,
    new_string: str,
    replace_all: bool = False,
    line_hint: int | None = None,
) -> TryReplaceResult:
    """Try to replace content, returning a result object instead of raising.

    Args:
        content: The text content to search within
        old_string: Text to find and replace
        new_string: Replacement text
        replace_all: If True, replace all occurrences
        line_hint: Optional line number hint for disambiguation

    Returns:
        TryReplaceResult with success info and error details
    """
    ...

def trim_diff(diff_text: str) -> str:
    """Trim common indentation from diff output.

    Args:
        diff_text: A unified diff string

    Returns:
        The diff with common indentation removed from content lines
    """
    ...

def parse_locationless_diff(diff_text: str) -> list[DiffHunk]:
    """Parse a locationless unified diff into DiffHunk objects.

    Handles diff format without line numbers - the location is inferred
    by matching context in the file.

    Format expected:
    ```
     context line (unchanged)
    -removed line
    +added line
     more context
    ```

    Multiple hunks are separated by:
    - Blank lines (empty line not starting with space)
    - Non-diff content lines

    Also handles:
    - Content wrapped in <diff>...</diff> tags
    - Content wrapped in ```diff...``` code blocks
    - Standard diff headers (---, +++, @@, etc.) which are skipped

    Args:
        diff_text: The diff text (may contain multiple hunks)

    Returns:
        List of DiffHunk objects with old_text/new_text pairs
    """
    ...

def apply_diff_hunks(
    content: str,
    diff_text: str,
    replace_all: bool = False,
) -> ApplyDiffResult:
    """Apply locationless diff edits to content.

    Parses diff format and applies each hunk using content matching
    via multi-strategy fuzzy replacement.

    Args:
        content: The original file content
        diff_text: The diff text containing hunks to apply
        replace_all: Whether to replace all occurrences of each pattern

    Returns:
        ApplyDiffResult with the modified content and application details

    Raises:
        RetryableError: If none of the hunks could be applied.
            Caller should retry with corrected diff context.
        ValueError: If no diff hunks found in input (format error).
    """
    ...

def apply_diff_hunks_with_hint(
    content: str,
    diff_text: str,
    line_hint: int | None = None,
) -> ApplyDiffResult:
    """Apply locationless diff edits with line hint for disambiguation.

    Similar to apply_diff_hunks but accepts an optional line hint
    to help disambiguate when multiple matches exist.

    Args:
        content: The original file content
        diff_text: The diff text containing hunks to apply
        line_hint: Optional line number hint for disambiguation

    Returns:
        ApplyDiffResult with the modified content and application details

    Raises:
        RetryableError: If none of the hunks could be applied.
            Caller should retry with corrected diff context.
        ValueError: If no diff hunks found in input (format error).
    """
    ...

def try_apply_diff_hunks(
    content: str,
    diff_text: str,
    replace_all: bool = False,
    line_hint: int | None = None,
) -> ApplyDiffResult | None:
    """Try to apply diff hunks, returning a result object instead of raising.

    This is useful when you want to handle partial application or errors
    programmatically rather than catching exceptions.

    Args:
        content: The original file content
        diff_text: The diff text containing hunks to apply
        replace_all: Whether to replace all occurrences
        line_hint: Optional line number hint for disambiguation

    Returns:
        ApplyDiffResult with success info, or None if no hunks found.
    """
    ...
