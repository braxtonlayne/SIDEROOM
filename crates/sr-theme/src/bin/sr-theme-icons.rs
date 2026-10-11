//! `sr-theme-icons <svg> <out-dir>`: renders the door mark to the installable-PWA and favicon
//! set (Q71): `icon-192.png`, `icon-512.png`, `maskable-512.png`, `apple-touch-180.png` and a
//! multi-size `favicon.ico`. Output is deterministic; commit the results.

use resvg::{tiny_skia, usvg};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

/// Background of the door mark (the SVG's own rect colour), used to fill the maskable canvas.
const MARK_BG: (u8, u8, u8) = (0x11, 0x11, 0x10);
/// Maskable icons keep the mark inside the central 80% (the platform safe zone).
const MASKABLE_SCALE: f32 = 0.8;
const ICO_SIZES: [u32; 3] = [16, 32, 48];

fn render(
    tree: &usvg::Tree,
    size: u32,
    scale: f32,
    fill: Option<(u8, u8, u8)>,
) -> Result<tiny_skia::Pixmap, String> {
    let mut pm = tiny_skia::Pixmap::new(size, size).ok_or("zero-sized canvas")?;
    if let Some((r, g, b)) = fill {
        pm.fill(tiny_skia::Color::from_rgba8(r, g, b, 0xff));
    }
    let view = tree.size();
    let k = size as f32 / view.width().max(view.height()) * scale;
    let offset = size as f32 * (1.0 - scale) / 2.0;
    let ts = tiny_skia::Transform::from_row(k, 0.0, 0.0, k, offset, offset);
    resvg::render(tree, ts, &mut pm.as_mut());
    Ok(pm)
}

fn png(
    tree: &usvg::Tree,
    size: u32,
    scale: f32,
    fill: Option<(u8, u8, u8)>,
) -> Result<Vec<u8>, String> {
    render(tree, size, scale, fill)?
        .encode_png()
        .map_err(|e| e.to_string())
}

/// ICO container with PNG-compressed entries (Windows Vista and later).
fn ico(entries: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * entries.len() as u32;
    for (size, data) in entries {
        // A dimension of 256 is stored as 0.
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in entries {
        out.extend_from_slice(data);
    }
    out
}

fn run(svg_path: &str, out_dir: &str) -> Result<(), String> {
    let source = fs::read_to_string(svg_path).map_err(|e| format!("{svg_path}: {e}"))?;
    let tree =
        usvg::Tree::from_str(&source, &usvg::Options::default()).map_err(|e| e.to_string())?;
    let out = PathBuf::from(out_dir);
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let write = |name: &str, bytes: Vec<u8>| {
        fs::write(out.join(name), bytes).map_err(|e| format!("{name}: {e}"))
    };

    write("icon-192.png", png(&tree, 192, 1.0, None)?)?;
    write("icon-512.png", png(&tree, 512, 1.0, None)?)?;
    write(
        "maskable-512.png",
        png(&tree, 512, MASKABLE_SCALE, Some(MARK_BG))?,
    )?;
    write("apple-touch-180.png", png(&tree, 180, 1.0, None)?)?;
    let mut entries = Vec::new();
    for size in ICO_SIZES {
        entries.push((size, png(&tree, size, 1.0, None)?));
    }
    write("favicon.ico", ico(&entries))?;
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, svg, out_dir] = args.as_slice() else {
        eprintln!("usage: sr-theme-icons <door-mark.svg> <out-dir>");
        return ExitCode::from(2);
    };
    match run(svg, out_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sr-theme-icons: {e}");
            ExitCode::FAILURE
        }
    }
}
