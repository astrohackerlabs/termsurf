//! Mach VM helpers (port of upstream `os/mach`).

/// macOS virtual-memory tags for use with `mmap` / `mach_vm_*` (upstream `os.mach.VMTag`).
/// These identify memory regions in tools like `vmmap` and Instruments. Only the
/// application-specific tags (`240`–`255`) are named — the only ones realistically set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum VMTag {
    ApplicationSpecific1 = 240,
    ApplicationSpecific2 = 241,
    ApplicationSpecific3 = 242,
    ApplicationSpecific4 = 243,
    ApplicationSpecific5 = 244,
    ApplicationSpecific6 = 245,
    ApplicationSpecific7 = 246,
    ApplicationSpecific8 = 247,
    ApplicationSpecific9 = 248,
    ApplicationSpecific10 = 249,
    ApplicationSpecific11 = 250,
    ApplicationSpecific12 = 251,
    ApplicationSpecific13 = 252,
    ApplicationSpecific14 = 253,
    ApplicationSpecific15 = 254,
    ApplicationSpecific16 = 255,
}

impl VMTag {
    /// The tag in the format `mmap` / `mach_vm_*` expects — the tag byte shifted left 24 bits,
    /// reinterpreted as a signed `i32` (the C macro `VM_MAKE_TAG(tag)`; upstream `make`).
    pub(crate) fn make(self) -> i32 {
        ((self as u32) << 24) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [(VMTag, u8); 16] = [
        (VMTag::ApplicationSpecific1, 240),
        (VMTag::ApplicationSpecific2, 241),
        (VMTag::ApplicationSpecific3, 242),
        (VMTag::ApplicationSpecific4, 243),
        (VMTag::ApplicationSpecific5, 244),
        (VMTag::ApplicationSpecific6, 245),
        (VMTag::ApplicationSpecific7, 246),
        (VMTag::ApplicationSpecific8, 247),
        (VMTag::ApplicationSpecific9, 248),
        (VMTag::ApplicationSpecific10, 249),
        (VMTag::ApplicationSpecific11, 250),
        (VMTag::ApplicationSpecific12, 251),
        (VMTag::ApplicationSpecific13, 252),
        (VMTag::ApplicationSpecific14, 253),
        (VMTag::ApplicationSpecific15, 254),
        (VMTag::ApplicationSpecific16, 255),
    ];

    #[test]
    fn discriminants_match_upstream() {
        for (tag, value) in ALL {
            assert_eq!(tag as u8, value);
        }
    }

    #[test]
    fn make_is_vm_make_tag() {
        for (tag, value) in ALL {
            assert_eq!(tag.make(), ((value as u32) << 24) as i32);
        }
        // Spot-check the boundary tags.
        assert_eq!(VMTag::ApplicationSpecific1.make(), -268435456); // 0xF0000000 as i32
        assert_eq!(VMTag::ApplicationSpecific16.make(), ((255u32 << 24) as i32),);
    }
}
