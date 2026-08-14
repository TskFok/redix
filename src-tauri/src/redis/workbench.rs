use crate::error::AppError;

pub fn tokenize_command(command: &str) -> Result<Vec<String>, AppError> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut has_content = false;

    for character in command.chars() {
        if escaped {
            current.push(character);
            has_content = true;
            escaped = false;
            continue;
        }

        if character == '\\' {
            escaped = true;
            has_content = true;
            continue;
        }

        if let Some(quote_character) = quote {
            if character == quote_character {
                quote = None;
            } else {
                current.push(character);
            }
            has_content = true;
            continue;
        }

        match character {
            '\'' | '"' => {
                quote = Some(character);
                has_content = true;
            }
            character if character.is_whitespace() => {
                if has_content {
                    arguments.push(std::mem::take(&mut current));
                    has_content = false;
                }
            }
            character => {
                current.push(character);
                has_content = true;
            }
        }
    }

    if escaped || quote.is_some() {
        return Err(AppError::CommandFailed);
    }

    if has_content {
        arguments.push(current);
    }

    if arguments.is_empty() {
        Err(AppError::CommandFailed)
    } else {
        Ok(arguments)
    }
}

#[cfg(test)]
mod tests {
    use super::tokenize_command;

    #[test]
    fn parses_whitespace_and_escaped_arguments() {
        let args = tokenize_command(r#"  SET  greeting hello\ world 'from redis'  "#).unwrap();

        assert_eq!(args, vec!["SET", "greeting", "hello world", "from redis"]);
    }

    #[test]
    fn parses_quoted_workbench_argument() {
        let args = tokenize_command("SET greeting \"hello world\"").unwrap();

        assert_eq!(args, vec!["SET", "greeting", "hello world"]);
    }

    #[test]
    fn rejects_unterminated_workbench_quote() {
        let error = tokenize_command("SET greeting \"hello").unwrap_err();

        assert_eq!(error.code(), "COMMAND_FAILED");
    }

    #[test]
    fn rejects_empty_or_dangling_command() {
        assert_eq!(
            tokenize_command("   ").unwrap_err().code(),
            "COMMAND_FAILED"
        );
        assert_eq!(
            tokenize_command("SET key \\").unwrap_err().code(),
            "COMMAND_FAILED"
        );
    }
}
