//! Conditional configuration state and predicates (port of upstream `config/conditional`).
//!
//! Conditionals test a static, typed snapshot of the world (`State`) so the implementation stays
//! simple and type-checked.

/// The OS desktop theme (upstream `State.Theme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Theme {
    Light,
    Dark,
}

impl Theme {
    /// The tag name compared against a conditional's value (upstream `@tagName`).
    fn name(self) -> &'static [u8] {
        match self {
            Theme::Light => b"light",
            Theme::Dark => b"dark",
        }
    }
}

/// The build-target OS (upstream `std.Target.Os.Tag`). roastty is macOS-only, so the only build
/// target is `macos`; a conditional comparing `os` against another OS name simply does not match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OsTag {
    Macos,
}

impl OsTag {
    fn name(self) -> &'static [u8] {
        match self {
            OsTag::Macos => b"macos",
        }
    }
}

/// A static, typed snapshot of the world a conditional tests against (upstream `State`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) theme: Theme,
    pub(crate) os: OsTag,
}

impl Default for State {
    fn default() -> Self {
        // Upstream: theme defaults to light, os to the build target (macos here).
        State {
            theme: Theme::Light,
            os: OsTag::Macos,
        }
    }
}

impl State {
    /// Test a conditional against this state (upstream `match`). Compares the named state field's
    /// tag name to the conditional's value.
    pub(crate) fn matches(&self, cond: &Conditional) -> bool {
        let value: &[u8] = match cond.key {
            Key::Theme => self.theme.name(),
            Key::Os => self.os.name(),
        };
        match cond.op {
            Op::Eq => value == cond.value.as_slice(),
            Op::Ne => value != cond.value.as_slice(),
        }
    }
}

/// Which state field a conditional tests (upstream `Key`, derived from `State`'s fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Theme,
    Os,
}

/// The comparison a conditional applies (upstream `Conditional.Op`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    Eq,
    Ne,
}

/// A single conditional predicate (upstream `Conditional`). `clone` is the derived `Clone`, which
/// duplicates `value` exactly as upstream's `alloc.dupe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Conditional {
    pub(crate) key: Key,
    pub(crate) op: Op,
    pub(crate) value: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditional_enum_match() {
        let state = State {
            theme: Theme::Dark,
            ..State::default()
        };
        assert!(state.matches(&Conditional {
            key: Key::Theme,
            op: Op::Eq,
            value: b"dark".to_vec(),
        }));
        assert!(!state.matches(&Conditional {
            key: Key::Theme,
            op: Op::Ne,
            value: b"dark".to_vec(),
        }));
        assert!(state.matches(&Conditional {
            key: Key::Theme,
            op: Op::Ne,
            value: b"light".to_vec(),
        }));
    }

    #[test]
    fn default_state_is_light_macos() {
        let state = State::default();
        assert_eq!(state.theme, Theme::Light);
        assert_eq!(state.os, OsTag::Macos);
    }

    #[test]
    fn os_matching_resolves_to_macos() {
        let state = State::default();
        assert!(state.matches(&Conditional {
            key: Key::Os,
            op: Op::Eq,
            value: b"macos".to_vec(),
        }));
        // Comparing against a different OS name does not match on a macOS build.
        assert!(!state.matches(&Conditional {
            key: Key::Os,
            op: Op::Eq,
            value: b"linux".to_vec(),
        }));
        assert!(state.matches(&Conditional {
            key: Key::Os,
            op: Op::Ne,
            value: b"linux".to_vec(),
        }));
    }

    #[test]
    fn conditional_clone_duplicates_value() {
        let cond = Conditional {
            key: Key::Theme,
            op: Op::Eq,
            value: b"dark".to_vec(),
        };
        let cloned = cond.clone();
        assert_eq!(cond, cloned);
        assert_eq!(cloned.value, b"dark");
    }
}
