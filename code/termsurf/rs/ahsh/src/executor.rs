use std::collections::HashMap;
use std::path::PathBuf;

/// Parse shell-agnostic `printenv` output plus __SHANNON_ markers.
///
/// Expected lines inside the sentinel region:
/// - `KEY=value` from `printenv` (split on the first `=`)
/// - `__SHANNON_CWD=...`
/// - `__SHANNON_EXIT=...` (ignored here; exit code is read by the caller)
pub fn parse_printenv_env(contents: &str) -> Option<(HashMap<String, String>, PathBuf)> {
    let mut env = HashMap::new();
    let mut cwd: Option<PathBuf> = None;

    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("__SHANNON_CWD=") {
            cwd = Some(PathBuf::from(rest));
            continue;
        }
        if line.starts_with("__SHANNON_EXIT=") {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if key.is_empty() || key.starts_with("__SHANNON_") {
                continue;
            }
            env.insert(key.to_string(), value.to_string());
        }
    }

    Some((env, cwd.unwrap_or_else(|| PathBuf::from("/"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_printenv_round_trips_simple_vars() {
        let sample = "\
FOO=bar
PATH=/usr/bin:/bin
__SHANNON_CWD=/tmp/work
__SHANNON_EXIT=0
";
        let (env, cwd) = parse_printenv_env(sample).unwrap();
        assert_eq!(env.get("FOO").unwrap(), "bar");
        assert_eq!(env.get("PATH").unwrap(), "/usr/bin:/bin");
        assert_eq!(cwd, PathBuf::from("/tmp/work"));
        assert!(!env.contains_key("__SHANNON_CWD"));
    }

    #[test]
    fn parse_printenv_value_may_contain_equals() {
        let sample = "OPTS=a=b=c\n__SHANNON_CWD=/\n";
        let (env, _) = parse_printenv_env(sample).unwrap();
        assert_eq!(env.get("OPTS").unwrap(), "a=b=c");
    }

    #[test]
    fn parse_printenv_ignores_declare_x_bash_format() {
        // Must not depend on bash export -p lines.
        let sample = "\
declare -x IGNORED=\"nope\"
REAL=yes
__SHANNON_CWD=/home
";
        let (env, cwd) = parse_printenv_env(sample).unwrap();
        // "declare -x IGNORED" is not a valid KEY=value env name from printenv.
        assert_eq!(env.get("REAL").unwrap(), "yes");
        assert_eq!(cwd, PathBuf::from("/home"));
    }
}
