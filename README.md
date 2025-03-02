# fuzzy_match_rs

A fast, Rust implementation of sublime-text style fuzzy matching for Python.

## Installation

```bash
pip install fuzzy_match_rs
uv add fuzzy_match_rs
```

## Usage

```python
import fuzzy_match_rs

# Check if a pattern matches a string with a score
is_match, score = fuzzy_match_rs.fuzzy_match("abc", "abcdef")
print(f"Match: {is_match}, Score: {score}")

# Find best matching strings from a list of candidates
results = fuzzy_match_rs.get_best_matches("abc", ["abcdef", "xabc", "testing"])
for candidate, score in results:
    print(f"{candidate}: {score}")
```
