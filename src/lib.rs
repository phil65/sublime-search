use pyo3::prelude::*;
use std::cmp::max;

#[pyfunction]
#[pyo3(signature = (pattern, instring, adj_bonus=5, sep_bonus=10, camel_bonus=10, lead_penalty=-3, max_lead_penalty=-9, unmatched_penalty=-1))]
fn fuzzy_match(
    pattern: &str,
    instring: &str,
    adj_bonus: i32,
    sep_bonus: i32,
    camel_bonus: i32,
    lead_penalty: i32,
    max_lead_penalty: i32,
    unmatched_penalty: i32,
) -> (bool, i32) {
    let mut score = 0;
    let mut p_idx = 0;
    let p_len = pattern.chars().count();

    let mut prev_match = false;
    let mut prev_lower = false;
    let mut prev_sep = true; // so that matching first letter gets sep_bonus

    let mut best_letter = None;
    let mut best_lower = None;
    let mut best_letter_idx = None;
    let mut best_letter_score = 0;
    let mut matched_indices = Vec::new();

    // Convert pattern and instring to chars for proper Unicode handling
    let pattern_chars: Vec<char> = pattern.chars().collect();

    for (s_idx, s_char) in instring.chars().enumerate() {
        let p_char = if p_idx != p_len {
            Some(pattern_chars[p_idx])
        } else {
            None
        };

        let p_lower = p_char.map(|c| c.to_lowercase().next().unwrap());
        let s_lower = s_char.to_lowercase().next().unwrap();
        let s_upper = s_char.to_uppercase().next().unwrap();

        let next_match = p_char.is_some() && p_lower == Some(s_lower);
        let rematch = best_letter.is_some() && best_lower == Some(s_lower);

        let advanced = next_match && best_letter.is_some();
        let p_repeat = best_letter.is_some() && p_char.is_some() && best_lower == p_lower;

        if advanced || p_repeat {
            score += best_letter_score;
            matched_indices.push(best_letter_idx);
            best_letter = None;
            best_lower = None;
            best_letter_idx = None;
            best_letter_score = 0;
        }

        if next_match || rematch {
            let mut new_score = 0;

            // apply penalty for each letter before the first match
            if p_idx == 0 {
                score += max(s_idx as i32 * lead_penalty, max_lead_penalty);
            }

            // apply bonus for consecutive matches
            if prev_match {
                new_score += adj_bonus;
            }

            // apply bonus for matches after a separator
            if prev_sep {
                new_score += sep_bonus;
            }

            // apply bonus across camelCase boundaries
            if prev_lower && s_char == s_upper && s_lower != s_upper {
                new_score += camel_bonus;
            }

            // update pattern index iff the next pattern letter was matched
            if next_match {
                p_idx += 1;
            }

            // update best letter match (may be next or rematch)
            if new_score >= best_letter_score {
                // apply penalty for now-skipped letter
                if best_letter.is_some() {
                    score += unmatched_penalty;
                }
                best_letter = Some(s_char);
                best_lower = Some(s_lower);
                best_letter_idx = Some(s_idx);
                best_letter_score = new_score;
            }

            prev_match = true;
        } else {
            score += unmatched_penalty;
            prev_match = false;
        }

        prev_lower = s_char == s_lower && s_lower != s_upper;
        prev_sep = s_char == '_' || s_char == ' ';
    }

    if best_letter.is_some() {
        score += best_letter_score;
        matched_indices.push(best_letter_idx);
    }

    (p_idx == p_len, score)
}

#[pyfunction]
fn get_best_matches(search_string: &str, candidates: Vec<String>) -> PyResult<Vec<(String, i32)>> {
    let mut results = Vec::new();

    for candidate in candidates {
        let (matched, score) = fuzzy_match(search_string, &candidate, 5, 10, 10, -3, -9, -1);
        if matched {
            results.push((candidate, score));
        }
    }

    // Sort by score in descending order
    results.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(results)
}

/// A Python module implemented in Rust
#[pymodule]
fn _fuzzy_match_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(fuzzy_match, m)?)?;
    m.add_function(wrap_pyfunction!(get_best_matches, m)?)?;
    Ok(())
}
