//! Home-directory path expansion (port of upstream `os/homedir`).

use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

/// Expand a leading `~/` in `path` to `home_dir` (upstream `os.homedir.expandHomeUnix`,
/// parameterized over the resolved home directory). A `path` that does not begin with `~/`
/// is returned unchanged.
pub(crate) fn expand_home<'a>(path: &'a OsStr, home_dir: &OsStr) -> Cow<'a, OsStr> {
    let bytes = path.as_bytes();
    if !bytes.starts_with(b"~/") {
        return Cow::Borrowed(path);
    }

    // Skip the '~', keeping the '/...'.
    let rest = &bytes[1..];
    let mut expanded = OsString::with_capacity(home_dir.len() + rest.len());
    expanded.push(home_dir);
    expanded.push(OsStr::from_bytes(rest));
    Cow::Owned(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> &OsStr {
        OsStr::new(s)
    }

    #[test]
    fn expands_tilde_slash_prefix() {
        let home = os("/home/user");
        // "~/" joins to "/home/user" + "/", so it ends with the separator.
        assert_eq!(
            expand_home(os("~/"), home),
            Cow::<OsStr>::Owned(os("/home/user/").into())
        );
        assert_eq!(
            expand_home(os("~/Downloads/shader.glsl"), home),
            Cow::<OsStr>::Owned(os("/home/user/Downloads/shader.glsl").into()),
        );
    }

    #[test]
    fn leaves_non_tilde_slash_paths_unchanged() {
        let home = os("/home/user");
        for input in ["~", "~abc/", "/home/user", ""] {
            let result = expand_home(os(input), home);
            assert_eq!(result, Cow::Borrowed(os(input)));
            // The unchanged case borrows the input rather than allocating.
            assert!(matches!(result, Cow::Borrowed(_)));
        }
    }

    #[test]
    fn expanded_case_is_owned() {
        let home = os("/home/user");
        assert!(matches!(expand_home(os("~/x"), home), Cow::Owned(_)));
    }

    #[test]
    fn preserves_non_utf8_home_bytes() {
        let home = OsStr::from_bytes(b"/h\xff");
        let result = expand_home(os("~/x"), home);
        assert_eq!(result.as_bytes(), b"/h\xff/x");
    }
}
