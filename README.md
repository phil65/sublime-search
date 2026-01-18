# sublime-search

A simple, fast, Rust implementation of sublime-text style fuzzy matching for Python.

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

#### Full Example

See [examples/streaming_matcher_demo.py](examples/streaming_matcher_demo.py) for a comprehensive
demonstration of all features including exact matching, fuzzy matching with typos,
line hints for disambiguation, and real-time editing simulation.
