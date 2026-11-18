use regex::Regex;

pub const DEFAULT_ARGS: &str = "-epicapp=Fortnite -skippatchcheck -epicportal -HTTP=WinINet -log";

/// Splits a command-line string into arguments, honouring "double quotes".
pub fn split_args(text: &str) -> Vec<String> {
    let re = Regex::new(r#""([^"]*)"|(\S+)"#).unwrap();
    re.captures_iter(text)
        .map(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        })
        .collect()
}

/// Final argument list handed to the game executable (never goes through a shell).
pub fn build_launch_args(extra_args: &str, email: &str, password: &str) -> Vec<String> {
    let auth_re = Regex::new(r"(?i)^-AUTH_(LOGIN|PASSWORD|TYPE)=").unwrap();
    let mut args: Vec<String> = split_args(extra_args)
        .into_iter()
        .filter(|a| !auth_re.is_match(a))
        .collect();
    args.push(format!("-AUTH_LOGIN={}", email));
    args.push(format!("-AUTH_PASSWORD={}", password));
    args.push("-AUTH_TYPE=epic".to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::{build_launch_args, split_args};

    #[test]
    fn quoted_arguments_remain_single_arguments() {
        assert_eq!(split_args(r#"-log -windowed "hello world""#), vec![
            "-log".to_string(), "-windowed".to_string(), "hello world".to_string()
        ]);
    }

    #[test]
    fn launch_arguments_replace_existing_auth_values() {
        let args = build_launch_args("-epicportal -AUTH_LOGIN=old -AUTH_PASSWORD=old", "user@example.test", "secret");
        assert_eq!(args.iter().filter(|a| a.starts_with("-AUTH_LOGIN=")).count(), 1);
        assert_eq!(args.iter().filter(|a| a.starts_with("-AUTH_PASSWORD=")).count(), 1);
        assert!(args.contains(&"-AUTH_LOGIN=user@example.test".to_string()));
        assert!(args.contains(&"-AUTH_PASSWORD=secret".to_string()));
    }
}
