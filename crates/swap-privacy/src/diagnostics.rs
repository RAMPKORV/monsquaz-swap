/// Values that must not appear in a diagnostics bundle.
#[derive(Debug, Clone, Default)]
pub struct RedactionSpec {
    pub secrets: Vec<String>,
    pub paths: Vec<String>,
    pub user_ids: Vec<String>,
}

impl RedactionSpec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn secret(mut self, value: impl Into<String>) -> Self {
        self.secrets.push(value.into());
        self
    }

    pub fn path(mut self, value: impl Into<String>) -> Self {
        self.paths.push(value.into());
        self
    }

    pub fn user_id(mut self, value: impl Into<String>) -> Self {
        self.user_ids.push(value.into());
        self
    }
}

/// Redact secrets, absolute paths, and Discord user IDs from diagnostics.
///
/// Longer values are replaced first so a path or token cannot leave a partial
/// fragment behind.
pub fn redact_diagnostics(input: &str, spec: &RedactionSpec) -> String {
    let mut output = input.to_string();
    for value in sorted(&spec.secrets) {
        output = output.replace(value, "[redacted]");
    }
    for value in sorted(&spec.paths) {
        output = output.replace(value, "[path]");
    }
    for value in sorted(&spec.user_ids) {
        output = output.replace(value, "[user]");
    }
    output
}

fn sorted(values: &[String]) -> Vec<&str> {
    let mut values: Vec<&str> = values
        .iter()
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .collect();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_path_and_user_id_are_redacted() {
        let spec = RedactionSpec::new()
            .secret("abc.def.ghi")
            .path("/home/patrik/Events")
            .user_id("123456789012345678");
        let input = "token abc.def.ghi path /home/patrik/Events/users/123456789012345678";
        let output = redact_diagnostics(input, &spec);
        assert!(!output.contains("abc.def.ghi"));
        assert!(!output.contains("/home/patrik/Events"));
        assert!(!output.contains("123456789012345678"));
        assert!(output.contains("[redacted]"));
        assert!(output.contains("[path]"));
        assert!(output.contains("[user]"));
    }

    #[test]
    fn empty_values_are_ignored() {
        let spec = RedactionSpec {
            secrets: vec![String::new()],
            ..RedactionSpec::default()
        };
        assert_eq!(redact_diagnostics("plain text", &spec), "plain text");
    }

    #[test]
    fn a_longer_value_is_replaced_before_its_prefix() {
        let spec = RedactionSpec::new()
            .secret("token-abc")
            .secret("token-abcdef");
        let output = redact_diagnostics("token-abcdef", &spec);
        assert_eq!(output, "[redacted]");
    }
}
