//! `wasm-pack test --node crates/sr-tools-wasm`: the GIF builder through its JavaScript-facing API, in a real
//! wasm runtime (UTL-IMG-001). A 10-frame generated sequence decodes with the `gif` crate to the same frames.
#![cfg(target_arch = "wasm32")]

use sr_tools_wasm::gif::GifBuilder;
use wasm_bindgen_test::wasm_bindgen_test;

fn frame(i: u32) -> Vec<u8> {
    (0..16 * 8u32)
        .flat_map(|p| {
            let (x, y) = (p % 16, p / 16);
            let on = x >= i && x < i + 3 && y > 1 && y < 5;
            if on {
                [240, 40 + (i * 12) as u8, 10, 255]
            } else {
                [5, 10, 40, 255]
            }
        })
        .collect()
}

#[wasm_bindgen_test]
fn ten_generated_frames_decode_to_the_same_frames() {
    let mut b = GifBuilder::new(16, 8, 6).ok().expect("builder");
    for i in 0..10 {
        b.add_frame(&frame(i)).ok().expect("frame");
    }
    assert_eq!(b.frame_count(), 10);
    let bytes = b.finish().ok().expect("finish");

    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(&bytes[..]).unwrap();
    let mut n = 0;
    while let Some(f) = dec.read_next_frame().unwrap() {
        assert_eq!(f.buffer.to_vec(), frame(n), "frame {n}");
        assert_eq!(f.delay, 6);
        n += 1;
    }
    assert_eq!(n, 10);
}

#[wasm_bindgen_test]
fn the_fifteen_second_cap_refuses_the_151st_frame() {
    // 15 s at 10 cs per frame is 150 frames; the 151st is refused.
    let mut b = GifBuilder::new(2, 2, 10).ok().expect("builder");
    for _ in 0..150 {
        b.add_frame(&[1u8; 16]).ok().expect("frame");
    }
    assert!(b.add_frame(&[1u8; 16]).is_err());
}
