from __future__ import annotations

# Import the Rust-compiled extension
from ._sublime_search import fuzzy_match as _fuzzy_match
from ._sublime_search import get_best_matches as _get_best_matches


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


def get_best_matches(search_string: str, candidates: list[str]) -> list[tuple[str, int]]:
    """Return sorted list of all matches.

    Args:
        search_string: The pattern to search for
        candidates: List of strings to search through

    Returns:
        List of tuples containing (candidate, score) sorted by descending score
    """
    return _get_best_matches(search_string, candidates)


# Make these the public API
__all__ = ["fuzzy_match", "get_best_matches"]
