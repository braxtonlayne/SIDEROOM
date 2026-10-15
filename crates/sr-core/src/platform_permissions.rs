//! Platform permission bits (FND-CORE-004), doc 07 section 3.
//! Stored on Staff Server roles; only meaningful for active staff appointments.

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct PlatformPermissions: u64 {
        const STAFF_BASIC = 1 << 0;
        const REVIEW_MEDIA = 1 << 1;
        const REVIEW_READING = 1 << 2;
        const PUBLISH_MEDIA = 1 << 3;
        const PUBLISH_READING = 1 << 4;
        const DELETE_CONTENT = 1 << 5;
        const MANAGE_COLLECTIONS = 1 << 6;
        const MANAGE_TV = 1 << 7;
        const REVIEW_APPS = 1 << 8;
        const REVIEW_PACKAGES = 1 << 9;
        const MANAGE_DIRECTORY = 1 << 10;
        const MANAGE_ACCOUNTS = 1 << 11;
        const PLATFORM_BAN = 1 << 12;
        const MANAGE_INVITES = 1 << 13;
        const MANAGE_DONATIONS = 1 << 14;
        const MANAGE_FEATURES = 1 << 15;
        /// High-level only; every access is logged. Only the Owner can grant it.
        const READ_PRIVATE_CONTENT = 1 << 16;
        const MANAGE_GUILDS_GLOBAL = 1 << 17;
        const VIEW_ACCESS_LOG = 1 << 18;
        const APPOINT_STAFF = 1 << 19;
        const MANAGE_TRANSCODE = 1 << 20;
        const OPERATOR = 1 << 21;
        /// Moderation cases, holds and appeals (D-210, D-213).
        const MANAGE_CASES = 1 << 22;
        /// Takedown notices and their records (D-210).
        const MANAGE_TAKEDOWNS = 1 << 23;
        /// Implicit for the Staff Server owner. Everything.
        const OWNER = 1 << 63;
    }
}

impl PlatformPermissions {
    /// True if `self` holds `perm`; `OWNER` implies everything.
    pub fn has(self, perm: Self) -> bool {
        self.contains(Self::OWNER) || self.contains(perm)
    }

    /// High-level platform staff (D-125): holds `READ_PRIVATE_CONTENT`.
    pub fn is_high_level(self) -> bool {
        self.has(Self::READ_PRIVATE_CONTENT)
    }

    /// Union of the platform permissions of a staff member's roles.
    pub fn from_roles(roles: impl IntoIterator<Item = Self>) -> Self {
        roles.into_iter().fold(Self::empty(), |a, b| a | b)
    }
}

impl serde::Serialize for PlatformPermissions {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(&self.bits())
    }
}

impl<'de> serde::Deserialize<'de> for PlatformPermissions {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let bits: u64 = s.parse().map_err(serde::de::Error::custom)?;
        Ok(Self::from_bits_truncate(bits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_implies_all() {
        assert!(PlatformPermissions::OWNER.has(PlatformPermissions::OPERATOR));
        assert!(!PlatformPermissions::STAFF_BASIC.has(PlatformPermissions::OPERATOR));
    }

    #[test]
    fn high_level() {
        assert!(PlatformPermissions::READ_PRIVATE_CONTENT.is_high_level());
        assert!(!PlatformPermissions::MANAGE_TV.is_high_level());
    }

    #[test]
    fn serde_string() {
        let p = PlatformPermissions::STAFF_BASIC | PlatformPermissions::REVIEW_MEDIA;
        assert_eq!(serde_json::to_string(&p).unwrap(), "\"3\"");
        assert_eq!(
            serde_json::from_str::<PlatformPermissions>("\"3\"").unwrap(),
            p
        );
    }
}
