# sublime-search

A simple, fast, Rust implementation of sublime-text style fuzzy matching for Python.

## Installation

```bash
pip install sublime-search
uv add sublime-search
```

## Usage

```python
import sublime_search

# Check if a pattern matches a string with a score
is_match, score = sublime_search.fuzzy_match("abc", "abcdef")
print(f"Match: {is_match}, Score: {score}")

# Find best matching strings from a list of candidates
results = sublime_search.get_best_matches("abc", ["abcdef", "xabc", "testing"])
for candidate, score in results:
    print(f"{candidate}: {score}")
```
