use crate::error::{MudClientError, Result};

pub fn substitute_captures(template: &str, captures: &[String]) -> String {
    let mut output = String::new();
    let mut chars = template.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '{' || !chars.peek().is_some_and(char::is_ascii_digit) {
            output.push(character);
            continue;
        }
        let mut digits = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            if let Some(digit) = chars.next() {
                digits.push(digit);
            }
        }
        if chars.peek() == Some(&'}') {
            chars.next();
        }
        if let Ok(index) = digits.parse::<usize>() {
            output.push_str(captures.get(index).map(String::as_str).unwrap_or_default());
        }
    }
    output
}

pub fn validate_capture_references(
    template: &str,
    available_captures: usize,
    context: &str,
) -> Result<()> {
    let requested = capture_references(template)?;
    if let Some(requested) = requested.into_iter().max()
        && requested > available_captures
    {
        return Err(MudClientError::ConfigValidation(format!(
            "{context} references capture {{{requested}}}, but its pattern defines only {available_captures}"
        )));
    }
    Ok(())
}

pub fn has_capture_reference(template: &str) -> Result<bool> {
    Ok(!capture_references(template)?.is_empty())
}

fn capture_references(template: &str) -> Result<Vec<usize>> {
    let mut references = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '{' || !chars.peek().is_some_and(char::is_ascii_digit) {
            continue;
        }
        let mut digits = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            if let Some(digit) = chars.next() {
                digits.push(digit);
            }
        }
        if chars.next() != Some('}') {
            return Err(MudClientError::ConfigValidation(format!(
                "malformed capture reference `{{{digits}`"
            )));
        }
        let index = digits.parse::<usize>().map_err(|error| {
            MudClientError::ConfigValidation(format!(
                "invalid capture reference `{{{digits}}}`: {error}"
            ))
        })?;
        references.push(index);
    }
    Ok(references)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_and_substitutes_numeric_captures_only() {
        validate_capture_references("kill {1} with {name}", 1, "action").unwrap();
        assert_eq!(
            substitute_captures(
                "kill {1} with {name}",
                &["whole".to_string(), "orc".to_string()]
            ),
            "kill orc with {name}"
        );
        assert!(validate_capture_references("kill {2}", 1, "action").is_err());
        assert!(validate_capture_references("kill {1", 1, "action").is_err());
        assert_eq!(
            substitute_captures(
                "/variable {target} {{1}}",
                &["whole".to_string(), "orc".to_string()]
            ),
            "/variable {target} {orc}"
        );
    }
}
