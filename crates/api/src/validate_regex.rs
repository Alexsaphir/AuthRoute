use regex::Regex;

/// Why a path regex failed to compile.
#[derive(Debug, thiserror::Error)]
#[error("pathRegex is not a valid regular expression: {0}")]
pub struct PathRegexError(String);

/// Compile a path regex with the same engine used at request time (ADR-0006).
pub fn compile_path_regex(pattern: &str) -> Result<Regex, PathRegexError> {
    Regex::new(pattern).map_err(|e| PathRegexError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_and_invalid_regexes() {
        assert!(compile_path_regex(r"^/public(/.*)?$").is_ok());
        let err = compile_path_regex(r"^/api(/.*$").unwrap_err();
        assert!(matches!(err, PathRegexError(_)), "got {err:?}");
    }
}
