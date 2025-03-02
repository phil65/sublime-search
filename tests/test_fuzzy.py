import pytest
import sublime_search


def test_fuzzy_match_basic():
    """Test basic matching functionality."""
    # Should match
    is_match, score = sublime_search.fuzzy_match("abc", "abcdef")
    assert is_match is True
    assert score > 0

    # Should not match
    is_match, score = sublime_search.fuzzy_match("xyz", "abcdef")
    assert is_match is False


def test_fuzzy_match_case_sensitivity():
    """Verify that matching is case insensitive by default."""
    is_match1, score1 = sublime_search.fuzzy_match("abc", "ABCDEF")
    is_match2, score2 = sublime_search.fuzzy_match("ABC", "abcdef")

    assert is_match1 is True
    assert is_match2 is True


def test_scoring_logic():
    """Test that scoring follows expected patterns."""
    # Consecutive matches should score higher
    _, score1 = sublime_search.fuzzy_match("abc", "abcdef")
    _, score2 = sublime_search.fuzzy_match("abc", "axbycz")
    assert score1 > score2

    # Early matches should score higher than later matches
    _, score1 = sublime_search.fuzzy_match("abc", "abcxxx")
    _, score2 = sublime_search.fuzzy_match("abc", "xxxabc")
    assert score1 > score2

    # Matches after separators should score higher
    _, score1 = sublime_search.fuzzy_match("abc", "a_b_c")
    _, score2 = sublime_search.fuzzy_match("abc", "axbxc")
    assert score1 > score2


def test_custom_parameters():
    """Test that custom scoring parameters work as expected."""
    # Default parameters
    _, score1 = sublime_search.fuzzy_match("abc", "a_b_c")

    # Increased separator bonus
    _, score2 = sublime_search.fuzzy_match("abc", "a_b_c", sep_bonus=20)
    assert score2 > score1

    # Increased adjacent bonus
    _, score3 = sublime_search.fuzzy_match("abc", "abc", adj_bonus=20)
    _, score4 = sublime_search.fuzzy_match("abc", "abc")
    assert score3 > score4


def test_get_best_matches():
    """Test the get_best_matches function."""
    candidates = ["abcdef", "xabc", "testing", "a_b_c", "zzzabc"]
    results = sublime_search.get_best_matches("abc", candidates)

    # Should return all matches
    assert len(results) > 0

    # Should be sorted by score (highest first)
    scores = [score for _, score in results]
    assert all(scores[i] >= scores[i + 1] for i in range(len(scores) - 1))

    # First result should be the best match
    assert results[0][0] in candidates


def test_empty_inputs():
    """Test handling of empty inputs."""
    # Empty pattern
    is_match, _ = sublime_search.fuzzy_match("", "abcdef")
    assert is_match is False  # or True, depending on your implementation

    # Empty string
    is_match, _ = sublime_search.fuzzy_match("abc", "")
    assert is_match is False

    # Empty candidates list
    results = sublime_search.get_best_matches("abc", [])
    assert len(results) == 0


@pytest.mark.parametrize(
    "pattern,text,expected",
    [
        ("abc", "abcdef", True),
        ("xyz", "abcdef", False),
        ("a", "a", True),
        ("ac", "abc", True),
        ("", "", False),  # Adjust based on your implementation
        # ("čěš", "čtyři české švestky", True),  # Unicode test
    ],
)
def test_fuzzy_match_parametrized(pattern, text, expected):
    """Parametrized tests for various input combinations."""
    is_match, _ = sublime_search.fuzzy_match(pattern, text)
    assert is_match is expected


def test_performance():
    """Simple performance benchmark."""
    import time

    # Create a large dataset
    candidates = ["aaaaab", "aacb", "abc", "abbaab"] * 2500

    start = time.time()
    results = sublime_search.get_best_matches("aab", candidates)
    duration = time.time() - start

    # Just ensure it completes in reasonable time, not a strict test
    assert duration < 1.0, f"Took too long: {duration:.2f} seconds"
