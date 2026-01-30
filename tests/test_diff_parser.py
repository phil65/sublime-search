"""Tests for the diff parsing and application functionality."""

from __future__ import annotations

import pytest
from sublime_search import (
    apply_diff_hunks,
    apply_diff_hunks_with_hint,
    parse_locationless_diff,
    try_apply_diff_hunks,
)


class TestParseLocationlessDiff:
    """Tests for parse_locationless_diff function."""

    def test_parse_simple_hunk(self) -> None:
        diff = " context\n-old line\n+new line\n context2"
        hunks = parse_locationless_diff(diff)

        assert len(hunks) == 1
        assert hunks[0].old_text == "context\nold line\ncontext2"
        assert hunks[0].new_text == "context\nnew line\ncontext2"

    def test_parse_multiple_hunks(self) -> None:
        diff = " ctx1\n-old1\n+new1\n\n ctx2\n-old2\n+new2"
        hunks = parse_locationless_diff(diff)

        assert len(hunks) == 2
        assert hunks[0].old_text == "ctx1\nold1"
        assert hunks[0].new_text == "ctx1\nnew1"
        assert hunks[1].old_text == "ctx2\nold2"
        assert hunks[1].new_text == "ctx2\nnew2"

    def test_parse_with_diff_tags(self) -> None:
        diff = "Some preamble\n<diff>\n context\n-old\n+new\n</diff>\nSome postamble"
        hunks = parse_locationless_diff(diff)

        assert len(hunks) == 1
        assert hunks[0].old_text == "context\nold"
        assert hunks[0].new_text == "context\nnew"

    def test_parse_with_code_block(self) -> None:
        diff = "```diff\n context\n-old\n+new\n```"
        hunks = parse_locationless_diff(diff)

        assert len(hunks) == 1
        assert hunks[0].old_text == "context\nold"
        assert hunks[0].new_text == "context\nnew"

    def test_parse_skips_diff_headers(self) -> None:
        diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n context\n-old\n+new"
        hunks = parse_locationless_diff(diff)

        assert len(hunks) == 1
        assert hunks[0].old_text == "context\nold"
        assert hunks[0].new_text == "context\nnew"

    def test_parse_pure_addition(self) -> None:
        diff = " context\n+new line\n context2"
        hunks = parse_locationless_diff(diff)

        assert len(hunks) == 1
        assert hunks[0].old_text == "context\ncontext2"
        assert hunks[0].new_text == "context\nnew line\ncontext2"

    def test_parse_pure_deletion(self) -> None:
        diff = " context\n-deleted line\n context2"
        hunks = parse_locationless_diff(diff)

        assert len(hunks) == 1
        assert hunks[0].old_text == "context\ndeleted line\ncontext2"
        assert hunks[0].new_text == "context\ncontext2"

    def test_parse_empty_returns_empty(self) -> None:
        hunks = parse_locationless_diff("")
        assert len(hunks) == 0

    def test_parse_no_diff_lines_returns_empty(self) -> None:
        diff = "Just some regular text\nwithout any diff markers"
        hunks = parse_locationless_diff(diff)
        assert len(hunks) == 0

    def test_hunk_has_raw_text(self) -> None:
        diff = " context\n-old\n+new"
        hunks = parse_locationless_diff(diff)

        assert len(hunks) == 1
        assert " context\n-old\n+new" in hunks[0].raw

    def test_hunk_repr(self) -> None:
        diff = " context\n-old\n+new"
        hunks = parse_locationless_diff(diff)

        repr_str = repr(hunks[0])
        assert "DiffHunk" in repr_str
        assert "old=" in repr_str
        assert "new=" in repr_str


class TestApplyDiffHunks:
    """Tests for apply_diff_hunks function."""

    def test_apply_simple_diff(self) -> None:
        content = "line1\nold line\nline3"
        diff = " line1\n-old line\n+new line\n line3"

        result = apply_diff_hunks(content, diff)

        assert result.content == "line1\nnew line\nline3"
        assert result.applied_count == 1
        assert result.total_count == 1
        assert result.all_applied

    def test_apply_multiple_hunks(self) -> None:
        content = "aaa\nbbb\nccc\nddd\neee"
        diff = " aaa\n-bbb\n+BBB\n ccc\n\n ccc\n-ddd\n+DDD\n eee"

        result = apply_diff_hunks(content, diff)

        assert result.content == "aaa\nBBB\nccc\nDDD\neee"
        assert result.applied_count == 2
        assert result.all_applied

    def test_apply_no_hunks_raises(self) -> None:
        content = "some content"
        diff = "no diff markers here"

        with pytest.raises(ValueError, match="No diff hunks found"):
            apply_diff_hunks(content, diff)

    def test_apply_no_match_raises(self) -> None:
        content = "completely different content"
        diff = " context\n-old\n+new"

        with pytest.raises(ValueError, match="could be applied"):
            apply_diff_hunks(content, diff)

    def test_apply_partial_success(self) -> None:
        content = "line1\nmatch\nline3"
        diff = " line1\n-match\n+MATCH\n\n nomatch\n-x\n+y"

        result = apply_diff_hunks(content, diff)

        # First hunk succeeds, second fails
        assert result.applied_count == 1
        assert result.total_count == 2
        assert not result.all_applied
        assert result.content == "line1\nMATCH\nline3"

    def test_hunk_results_detail(self) -> None:
        content = "ctx\nold\nctx2"
        diff = " ctx\n-old\n+new\n ctx2"

        result = apply_diff_hunks(content, diff)

        assert len(result.hunk_results) == 1
        assert result.hunk_results[0].success
        assert result.hunk_results[0].strategy is not None
        assert result.hunk_results[0].start_line is not None

    def test_preserves_indentation(self) -> None:
        content = "def foo():\n    old_code()\n    more()"
        diff = " def foo():\n-    old_code()\n+    new_code()\n     more()"

        result = apply_diff_hunks(content, diff)

        assert "    new_code()" in result.content

    def test_result_bool_true_when_all_applied(self) -> None:
        content = "ctx\nold\nctx2"
        diff = " ctx\n-old\n+new\n ctx2"

        result = apply_diff_hunks(content, diff)
        assert bool(result) is True

    def test_result_bool_false_when_partial(self) -> None:
        content = "ctx\nold\nctx2"
        diff = " ctx\n-old\n+new\n ctx2\n\n nomatch\n-x\n+y"

        result = apply_diff_hunks(content, diff)
        assert bool(result) is False

    def test_result_repr(self) -> None:
        content = "ctx\nold\nctx2"
        diff = " ctx\n-old\n+new\n ctx2"

        result = apply_diff_hunks(content, diff)
        repr_str = repr(result)

        assert "ApplyDiffResult" in repr_str
        assert "1/1" in repr_str


class TestApplyDiffHunksWithHint:
    """Tests for apply_diff_hunks_with_hint function."""

    def test_apply_with_line_hint(self) -> None:
        # Content with duplicate patterns
        content = "def a():\n    x = 1\n\ndef b():\n    x = 1"
        diff = " def b():\n-    x = 1\n+    x = 2"

        # Line hint points to the second occurrence (line 5)
        result = apply_diff_hunks_with_hint(content, diff, line_hint=5)

        assert "def b():\n    x = 2" in result.content
        # First occurrence should be unchanged
        assert "def a():\n    x = 1" in result.content


class TestTryApplyDiffHunks:
    """Tests for try_apply_diff_hunks function."""

    def test_returns_none_for_no_hunks(self) -> None:
        result = try_apply_diff_hunks("content", "no diff markers")
        assert result is None

    def test_returns_result_for_no_match(self) -> None:
        result = try_apply_diff_hunks("different", " ctx\n-old\n+new")

        assert result is not None
        assert result.applied_count == 0
        assert result.total_count == 1
        assert not result.all_applied

    def test_returns_result_for_success(self) -> None:
        result = try_apply_diff_hunks("ctx\nold", " ctx\n-old\n+new")

        assert result is not None
        assert result.applied_count == 1
        assert result.all_applied
        assert result.content == "ctx\nnew"

    def test_with_line_hint(self) -> None:
        content = "a\nx\nb\nx"
        diff = " b\n-x\n+y"

        result = try_apply_diff_hunks(content, diff, line_hint=4)

        assert result is not None
        assert result.applied_count == 1


class TestHunkResult:
    """Tests for HunkResult class."""

    def test_hunk_result_success(self) -> None:
        content = "ctx\nold\nctx2"
        diff = " ctx\n-old\n+new\n ctx2"

        result = apply_diff_hunks(content, diff)
        hunk_result = result.hunk_results[0]

        assert hunk_result.success
        assert hunk_result.strategy is not None
        assert hunk_result.error is None
        assert bool(hunk_result) is True

    def test_hunk_result_failure(self) -> None:
        content = "ctx\nold\nctx2\n\ndifferent"
        diff = " ctx\n-old\n+new\n ctx2\n\n nomatch\n-x\n+y"

        result = apply_diff_hunks(content, diff)

        # Second hunk should fail
        assert len(result.hunk_results) == 2
        failed_result = result.hunk_results[1]
        assert not failed_result.success
        assert failed_result.error is not None
        assert bool(failed_result) is False

    def test_hunk_result_repr(self) -> None:
        content = "ctx\nold\nctx2"
        diff = " ctx\n-old\n+new\n ctx2"

        result = apply_diff_hunks(content, diff)
        repr_str = repr(result.hunk_results[0])

        assert "HunkResult" in repr_str
        assert "success=True" in repr_str
