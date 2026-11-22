# sublime-search

[![PyPI](https://img.shields.io/pypi/v/sublime-search.svg)](https://pypi.org/project/sublime-search/)
[![Python versions](https://img.shields.io/pypi/pyversions/sublime-search.svg)](https://pypi.org/project/sublime-search/)
[![CI](https://github.com/phil65/sublime-search/actions/workflows/build.yml/badge.svg)](https://github.com/phil65/sublime-search/actions/workflows/build.yml)

Fast fuzzy matching and fuzzy text editing for Python, implemented in Rust (PyO3).

- Sublime Text style fuzzy matching with scoring (`fuzzy_match`, `get_best_matches`)
- Robust search-and-replace with eight fallback strategies, built for LLM code-editing
  tools where the "old text" rarely matches exactly (`replace_content`)
- Parsing and applying locationless unified diffs (`apply_diff_hunks`)
- A streaming matcher that locates text in a file while it is still arriving
  (`StreamingFuzzyMatcher`)
- Typed (`py.typed` + stubs), prebuilt wheels for CPython 3.11+ (including free-threaded builds)

## Installation

```bash
pip install sublime-search
uv add sublime-search
```

## Usage

### Basic Fuzzy Matching

```python
import sublime_search

# Check if a pattern matches a string with a score
is_match, score = sublime_search.fuzzy_match("abc", "abcdef")
print(f"Match: {is_match}, Score: {score}")

# Find best matching strings from a list of candidates
results = sublime_search.get_best_matches("abc", ["abcdef", "xabc", "testing"])
for candidate, score in results:
    print(f"{candidate}: {score}")

# Simple match check (no scoring)
if sublime_search.fuzzy_match_simple("abc", "aXbXc"):
    print("Pattern found!")
```

### Content Replacement

Sophisticated text replacement with multiple fallback strategies for finding and
replacing text. Useful for code editing tools where exact matches may fail due to
whitespace, indentation, or minor differences.

```python
from sublime_search import replace_content, try_replace_content

content = """
def hello():
    print("Hello, World!")
    return True
"""

# Simple replacement
result = replace_content(content, 'print("Hello, World!")', 'print("Hello, Python!")')
print(result.content)      # The modified content
print(result.strategy)     # Which matching strategy was used
print(result.start_line)   # Line number where match was found
print(result.matched_text) # The actual text that was matched

# Replace all occurrences
content = "foo bar\nother\nfoo bar"
result = replace_content(content, "foo bar", "replaced", replace_all=True)
# Result: "replaced\nother\nreplaced"

# Use line_hint to disambiguate multiple matches
result = replace_content(content, "foo bar", "replaced", line_hint=3)
# Replaces the occurrence closest to line 3
```

#### Matching Strategies

The replacer tries these strategies in order until one succeeds:

1. **Simple** - Direct string matching
2. **Line-trimmed** - Line-by-line matching with trimmed whitespace
3. **Block-anchor** - Multi-line matching using first/last line anchors with similarity scoring
4. **Whitespace-normalized** - Matches after normalizing whitespace
5. **Indentation-flexible** - Matches with different indentation levels
6. **Escape-normalized** - Handles escape sequence differences (`\n` vs newline)
7. **Trimmed-boundary** - Matches with trimmed leading/trailing whitespace
8. **Context-aware** - Uses anchor lines for context-based matching

#### Error Handling

Recoverable failures (text not found, ambiguous matches, empty `old_string`) raise
`RetryableError`, so an agent framework can map them to its own "retry" signal.
`ValueError` is reserved for programming errors (`old_string == new_string`).

```python
from sublime_search import RetryableError, replace_content, try_replace_content

try:
    result = replace_content(content, "not found", "replacement")
except RetryableError as e:
    print(e)  # Detailed error with fuzzy match suggestions

# try_replace_content returns a typed result object instead of raising
result = try_replace_content(content, "not found", "replacement")
if result:  # TryReplaceResult is falsy on failure
    print(result.result.content)
else:
    print(result.error_type)    # "not_found", "multiple_matches", or "no_change"
    print(result.error)         # Detailed error message
    print(result.retryable)     # True for not_found / multiple_matches
    print(result.closest_match) # FuzzyMatchInfo if a close match was found
    print(result.locations)     # Line numbers for multiple matches
```

### Locationless Diffs

Apply diffs without line numbers (the format LLMs tend to produce). Each hunk's
context and removed lines are located with the same fallback strategies as
`replace_content`. Input wrapped in `<diff>...</diff>` tags or a diff code fence
is accepted, and `---`/`+++`/`@@` headers are skipped.

```python
from sublime_search import apply_diff_hunks, parse_locationless_diff

content = "def add(a, b):\n    return a - b\n"
diff = """\
 def add(a, b):
-    return a - b
+    return a + b
"""

hunks = parse_locationless_diff(diff)
print(hunks[0].old_text, hunks[0].new_text)

result = apply_diff_hunks(content, diff)
print(result.content)  # "def add(a, b):\n    return a + b\n"
print(result.applied_count, result.total_count, result.all_applied)
for hunk in result.hunk_results:
    print(hunk.success, hunk.strategy, hunk.start_line, hunk.end_line)
```

`apply_diff_hunks` raises `RetryableError` if no hunk could be applied and `ValueError`
if the input contains no hunks. `apply_diff_hunks_with_hint(content, diff, line_hint=...)`
disambiguates repeated context, and `try_apply_diff_hunks(...)` never raises (it returns
`None` if no hunks were found, otherwise an `ApplyDiffResult` that may report failures).

### Streaming Fuzzy Matcher

For real-time matching scenarios like code editing where text arrives in chunks
(e.g., from an LLM streaming response):

```python
from sublime_search import StreamingFuzzyMatcher

source_code = """
def hello():
    return "world"

def goodbye():
    return "world"
"""

matcher = StreamingFuzzyMatcher(source_code)

# Stream chunks as they arrive
chunks = ["def hello", "():\n", "    return \"wor", "ld\"\n"]
for chunk in chunks:
    result = matcher.push(chunk)
    if result:
        matched_text = matcher.get_text(result)
        print(f"Current match: {matched_text!r}")

# Finalize and get all matches
matches = matcher.finish()
for match in matches:
    print(f"Final: {matcher.get_text(match)!r}")
```

#### Line Hints

When multiple locations match equally well, use a line hint to prefer matches
near a specific line:

```python
matcher = StreamingFuzzyMatcher(source_code)
# Prefer matches near line 5
result = matcher.push("return \"world\"\n", line_hint=5)
```

#### Properties

```python
matcher.query_lines   # Accumulated query lines
matcher.source_lines  # Source text split into lines
```

## API Reference

### Functions

| Function | Description |
|----------|-------------|
| `fuzzy_match(pattern, instring, ...)` | Check if pattern matches string, returns `(bool, score)`; bonuses/penalties are keyword-tunable |
| `fuzzy_match_simple(pattern, instring, case_sensitive=False)` | Simple in-order character match, returns `bool` |
| `get_best_matches(search_string, candidates)` | All matching candidates as `(candidate, score)`, sorted by score |
| `replace_content(content, old_string, new_string, replace_all=False, line_hint=None)` | Replace text with fallback strategies |
| `try_replace_content(...)` | Non-raising version of `replace_content`, returns `TryReplaceResult` |
| `trim_diff(diff_text)` | Remove common indentation from unified diff output |
| `parse_locationless_diff(diff_text)` | Parse a locationless diff into `DiffHunk`s |
| `apply_diff_hunks(content, diff_text, replace_all=False)` | Apply all hunks, returns `ApplyDiffResult` |
| `apply_diff_hunks_with_hint(content, diff_text, line_hint=None)` | Same, preferring matches near `line_hint` |
| `try_apply_diff_hunks(content, diff_text, replace_all=False, line_hint=None)` | Non-raising version, returns `ApplyDiffResult` or `None` |

### Classes

| Class | Description |
|-------|-------------|
| `StreamingFuzzyMatcher` | Incremental fuzzy matcher for streaming text |
| `MatchRange` | Byte range result from streaming matcher (start, end) |
| `ReplaceResult` | Result from replace_content (content, matched_text, start_line, end_line, strategy) |
| `TryReplaceResult` | Result from try_replace_content (success, result, error, error_type, closest_match, locations, retryable) |
| `FuzzyMatchInfo` | Fuzzy match details (similarity, start_line, end_line, text, diff) |
| `DiffHunk` | Parsed hunk (old_text, new_text, raw) |
| `HunkResult` | Per-hunk outcome (success, strategy, error, start_line, end_line) |
| `ApplyDiffResult` | Overall outcome (content, applied_count, total_count, hunk_results, all_applied) |
| `RetryableError` | Exception for recoverable matching failures |

## Performance

All core algorithms are implemented in Rust for maximum performance:
- Levenshtein distance calculation
- Fuzzy matching with scoring
- Multiple replacement strategies
- Streaming text processing

This makes the library suitable for high-frequency operations like real-time
code editing with LLM streaming responses.
