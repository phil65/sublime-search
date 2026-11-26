"""Regression tests: panics on non-ASCII text, line endings, GIL release, determinism."""

from __future__ import annotations

import sys
import threading
import time

import pytest
from sublime_search import (
    RetryableError,
    apply_diff_hunks,
    replace_content,
    trim_diff,
    try_replace_content,
)


class TestMultiByteMatches:
    def test_repeated_match_starting_with_umlaut_is_multiple_matches(self) -> None:
        with pytest.raises(RetryableError, match="(?i)multiple"):
            replace_content("äx\näx\n", "äx", "y")

    def test_try_replace_content_does_not_raise(self) -> None:
        result = try_replace_content("äx\näx\n", "äx", "y")
        assert not result
        assert result.error_type == "multiple_matches"
        assert result.locations == [1, 2]

    def test_line_hint_picks_the_closest_multibyte_match(self) -> None:
        result = replace_content("→a\n→a\n→a\n", "→a", "b", line_hint=2)
        assert result.content == "→a\nb\n→a\n"
        assert result.start_line == 2

    def test_emoji_and_cjk(self) -> None:
        result = replace_content("😀 x\n漢字\n😀 x\n", "😀 x", "y", line_hint=3)
        assert result.content == "😀 x\n漢字\ny\n"


class TestUnicodeIndentation:
    def test_trim_diff_with_ideographic_space_indent(self) -> None:
        assert trim_diff("+  a\n+\u3000b") == "+  a\n+\u3000b"

    def test_trim_diff_with_nbsp_and_tabs(self) -> None:
        assert trim_diff("+\t\ta\n+\t\u00a0b") == "+\ta\n+\u00a0b"

    def test_trim_diff_still_trims_ascii_indent(self) -> None:
        assert trim_diff("+    a\n-    b\n      c") == "+a\n-b\n  c"

    def test_indentation_flexible_with_unicode_whitespace(self) -> None:
        content = "x\n  \u3000a\n  b\ny"
        result = try_replace_content(content, "\u3000a\nb\nzz", "q")
        assert not result  # no panic, just not found


class TestLineEndings:
    def test_crlf_file_keeps_crlf(self) -> None:
        result = replace_content("a\r\nb\r\nc\r\n", "b", "B")
        assert result.content == "a\r\nB\r\nc\r\n"

    def test_crlf_file_multiline_replacement(self) -> None:
        result = replace_content("a\r\nb\r\nc\r\n", "a\nb", "x\ny")
        assert result.content == "x\r\ny\r\nc\r\n"

    def test_lf_file_stays_lf(self) -> None:
        result = replace_content("a\nb\nc\n", "b", "B\r\nC")
        assert result.content == "a\nB\nC\nc\n"

    def test_diff_on_crlf_file(self) -> None:
        diff = "```diff\n-b\n+B\n```"
        assert apply_diff_hunks("a\r\nb\r\nc\r\n", diff).content == "a\r\nB\r\nc\r\n"


def _not_found_case() -> tuple[str, str]:
    content = "\n".join(f"    value_{i} = compute({i}, alpha, beta)" for i in range(3000))
    search = "\n".join(f"    other_{i} = something_else({i})" for i in range(150))
    return content, search


class TestPerformance:
    def test_not_found_releases_the_gil(self) -> None:
        if not getattr(sys, "_is_gil_enabled", lambda: True)():
            pytest.skip("free-threaded build: no GIL to release")
        content, search = _not_found_case()
        ticks = 0
        done = threading.Event()

        def ticker() -> None:
            nonlocal ticks
            while not done.is_set():
                ticks += 1
                time.sleep(0.005)

        thread = threading.Thread(target=ticker)
        thread.start()
        start = time.perf_counter()
        try:
            assert not try_replace_content(content, search, "x")
        finally:
            elapsed = time.perf_counter() - start
            done.set()
            thread.join()
        expected = elapsed / 0.005
        assert ticks > expected * 0.5, (ticks, elapsed)

    def test_not_found_is_fast(self) -> None:
        content, search = _not_found_case()
        start = time.perf_counter()
        assert not try_replace_content(content, search, "x")
        assert time.perf_counter() - start < 2.0  # noqa: PLR2004

    def test_closest_match_is_deterministic(self) -> None:
        block = "def f():\n    return 1\n"
        content = (block + "\n") * 30
        search = "def f():\n    return 2\n    pass"
        results = {
            (r.closest_match.start_line if r.closest_match else None)
            for r in (try_replace_content(content, search, "x") for _ in range(20))
        }
        assert len(results) == 1
