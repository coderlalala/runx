#![no_main]

// Differential property:
//   if `assert_yaml_parity_subset` returns Ok and serde_norway parses the
//   document into a mapping at the top level, then no unquoted key in that
//   mapping contains `": "` (colon-space). Quoted keys may contain colon-space
//   because they are unambiguous YAML; the parity validator's contract is "no
//   top-level ambiguous mapping construct"; serde_norway is the authoritative
//   reader; this asserts they agree on what got past the validator.
//
// Run with `cargo +nightly fuzz run fuzz_yaml_parity_subset -- -max_total_time=60`
// from inside `crates/runx-parser/fuzz`.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let parity = runx_parser::assert_yaml_parity_subset("fuzz", text);
    let parsed: Result<serde_norway::Value, _> = serde_norway::from_str(text);
    if let (Ok(()), Ok(serde_norway::Value::Mapping(map))) = (parity, parsed) {
        for (key, _) in map {
            if let serde_norway::Value::String(string_key) = key {
                assert!(
                    !string_key.contains(": ") || source_has_quoted_mapping_key(text, &string_key),
                    "validator accepted colon-space top-level key: {string_key:?}\ninput: {text:?}"
                );
            }
        }
    }
});

fn source_has_quoted_mapping_key(source: &str, expected: &str) -> bool {
    source.char_indices().any(|(index, char)| match char {
        '\'' | '"' => quoted_mapping_key_matches(source, index, char, expected),
        _ => false,
    })
}

fn quoted_mapping_key_matches(source: &str, start: usize, quote: char, expected: &str) -> bool {
    let Some(end) = quoted_scalar_end(source, start, quote) else {
        return false;
    };
    let rest = &source[end..];
    if !rest.trim_start().starts_with(':') {
        return false;
    }
    serde_norway::from_str::<serde_norway::Value>(&format!("{}: null", &source[start..end]))
        .ok()
        .and_then(single_mapping_key)
        .is_some_and(|key| key == expected)
}

fn single_mapping_key(value: serde_norway::Value) -> Option<String> {
    let serde_norway::Value::Mapping(map) = value else {
        return None;
    };
    let mut keys = map.into_iter().filter_map(|(key, _)| match key {
        serde_norway::Value::String(key) => Some(key),
        _ => None,
    });
    let key = keys.next()?;
    if keys.next().is_none() {
        Some(key)
    } else {
        None
    }
}

fn quoted_scalar_end(source: &str, start: usize, quote: char) -> Option<usize> {
    match quote {
        '"' => {
            let mut escaped = false;
            for (relative_index, char) in source[start + quote.len_utf8()..].char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                if char == '\\' {
                    escaped = true;
                    continue;
                }
                if char == '"' {
                    return Some(start + quote.len_utf8() + relative_index + quote.len_utf8());
                }
            }
            None
        }
        '\'' => {
            let mut chars = source[start + quote.len_utf8()..].char_indices().peekable();
            while let Some((relative_index, char)) = chars.next() {
                if char == '\'' {
                    if chars.peek().is_some_and(|(_, next)| *next == '\'') {
                        let _ = chars.next();
                        continue;
                    }
                    return Some(start + quote.len_utf8() + relative_index + quote.len_utf8());
                }
            }
            None
        }
        _ => None,
    }
}
