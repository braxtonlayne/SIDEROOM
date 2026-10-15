//! Profile catalogue (G-48, P11-SHL-3): the approved profile effects and avatar decorations
//! a person can pick. It is built in; assets live under `apps/web/public/profile/` with their
//! licence file. The four CSS effects ship today; avatar decorations need artwork the owner
//! supplies (or approves later as a layer-4 package), so that list is empty.

/// Catalogue of what profiles may use.
pub mod catalog {
    use serde::Serialize;

    /// What a catalogue entry decorates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Kind {
        /// A built-in animated layer behind the profile.
        Effect,
        /// An image drawn around the avatar.
        Decoration,
    }

    /// One approved entry.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
    pub struct Entry {
        pub slug: &'static str,
        pub kind: Kind,
        /// File stem under `/cdn/avatar-decorations/` for decorations; none for effects.
        pub asset: Option<&'static str>,
        /// Web i18n key of the display name.
        pub label_key: &'static str,
    }

    /// Every approved entry. Effects first; decorations are empty until artwork is supplied.
    pub const ENTRIES: &[Entry] = &[
        Entry {
            slug: "sparkle",
            kind: Kind::Effect,
            asset: None,
            label_key: "profile.effect.sparkle",
        },
        Entry {
            slug: "aurora",
            kind: Kind::Effect,
            asset: None,
            label_key: "profile.effect.aurora",
        },
        Entry {
            slug: "snow",
            kind: Kind::Effect,
            asset: None,
            label_key: "profile.effect.snow",
        },
        Entry {
            slug: "embers",
            kind: Kind::Effect,
            asset: None,
            label_key: "profile.effect.embers",
        },
    ];

    /// The entries of one kind.
    pub fn of_kind(kind: Kind) -> impl Iterator<Item = &'static Entry> {
        ENTRIES.iter().filter(move |e| e.kind == kind)
    }

    /// Is `slug` an approved profile effect?
    pub fn is_effect(slug: &str) -> bool {
        of_kind(Kind::Effect).any(|e| e.slug == slug)
    }

    /// Is `slug` an approved avatar decoration?
    pub fn is_decoration(slug: &str) -> bool {
        of_kind(Kind::Decoration).any(|e| e.slug == slug)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn effects_only_until_artwork_exists() {
            assert_eq!(of_kind(Kind::Effect).count(), 4);
            assert_eq!(of_kind(Kind::Decoration).count(), 0);
            assert!(is_effect("aurora"));
            assert!(!is_effect("neon-halo"));
            assert!(!is_decoration("neon-halo"));
        }

        #[test]
        fn slugs_are_unique_and_url_safe() {
            let mut seen = std::collections::HashSet::new();
            for e in ENTRIES {
                assert!(seen.insert(e.slug));
                assert!(e.slug.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'));
            }
        }
    }
}
