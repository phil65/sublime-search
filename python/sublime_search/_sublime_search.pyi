"""Type stubs for the Rust extension module."""

from __future__ import annotations

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
