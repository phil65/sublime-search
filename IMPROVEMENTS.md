# Proposed Improvements for sublime-search

## Critical Issues

### 1. Empty String Validation

**Problem**: Empty `old_string` gives confusing "multiple matches at line 1" error instead of a clear message.

**Fix**: Add early validation in `replace_content`:
```rust
if old_string.is_empty() {
    return Err(ReplaceError::NotFound {
        message: "Search text cannot be empty".to_string(),
        closest_match: None,
    });
}
```

### 2. Block Anchor Threshold Too Permissive

**Problem**: `SINGLE_CANDIDATE_THRESHOLD = 0.0` means any block where first/last lines match is accepted, regardless of middle content similarity. This can replace wrong code silently.

**Example**:
```python
# Content has: yield item * 2
# Search has:  yield item * 3
# Block anchor matches anyway because first/last lines match
```

**Fix**: Increase threshold to require meaningful similarity:
```rust
const SINGLE_CANDIDATE_THRESHOLD: f64 = 0.6;  // Was 0.0
```

### 3. CRLF Line Ending Handling

**Problem**: When content has `\r\n` (Windows) and search text has `\n` (Unix), strategies may find matches but location reporting gets confused, showing "multiple locations" for a single match.

**Fix**: Normalize line endings at the start of `replace_content`:
```rust
// Normalize line endings for consistent matching
let content = content.replace("\r\n", "\n");
let old_string = old_string.replace("\r\n", "\n");
// Also normalize new_string to maintain consistency
let new_string = new_string.replace("\r\n", "\n");
```

## Nice-to-Have Improvements

### 4. Enhanced Multiple Matches Error Message

**Problem**: Current error shows line numbers but not content, making it hard to distinguish which match is intended.

**Current**:
```
Pattern found at multiple locations (lines: 10, 25, 40).
Search text starts with: "def foo():"
```

**Proposed**:
```
Pattern found at multiple locations (lines: 10, 25, 40).
Search text starts with: "def foo():"

Match previews:
  Line 10: def foo():  # Helper function
  Line 25: def foo():  # Main implementation  
  Line 40: def foo():  # Test version
```

**Fix**: Update `build_multiple_matches_error` to include content context:
```rust
fn build_multiple_matches_error(content: &str, old_string: &str, locations: &[usize]) -> String {
    // ... existing code ...
    
    // Add preview of each location
    let lines: Vec<&str> = content.lines().collect();
    let mut previews = String::new();
    for &loc in locations.iter().take(5) {
        let line_idx = loc.saturating_sub(1);
        if line_idx < lines.len() {
            let preview: String = lines[line_idx].chars().take(60).collect();
            let ellipsis = if lines[line_idx].len() > 60 { "..." } else { "" };
            previews.push_str(&format!("\n  Line {}: {}{}", loc, preview, ellipsis));
        }
    }
    
    format!(
        "Pattern found at multiple locations (lines: {}).\n\
         Search text starts with: \"{}{}\"\n\
         \nMatch previews:{}\n\n\
         To fix, include more surrounding context...",
        location_str, search_preview, ellipsis, previews
    )
}
```

Note: This requires passing `content` to `build_multiple_matches_error`.

### 5. Strategy Attribution in Errors

**Problem**: When a match fails due to multiple occurrences, user doesn't know which matching strategy found it, which could help debugging.

**Fix**: Include strategy name in `MultipleMatches` error or add to error message.

## Testing Checklist

After implementing fixes, verify:

- [ ] Empty string gives clear error message
- [ ] Block anchor requires >60% similarity for single candidates
- [ ] CRLF content with LF search text works correctly
- [ ] Multiple matches error shows line content previews
- [ ] All existing tests still pass
- [ ] No performance regression on large files
