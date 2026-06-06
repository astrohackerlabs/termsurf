//! Locale probing helpers (Cocoa slice of upstream `os/locale`).

use std::ffi::{CStr, CString};

use crate::os::i18n;

#[cfg(target_os = "macos")]
use objc2_foundation::NSLocale;

/// Build a `LANG` environment value from macOS system locale preferences.
pub(crate) fn macos_lang_from_cocoa() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let locale = NSLocale::currentLocale();
        let language = locale.languageCode().to_string();
        #[allow(deprecated)]
        let country = locale.countryCode()?.to_string();
        lang_env_value(&language, &country)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Build a gettext `LANGUAGE` environment value from macOS preferred languages.
pub(crate) fn macos_language_from_cocoa() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let preferred = NSLocale::preferredLanguages();
        let values = (0..preferred.count()).map(|i| preferred.objectAtIndex(i).to_string());
        language_env_value(values)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Ensure the process C locale is initialized using upstream's recovery sequence.
pub(crate) fn ensure_locale() -> EnsureLocaleOutcome {
    let mut env = RealLocaleEnv;
    ensure_locale_with(
        &mut env,
        macos_lang_from_cocoa,
        macos_language_from_cocoa,
        real_setlocale,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnsureLocaleOutcome {
    FromEnvironment(String),
    SystemDefault(String),
    Fallback(String),
    Failed,
}

trait LocaleEnv {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str);
    fn unset(&mut self, key: &str);
}

struct RealLocaleEnv;

impl LocaleEnv for RealLocaleEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn set(&mut self, key: &str, value: &str) {
        std::env::set_var(key, value);
    }

    fn unset(&mut self, key: &str) {
        std::env::remove_var(key);
    }
}

fn ensure_locale_with<E, LangProbe, LanguageProbe, SetLocale>(
    env: &mut E,
    lang_probe: LangProbe,
    language_probe: LanguageProbe,
    mut setlocale: SetLocale,
) -> EnsureLocaleOutcome
where
    E: LocaleEnv,
    LangProbe: FnOnce() -> Option<String>,
    LanguageProbe: FnOnce() -> Option<String>,
    SetLocale: FnMut(Option<&str>) -> Option<String>,
{
    if env.get("LANG").as_deref().unwrap_or("").is_empty() {
        if let Some(lang) = lang_probe() {
            env.set("LANG", &lang);
            if let Some(language) = language_probe() {
                env.set("LANGUAGE", &language);
            }
        }
    }

    if let Some(value) = setlocale(Some("")) {
        return EnsureLocaleOutcome::FromEnvironment(value);
    }

    if !env.get("LANG").as_deref().unwrap_or("").is_empty() {
        env.set("LANG", "");
        env.unset("LANG");
        if let Some(value) = setlocale(Some("")) {
            if value != "C" {
                return EnsureLocaleOutcome::SystemDefault(value);
            }
        }
    }

    if let Some(value) = setlocale(Some("en_US.UTF-8")) {
        env.set("LANG", "en_US.UTF-8");
        return EnsureLocaleOutcome::Fallback(value);
    }

    EnsureLocaleOutcome::Failed
}

fn real_setlocale(locale: Option<&str>) -> Option<String> {
    let locale = match locale {
        Some(locale) => Some(CString::new(locale).ok()?),
        None => None,
    };
    let ptr = locale
        .as_ref()
        .map_or(std::ptr::null(), |locale| locale.as_ptr());

    let result = unsafe { libc::setlocale(libc::LC_ALL, ptr) };
    if result.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(result) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn lang_env_value(language: &str, country: &str) -> Option<String> {
    if language.is_empty() || country.is_empty() {
        None
    } else {
        Some(format!("{language}_{country}.UTF-8"))
    }
}

fn language_env_value<I, S>(values: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    i18n::gettext_language_list(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};

    #[test]
    fn lang_env_value_formats_language_and_country() {
        assert_eq!(lang_env_value("en", "US"), Some("en_US.UTF-8".to_owned()));
    }

    #[test]
    fn lang_env_value_rejects_empty_parts() {
        assert_eq!(lang_env_value("", "US"), None);
        assert_eq!(lang_env_value("en", ""), None);
        assert_eq!(lang_env_value("", ""), None);
    }

    #[test]
    fn language_env_value_canonicalizes_and_joins() {
        assert_eq!(
            language_env_value(["en-US", "", "zh-Hant-HK"]),
            Some("en_US.UTF-8:zh_HK.UTF-8".to_owned())
        );
    }

    #[test]
    fn language_env_value_rejects_empty_lists() {
        assert_eq!(language_env_value([] as [&str; 0]), None);
        assert_eq!(language_env_value(["", ""]), None);
    }

    #[derive(Default)]
    struct FakeLocaleEnv {
        values: HashMap<String, String>,
        log: Vec<String>,
    }

    impl FakeLocaleEnv {
        fn with_var(key: &str, value: &str) -> Self {
            let mut env = Self::default();
            env.values.insert(key.to_owned(), value.to_owned());
            env
        }
    }

    impl LocaleEnv for FakeLocaleEnv {
        fn get(&self, key: &str) -> Option<String> {
            self.values.get(key).cloned()
        }

        fn set(&mut self, key: &str, value: &str) {
            self.log.push(format!("set {key}={value}"));
            self.values.insert(key.to_owned(), value.to_owned());
        }

        fn unset(&mut self, key: &str) {
            self.log.push(format!("unset {key}"));
            self.values.remove(key);
        }
    }

    #[test]
    fn ensure_locale_prepopulates_lang_and_language_before_setlocale() {
        let mut env = FakeLocaleEnv::default();
        let mut calls = Vec::new();
        let outcome = ensure_locale_with(
            &mut env,
            || Some("en_US.UTF-8".to_owned()),
            || Some("en_US.UTF-8:fr.UTF-8".to_owned()),
            |locale| {
                calls.push(locale.map(str::to_owned));
                Some("en_US.UTF-8".to_owned())
            },
        );

        assert_eq!(
            outcome,
            EnsureLocaleOutcome::FromEnvironment("en_US.UTF-8".to_owned())
        );
        assert_eq!(
            env.values.get("LANG").map(String::as_str),
            Some("en_US.UTF-8")
        );
        assert_eq!(
            env.values.get("LANGUAGE").map(String::as_str),
            Some("en_US.UTF-8:fr.UTF-8")
        );
        assert_eq!(
            env.log,
            [
                "set LANG=en_US.UTF-8".to_owned(),
                "set LANGUAGE=en_US.UTF-8:fr.UTF-8".to_owned(),
            ]
        );
        assert_eq!(calls, [Some(String::new())]);
    }

    #[test]
    fn ensure_locale_does_not_probe_language_when_lang_probe_fails() {
        let mut env = FakeLocaleEnv::default();
        let mut language_probe_calls = 0;

        let outcome = ensure_locale_with(
            &mut env,
            || None,
            || {
                language_probe_calls += 1;
                Some("fr.UTF-8".to_owned())
            },
            |_| Some("C".to_owned()),
        );

        assert_eq!(
            outcome,
            EnsureLocaleOutcome::FromEnvironment("C".to_owned())
        );
        assert_eq!(language_probe_calls, 0);
        assert!(!env.values.contains_key("LANG"));
        assert!(!env.values.contains_key("LANGUAGE"));
        assert!(env.log.is_empty());
    }

    #[test]
    fn ensure_locale_skips_cocoa_probes_when_lang_exists() {
        let mut env = FakeLocaleEnv::with_var("LANG", "fr_FR.UTF-8");
        let mut lang_probe_calls = 0;
        let mut language_probe_calls = 0;

        let outcome = ensure_locale_with(
            &mut env,
            || {
                lang_probe_calls += 1;
                Some("en_US.UTF-8".to_owned())
            },
            || {
                language_probe_calls += 1;
                Some("en_US.UTF-8".to_owned())
            },
            |_| Some("fr_FR.UTF-8".to_owned()),
        );

        assert_eq!(
            outcome,
            EnsureLocaleOutcome::FromEnvironment("fr_FR.UTF-8".to_owned())
        );
        assert_eq!(lang_probe_calls, 0);
        assert_eq!(language_probe_calls, 0);
        assert_eq!(
            env.values.get("LANG").map(String::as_str),
            Some("fr_FR.UTF-8")
        );
        assert!(env.log.is_empty());
    }

    #[test]
    fn ensure_locale_recovers_invalid_lang_with_system_default() {
        let mut env = FakeLocaleEnv::with_var("LANG", "bad_locale");
        let mut responses = VecDeque::from([None, Some("fr_FR.UTF-8".to_owned())]);
        let mut calls = Vec::new();

        let outcome = ensure_locale_with(
            &mut env,
            || Some("en_US.UTF-8".to_owned()),
            || Some("en_US.UTF-8".to_owned()),
            |locale| {
                calls.push(locale.map(str::to_owned));
                responses.pop_front().flatten()
            },
        );

        assert_eq!(
            outcome,
            EnsureLocaleOutcome::SystemDefault("fr_FR.UTF-8".to_owned())
        );
        assert!(!env.values.contains_key("LANG"));
        assert_eq!(env.log, ["set LANG=".to_owned(), "unset LANG".to_owned()]);
        assert_eq!(calls, [Some(String::new()), Some(String::new())]);
    }

    #[test]
    fn ensure_locale_rejects_c_system_default_and_uses_fallback() {
        let mut env = FakeLocaleEnv::with_var("LANG", "bad_locale");
        let mut responses =
            VecDeque::from([None, Some("C".to_owned()), Some("en_US.UTF-8".to_owned())]);
        let mut calls = Vec::new();

        let outcome = ensure_locale_with(
            &mut env,
            || None,
            || None,
            |locale| {
                calls.push(locale.map(str::to_owned));
                responses.pop_front().flatten()
            },
        );

        assert_eq!(
            outcome,
            EnsureLocaleOutcome::Fallback("en_US.UTF-8".to_owned())
        );
        assert_eq!(
            env.values.get("LANG").map(String::as_str),
            Some("en_US.UTF-8")
        );
        assert_eq!(
            env.log,
            [
                "set LANG=".to_owned(),
                "unset LANG".to_owned(),
                "set LANG=en_US.UTF-8".to_owned(),
            ]
        );
        assert_eq!(
            calls,
            [
                Some(String::new()),
                Some(String::new()),
                Some("en_US.UTF-8".to_owned()),
            ]
        );
    }

    #[test]
    fn ensure_locale_total_failure_returns_failed() {
        let mut env = FakeLocaleEnv::with_var("LANG", "bad_locale");

        let outcome = ensure_locale_with(&mut env, || None, || None, |_| None);

        assert_eq!(outcome, EnsureLocaleOutcome::Failed);
        assert!(!env.values.contains_key("LANG"));
        assert_eq!(env.log, ["set LANG=".to_owned(), "unset LANG".to_owned()]);
    }

    #[test]
    fn real_setlocale_query_smoke_test() {
        let value = real_setlocale(None).expect("current locale query should succeed");
        assert!(!value.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cocoa_probes_smoke_when_values_are_available() {
        if let Some(lang) = macos_lang_from_cocoa() {
            assert!(!lang.is_empty());
            assert!(lang.contains('_'), "{lang}");
            assert!(lang.ends_with(".UTF-8"), "{lang}");
        }

        if let Some(language) = macos_language_from_cocoa() {
            assert!(!language.is_empty());
            assert!(language.contains(".UTF-8"), "{language}");
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn cocoa_probes_return_none_on_non_macos() {
        assert_eq!(macos_lang_from_cocoa(), None);
        assert_eq!(macos_language_from_cocoa(), None);
    }
}
