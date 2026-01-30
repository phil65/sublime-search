from __future__ import annotations

from importlib.metadata import version

from ._sublime_search import (
    # Classes
    ApplyDiffResult,
    DiffHunk,
    FuzzyMatchInfo,
    HunkResult,
    MatchRange,
    ReplaceResult,
    StreamingFuzzyMatcher,
    TryReplaceResult,
    # Functions
    apply_diff_hunks,
    apply_diff_hunks_with_hint,
    parse_locationless_diff,
    replace_content,
    trim_diff,
    try_apply_diff_hunks,
    try_replace_content,
)

# Import the Rust-compiled extension
from ._sublime_search import fuzzy_match as _fuzzy_match
from ._sublime_search import fuzzy_match_simple as _fuzzy_match_simple
from ._sublime_search import get_best_matches as _get_best_matches

__version__ = version("sublime-search")


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
    return _fuzzy_match(
        pattern,
        instring,
        adj_bonus,
        sep_bonus,
        camel_bonus,
        lead_penalty,
        max_lead_penalty,
        unmatched_penalty,
    )


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
    return _get_best_matches(search_string, candidates)


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
    return _fuzzy_match_simple(pattern, instring, case_sensitive)


# Make these the public API
__all__ = [
    # Fuzzy matching
    "fuzzy_match",
    "get_best_matches",
    "fuzzy_match_simple",
    # Streaming matcher
    "MatchRange",
    "StreamingFuzzyMatcher",
    # Content replacement
    "replace_content",
    "try_replace_content",
    "trim_diff",
    "ReplaceResult",
    "TryReplaceResult",
    "FuzzyMatchInfo",
    # Diff parsing
    "parse_locationless_diff",
    "apply_diff_hunks",
    "apply_diff_hunks_with_hint",
    "try_apply_diff_hunks",
    "DiffHunk",
    "HunkResult",
    "ApplyDiffResult",
]
