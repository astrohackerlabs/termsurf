//! Tests drive the **shipped** nu-cli merge helpers (Issue 26072213251282 Exp 2).

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use nu_cli::{merge_alt_shell_bootstrap_env, union_path_nu_first};

    #[test]
    fn real_helper_path_nu_first_order() {
        let got = union_path_nu_first(
            vec!["n1".into(), "n2".into()],
            vec!["a".into(), "b".into()],
        );
        assert_eq!(got, vec!["n1", "n2", "a", "b"]);
    }

    #[test]
    fn real_helper_path_dedupe_keeps_nu_first() {
        let got = union_path_nu_first(
            vec!["/nu/bin".into()],
            vec!["/nu/bin".into(), "/zsh/bin".into()],
        );
        assert_eq!(got, vec!["/nu/bin", "/zsh/bin"]);
    }

    #[test]
    fn real_helper_scalar_nu_wins_zsh_fills() {
        let mut existing = HashMap::new();
        existing.insert("FOO".into(), "nu".into());
        existing.insert("PATH".into(), "/nu/bin".into());
        let mut zsh = HashMap::new();
        zsh.insert("FOO".into(), "zsh".into());
        zsh.insert("BAR".into(), "zsh".into());
        zsh.insert("PATH".into(), "/zsh/bin".into());
        let out = merge_alt_shell_bootstrap_env(&existing, &zsh);
        assert!(!out.contains_key("FOO"));
        assert_eq!(out.get("BAR").map(String::as_str), Some("zsh"));
        let path_key = if cfg!(windows) { "Path" } else { "PATH" };
        let path = out.get(path_key).expect("PATH written");
        let parts: Vec<String> = std::env::split_paths(path)
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        let i_nu = parts.iter().position(|p| p.contains("nu/bin")).unwrap();
        let i_zsh = parts.iter().position(|p| p.contains("zsh/bin")).unwrap();
        assert!(i_nu < i_zsh, "expected Nu before zsh in {parts:?}");
    }
}
