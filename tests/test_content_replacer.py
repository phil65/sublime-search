"""Tests for content_replacer improvements.

These tests verify the improvements outlined in IMPROVEMENTS.md:
1. Empty string validation
2. Block anchor threshold
3. CRLF line ending handling
4. Enhanced multiple matches error message
"""

from __future__ import annotations

import pytest
from sublime_search import replace_content, try_replace_content


class TestEmptyStringValidation:
    """Tests for empty string validation improvement."""

    def test_empty_old_string_gives_clear_error(self) -> None:
        """Empty old_string should give a clear 'cannot be empty' error."""
        with pytest.raises(ValueError, match="cannot be empty"):
            replace_content("some content", "", "new text")

    def test_try_replace_empty_string(self) -> None:
        """try_replace_content with empty string should return error details."""
        result = try_replace_content("some content", "", "new text")

        assert not result.success
        assert result.error_type == "not_found"
        assert result.error is not None
        assert "cannot be empty" in result.error


class TestCRLFNormalization:
    """Tests for CRLF line ending normalization improvement."""

    def test_crlf_content_with_lf_search(self) -> None:
        """Content with CRLF should match search text with LF."""
        content = "line 1\r\nline 2\r\nline 3"
        result = replace_content(content, "line 2", "replaced")

        assert "replaced" in result.content
        assert result.start_line == 2

    def test_lf_content_with_crlf_search(self) -> None:
        """Content with LF should match search text with CRLF."""
        content = "line 1\nline 2\nline 3"
        result = replace_content(content, "line 1\r\nline 2", "replaced")

        assert "replaced" in result.content

    def test_multiline_crlf_normalization(self) -> None:
        """Multi-line CRLF content and search should work together."""
        content = "def foo():\r\n    return 1\r\n"
        result = replace_content(
            content,
            "def foo():\r\n    return 1",
            "def foo():\n    return 2",
        )

        assert "return 2" in result.content

    def test_mixed_crlf_in_multiline_block(self) -> None:
        """Content with CRLF should match multi-line search with LF."""
        content = "def test():\r\n    x = 1\r\n    return x"
        result = replace_content(
            content,
            "def test():\n    x = 1\n    return x",
            "def test():\n    return 42",
        )

        assert "return 42" in result.content


class TestMultipleMatchesPreviews:
    """Tests for enhanced multiple matches error message with previews."""

    def test_multiple_matches_shows_previews(self) -> None:
        """Multiple matches error should include content previews."""
        content = "def foo():  # Helper\n    pass\n\ndef foo():  # Main\n    pass"

        with pytest.raises(ValueError) as exc_info:
            replace_content(content, "def foo():", "def bar():")

        error_msg = str(exc_info.value)

        # Should include preview section
        assert "Match previews" in error_msg
        # Should include line numbers with content
        assert "Line " in error_msg

    def test_multiple_matches_includes_location_context(self) -> None:
        """Error message should help distinguish between matches."""
        content = "x = 1  # first assignment\ny = 2\nx = 1  # second assignment"

        with pytest.raises(ValueError) as exc_info:
            replace_content(content, "x = 1", "x = 42")

        error_msg = str(exc_info.value)

        # Should show multiple locations
        assert "lines:" in error_msg
        # Should include preview helping identify which is which
        assert "Match previews" in error_msg

    def test_try_replace_multiple_matches_has_locations(self) -> None:
        """try_replace_content should return locations for multiple matches."""
        content = "test\nother\ntest"
        result = try_replace_content(content, "test", "replaced")

        assert not result.success
        assert result.error_type == "multiple_matches"
        assert result.locations is not None
        assert result.locations == [1, 3]


class TestBlockAnchorThreshold:
    """Tests for block anchor similarity threshold improvement."""

    def test_block_anchor_accepts_high_similarity(self) -> None:
        """Block anchor should accept matches with high middle similarity."""
        content = "def function():\n    yield item * 2\n    return True"
        # Only one character different in middle - should still match
        result = replace_content(
            content,
            "def function():\n    yield item * 3\n    return True",
            "def function():\n    return False",
        )

        # Should match via block_anchor strategy
        assert "return False" in result.content

    def test_block_anchor_rejects_very_low_similarity(self) -> None:
        """Block anchor should reject when middle content is completely different."""
        content = "def function():\n    completely different code here\n    return True"

        # Try to match with very different middle content
        result = try_replace_content(
            content,
            "def function():\n    something entirely else blah blah\n    return True",
            "def function():\n    new code\n    return True",
        )

        # Should fail because middle line similarity is too low
        # With threshold 0.6, very different content should not match
        if result.success:
            # If it matched, it should NOT be via block_anchor
            assert result.result is not None
            assert result.result.strategy != "block_anchor"


class TestReplaceContentBasic:
    """Basic tests for replace_content to ensure existing functionality works."""

    def test_simple_replacement(self) -> None:
        """Basic replacement should work."""
        content = "hello world"
        result = replace_content(content, "world", "universe")

        assert result.content == "hello universe"
        assert result.matched_text == "world"
        assert result.strategy == "simple"

    def test_multiline_replacement(self) -> None:
        """Multi-line replacement should work."""
        content = "def foo():\n    return 1\n\ndef bar():\n    pass"
        result = replace_content(
            content, "def foo():\n    return 1", "def foo():\n    return 42"
        )

        assert "return 42" in result.content
        assert "def bar():" in result.content

    def test_line_hint_disambiguates(self) -> None:
        """Line hint should select the closest match."""
        content = "test\nother\ntest"
        result = replace_content(content, "test", "replaced", line_hint=3)

        assert result.content == "test\nother\nreplaced"
        assert result.start_line == 3

    def test_replace_all(self) -> None:
        """replace_all=True should replace all occurrences."""
        content = "foo bar\nfoo bar\nfoo bar"
        result = replace_content(content, "foo bar", "replaced", replace_all=True)

        assert result.content == "replaced\nreplaced\nreplaced"

    def test_no_change_error(self) -> None:
        """old_string == new_string should raise error."""
        with pytest.raises(ValueError, match="different"):
            replace_content("hello world", "hello", "hello")

    def test_not_found_has_fuzzy_match(self) -> None:
        """Not found error should include fuzzy match suggestion."""
        content = "def hello_world():\n    return True"
        result = try_replace_content(
            content, "def hello_word():\n    return True", "new"
        )

        assert not result.success
        assert result.error_type == "not_found"
        assert result.closest_match is not None
        assert result.closest_match.similarity > 0.8


class TestTryReplaceContent:
    """Tests for try_replace_content function."""

    def test_try_replace_success(self) -> None:
        """Successful replacement should return truthy result."""
        result = try_replace_content("hello world", "world", "universe")

        assert result.success
        assert bool(result)
        assert result.result is not None
        assert result.result.content == "hello universe"
        assert result.error is None

    def test_try_replace_not_found(self) -> None:
        """Not found should return falsy result with error details."""
        result = try_replace_content("hello world", "missing", "new")

        assert not result.success
        assert not bool(result)
        assert result.result is None
        assert result.error_type == "not_found"
        assert result.error is not None

    def test_try_replace_multiple_matches(self) -> None:
        """Multiple matches should return falsy result with locations."""
        result = try_replace_content("foo\nbar\nfoo", "foo", "replaced")

        assert not result.success
        assert result.error_type == "multiple_matches"
        assert result.locations == [1, 3]
