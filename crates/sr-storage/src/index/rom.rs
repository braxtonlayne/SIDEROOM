//! ROMs on a mount (ACT-EMU-001, wave 37): which platform a file under `emu/roms/` is for, and which emulator core
//! plays it. Pure, like the rest of the indexer: the extension decides what the listing can tell, and
//! [`detect`] refines it with the file's first bytes the way the browser does (`apps/web/src/emu/detect.ts` mirrors
//! this module).
//!
//! The core names are the directory names under `/emu/cores/<name>/` (fetched at install by digest, wave 37 emu-ops).
//! Only the five cores the owner allowed are named here (FCEUmm, Beetle Supafaust, mGBA, BlastEm, PCSX-ReARMed);
//! whether a core is installed on this instance is the browser's question, not the index's.

/// The folder of a mount where ROMs live.
pub const ROM_PREFIX: &str = "emu/roms/";

/// A game platform the emulator supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    /// Nintendo Entertainment System (and Famicom Disk System).
    Nes,
    /// Super Nintendo.
    Snes,
    /// Game Boy.
    Gb,
    /// Game Boy Color.
    Gbc,
    /// Game Boy Advance.
    Gba,
    /// Sega Genesis / Mega Drive.
    Genesis,
    /// Sony PlayStation (needs a BIOS the member supplies).
    Ps1,
}

impl Platform {
    /// The stored / wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nes => "nes",
            Self::Snes => "snes",
            Self::Gb => "gb",
            Self::Gbc => "gbc",
            Self::Gba => "gba",
            Self::Genesis => "genesis",
            Self::Ps1 => "ps1",
        }
    }
    /// The core that plays the platform: a directory name under `/emu/cores/`.
    pub fn core(self) -> &'static str {
        match self {
            Self::Nes => "fceumm",
            Self::Snes => "supafaust",
            Self::Gb | Self::Gbc | Self::Gba => "mgba",
            Self::Genesis => "blastem",
            Self::Ps1 => "pcsx_rearmed",
        }
    }
    /// Whether the platform cannot start without a BIOS the member supplies.
    pub fn needs_bios(self) -> bool {
        matches!(self, Self::Ps1)
    }
}

/// The platform an extension (lower case, no dot) stands for. `bin` is ambiguous (a Genesis cartridge or a PS1 disc
/// track) and answers `None`: [`detect`] decides it from the header.
pub fn by_extension(ext: &str) -> Option<Platform> {
    Some(match ext {
        "nes" | "fds" | "unf" | "unif" => Platform::Nes,
        "sfc" | "smc" | "swc" | "fig" => Platform::Snes,
        "gb" => Platform::Gb,
        "gbc" => Platform::Gbc,
        "gba" => Platform::Gba,
        "md" | "gen" | "smd" => Platform::Genesis,
        "cue" | "iso" | "img" | "pbp" | "chd" | "m3u" | "ccd" | "mdf" | "toc" | "cbn" => {
            Platform::Ps1
        }
        _ => return None,
    })
}

/// Whether an extension may be a ROM at all (the indexer keeps only these under `emu/roms/`).
pub fn is_rom_ext(ext: &str) -> bool {
    ext == "bin" || by_extension(ext).is_some()
}

const GB_LOGO: [u8; 4] = [0xCE, 0xED, 0x66, 0x66];
const GBA_LOGO: [u8; 4] = [0x24, 0xFF, 0xAE, 0x51];
const CD_SYNC: [u8; 12] = [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0];

fn is_snes_header_at(b: &[u8], at: usize) -> bool {
    // The checksum complement (0x1C) and the checksum (0x1E) add up to 0xFFFF.
    let Some(h) = b.get(at..at + 0x20) else {
        return false;
    };
    let comp = u16::from(h[0x1C]) | (u16::from(h[0x1D]) << 8);
    let sum = u16::from(h[0x1E]) | (u16::from(h[0x1F]) << 8);
    comp.wrapping_add(sum) == 0xFFFF && (h[0x15] & 0x0F) <= 5
}

/// The platform of a file from its name and its first bytes (as many as the caller has; 64 KiB covers a SNES
/// header, 512 bytes everything else). The header wins over the extension.
pub fn detect(name: &str, head: &[u8]) -> Option<Platform> {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if head.starts_with(b"NES\x1A") || head.starts_with(b"UNIF") || head.starts_with(b"FDS\x1A") {
        return Some(Platform::Nes);
    }
    if head.get(4..8) == Some(&GBA_LOGO[..]) && head.get(0xB2) == Some(&0x96) {
        return Some(Platform::Gba);
    }
    if head.get(0x104..0x108) == Some(&GB_LOGO[..]) {
        let cgb = head.get(0x143).copied().unwrap_or(0);
        return Some(if cgb == 0x80 || cgb == 0xC0 {
            Platform::Gbc
        } else {
            Platform::Gb
        });
    }
    if head
        .get(0x100..0x108)
        .is_some_and(|w| w.windows(4).any(|x| x == b"SEGA"))
    {
        return Some(Platform::Genesis);
    }
    if head.starts_with(&CD_SYNC) || head.starts_with(b"\0PBP") {
        return Some(Platform::Ps1);
    }
    if let Some(p) = by_extension(&ext) {
        return Some(p);
    }
    if ext == "bin" {
        return None;
    }
    // A SNES image without a known extension: LoROM, HiROM, with or without the 512-byte copier header.
    let snes = [0x7FC0, 0xFFC0, 0x7FC0 + 512, 0xFFC0 + 512]
        .into_iter()
        .any(|at| is_snes_header_at(head, at));
    snes.then_some(Platform::Snes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_pick_the_platform_and_the_core() {
        assert_eq!(by_extension("nes"), Some(Platform::Nes));
        assert_eq!(Platform::Nes.core(), "fceumm");
        assert_eq!(Platform::Gbc.core(), "mgba");
        assert_eq!(by_extension("cue"), Some(Platform::Ps1));
        assert!(Platform::Ps1.needs_bios());
        assert!(!Platform::Nes.needs_bios());
        assert_eq!(by_extension("bin"), None);
        assert!(is_rom_ext("bin"));
        assert!(!is_rom_ext("txt"));
    }

    #[test]
    fn headers_beat_extensions() {
        let mut nes = b"NES\x1A".to_vec();
        nes.resize(64, 0);
        assert_eq!(detect("game.bin", &nes), Some(Platform::Nes));

        let mut gba = vec![0u8; 0xC0];
        gba[4..8].copy_from_slice(&GBA_LOGO);
        gba[0xB2] = 0x96;
        assert_eq!(detect("x.bin", &gba), Some(Platform::Gba));

        let mut gb = vec![0u8; 0x150];
        gb[0x104..0x108].copy_from_slice(&GB_LOGO);
        assert_eq!(detect("x.rom", &gb), Some(Platform::Gb));
        gb[0x143] = 0x80;
        assert_eq!(detect("x.rom", &gb), Some(Platform::Gbc));

        let mut md = vec![0u8; 0x200];
        md[0x100..0x104].copy_from_slice(b"SEGA");
        assert_eq!(detect("x.bin", &md), Some(Platform::Genesis));

        let mut cd = vec![0u8; 0x930];
        cd[..12].copy_from_slice(&CD_SYNC);
        assert_eq!(detect("disc.bin", &cd), Some(Platform::Ps1));
    }

    #[test]
    fn a_headerless_bin_is_unknown_and_snes_is_found_by_checksum() {
        assert_eq!(detect("mystery.bin", &[0u8; 64]), None);
        let mut s = vec![0u8; 0x8000];
        s[0x7FC0 + 0x1D] = 0xFF;
        s[0x7FC0 + 0x1E] = 0xFF;
        assert_eq!(detect("zelda.rom", &s), Some(Platform::Snes));
        assert_eq!(detect("zelda.rom", &[0u8; 0x8000]), None);
    }
}
