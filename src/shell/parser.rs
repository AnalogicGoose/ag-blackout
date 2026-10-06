use std::collections::HashMap;
use std::iter::Peekable;
use std::str::Chars;

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("unterminated quote")]
    UnterminatedQuote,
    #[error("syntax error near unexpected token `|'")]
    EmptyPipelineStage,
    #[error("syntax error: expected a filename after redirection")]
    MissingRedirectTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectKind {
    Out,
    Append,
    In,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirection {
    pub kind: RedirectKind,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedCommand {
    pub argv: Vec<String>,
    pub redirections: Vec<Redirection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pipeline {
    pub stages: Vec<ParsedCommand>,
}

enum RawToken {
    Word(String),
    Pipe,
    Redirect(RedirectKind),
}

/// Parses one input line into a pipeline of commands, expanding `$VAR`/`${VAR}`
/// against `env` (skipped inside single quotes, applied inside double quotes
/// and bare words — same rule real shells use), plus a leading `~` (only as
/// the first character of a word, followed by `/`/whitespace/end-of-input) to
/// `env["HOME"]`. No globbing, no backslash escaping, no `;`/`&&`/`||`
/// chaining — not needed yet, keeps this small.
pub fn parse(input: &str, env: &HashMap<String, String>) -> Result<Pipeline, ParseError> {
    let tokens = tokenize(input, env)?;
    group_into_pipeline(tokens)
}

fn tokenize(input: &str, env: &HashMap<String, String>) -> Result<Vec<RawToken>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();
    let mut in_token = false;

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if in_token {
                    tokens.push(RawToken::Word(std::mem::take(&mut current)));
                    in_token = false;
                }
            }
            '|' => {
                flush(&mut tokens, &mut current, &mut in_token);
                tokens.push(RawToken::Pipe);
            }
            '>' => {
                flush(&mut tokens, &mut current, &mut in_token);
                if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(RawToken::Redirect(RedirectKind::Append));
                } else {
                    tokens.push(RawToken::Redirect(RedirectKind::Out));
                }
            }
            '<' => {
                flush(&mut tokens, &mut current, &mut in_token);
                tokens.push(RawToken::Redirect(RedirectKind::In));
            }
            '\'' => {
                in_token = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => current.push(ch),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
            }
            '"' => {
                in_token = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('$') => expand_var(&mut chars, &mut current, env),
                        Some(ch) => current.push(ch),
                        None => return Err(ParseError::UnterminatedQuote),
                    }
                }
            }
            '$' => {
                in_token = true;
                expand_var(&mut chars, &mut current, env);
            }
            '~' if !in_token => {
                in_token = true;
                match chars.peek() {
                    None | Some('/') | Some(' ') | Some('\t') => {
                        if let Some(home) = env.get("HOME") {
                            current.push_str(home);
                        } else {
                            current.push('~');
                        }
                    }
                    _ => current.push('~'),
                }
            }
            other => {
                in_token = true;
                current.push(other);
            }
        }
    }
    if in_token {
        tokens.push(RawToken::Word(current));
    }
    Ok(tokens)
}

fn flush(tokens: &mut Vec<RawToken>, current: &mut String, in_token: &mut bool) {
    if *in_token {
        tokens.push(RawToken::Word(std::mem::take(current)));
        *in_token = false;
    }
}

fn expand_var(chars: &mut Peekable<Chars>, out: &mut String, env: &HashMap<String, String>) {
    let mut name = String::new();
    if chars.peek() == Some(&'{') {
        chars.next();
        for ch in chars.by_ref() {
            if ch == '}' {
                break;
            }
            name.push(ch);
        }
    } else {
        while let Some(&ch) = chars.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                name.push(ch);
                chars.next();
            } else {
                break;
            }
        }
    }
    if let Some(value) = env.get(&name) {
        out.push_str(value);
    }
}

fn group_into_pipeline(tokens: Vec<RawToken>) -> Result<Pipeline, ParseError> {
    let mut stages = Vec::new();
    let mut current = ParsedCommand::default();
    let mut pending_redirect: Option<RedirectKind> = None;
    let mut has_word = false;

    for token in tokens {
        match token {
            RawToken::Word(w) => {
                if let Some(kind) = pending_redirect.take() {
                    current.redirections.push(Redirection { kind, target: w });
                } else {
                    current.argv.push(w);
                    has_word = true;
                }
            }
            RawToken::Redirect(kind) => pending_redirect = Some(kind),
            RawToken::Pipe => {
                if pending_redirect.is_some() {
                    return Err(ParseError::MissingRedirectTarget);
                }
                if !has_word {
                    return Err(ParseError::EmptyPipelineStage);
                }
                stages.push(std::mem::take(&mut current));
                has_word = false;
            }
        }
    }

    if pending_redirect.is_some() {
        return Err(ParseError::MissingRedirectTarget);
    }
    if has_word {
        stages.push(current);
    } else if !stages.is_empty() {
        return Err(ParseError::EmptyPipelineStage);
    }

    Ok(Pipeline { stages })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> HashMap<String, String> {
        let mut e = HashMap::new();
        e.insert("HOME".to_string(), "/home/guest".to_string());
        e
    }

    #[test]
    fn tokenizes_simple_command() {
        let p = parse("ls -la /etc", &env()).unwrap();
        assert_eq!(p.stages.len(), 1);
        assert_eq!(p.stages[0].argv, vec!["ls", "-la", "/etc"]);
    }

    #[test]
    fn single_and_double_quotes_group_spaces_into_one_word() {
        let p = parse(r#"echo 'a b' "c d""#, &env()).unwrap();
        assert_eq!(p.stages[0].argv, vec!["echo", "a b", "c d"]);
    }

    #[test]
    fn expands_vars_unquoted_and_in_double_quotes_not_single() {
        let p = parse(r#"echo $HOME "$HOME" '$HOME'"#, &env()).unwrap();
        assert_eq!(
            p.stages[0].argv,
            vec!["echo", "/home/guest", "/home/guest", "$HOME"]
        );
    }

    #[test]
    fn braced_var_expansion() {
        let p = parse("echo ${HOME}x", &env()).unwrap();
        assert_eq!(p.stages[0].argv, vec!["echo", "/home/guestx"]);
    }

    #[test]
    fn unknown_var_expands_to_empty() {
        let p = parse("echo $NOPE", &env()).unwrap();
        assert_eq!(p.stages[0].argv, vec!["echo", ""]);
    }

    #[test]
    fn tilde_expands_to_home_as_the_first_char_of_a_word() {
        let p = parse("cd ~", &env()).unwrap();
        assert_eq!(p.stages[0].argv, vec!["cd", "/home/guest"]);

        let p = parse("cd ~/docs", &env()).unwrap();
        assert_eq!(p.stages[0].argv, vec!["cd", "/home/guest/docs"]);
    }

    #[test]
    fn tilde_mid_word_or_for_another_user_is_left_literal() {
        let p = parse("echo foo~bar ~guest", &env()).unwrap();
        assert_eq!(p.stages[0].argv, vec!["echo", "foo~bar", "~guest"]);
    }

    #[test]
    fn parses_pipeline_into_stages() {
        let p = parse("cat file | grep foo | wc -l", &env()).unwrap();
        assert_eq!(p.stages.len(), 3);
        assert_eq!(p.stages[1].argv, vec!["grep", "foo"]);
    }

    #[test]
    fn parses_output_append_and_input_redirection() {
        let p = parse("cmd > out.txt", &env()).unwrap();
        assert_eq!(
            p.stages[0].redirections,
            vec![Redirection {
                kind: RedirectKind::Out,
                target: "out.txt".to_string()
            }]
        );

        let p = parse("cmd >> out.txt", &env()).unwrap();
        assert_eq!(p.stages[0].redirections[0].kind, RedirectKind::Append);

        let p = parse("cmd < in.txt", &env()).unwrap();
        assert_eq!(p.stages[0].redirections[0].kind, RedirectKind::In);
    }

    #[test]
    fn empty_input_yields_no_stages() {
        assert_eq!(parse("", &env()).unwrap().stages.len(), 0);
        assert_eq!(parse("   ", &env()).unwrap().stages.len(), 0);
    }

    #[test]
    fn trailing_pipe_is_a_parse_error() {
        assert_eq!(
            parse("ls |", &env()).unwrap_err(),
            ParseError::EmptyPipelineStage
        );
    }

    #[test]
    fn leading_pipe_is_a_parse_error() {
        assert_eq!(
            parse("| ls", &env()).unwrap_err(),
            ParseError::EmptyPipelineStage
        );
    }

    #[test]
    fn redirection_without_target_is_a_parse_error() {
        assert_eq!(
            parse("cat >", &env()).unwrap_err(),
            ParseError::MissingRedirectTarget
        );
    }

    #[test]
    fn unterminated_quote_is_a_parse_error() {
        assert_eq!(
            parse("echo \"unfinished", &env()).unwrap_err(),
            ParseError::UnterminatedQuote
        );
    }
}
