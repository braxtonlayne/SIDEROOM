//! Browser bindings for the Invite Graph layout (ADM-GRAPH-002).
//!
//! The graph travels as typed arrays so nothing is parsed: `edges` is a `Uint32Array` of node
//! index pairs `[a0, b0, a1, b1, ...]` and the positions come back as a `Float32Array`
//! `[x0, y0, x1, y1, ...]`, ready to upload as a vertex buffer. The algorithm lives in
//! [`layout`] and is tested natively.

use wasm_bindgen::prelude::*;

pub mod layout;

pub use layout::{LayoutError, Options};

/// Defaults of [`layout_graph`], for a UI that shows them: `[iterations, seed, theta,
/// spring_length, gravity]`.
#[wasm_bindgen(js_name = layoutDefaults)]
pub fn layout_defaults() -> Vec<f32> {
    let o = Options::default();
    vec![
        o.iterations as f32,
        o.seed as f32,
        o.theta,
        o.spring_length,
        o.gravity,
    ]
}

/// Lay out `node_count` nodes joined by `edges`. A value of 0 for `iterations`, or a
/// non-positive `spring_length`, means the default. Returns `2 * node_count` floats centred on
/// the origin, or throws when an edge points at a node that does not exist.
#[wasm_bindgen(js_name = layoutGraph)]
pub fn layout_graph(
    node_count: u32,
    edges: &[u32],
    iterations: u32,
    seed: u32,
    spring_length: f32,
) -> Result<Vec<f32>, JsError> {
    let d = Options::default();
    let opts = Options {
        iterations: if iterations == 0 {
            d.iterations
        } else {
            iterations
        },
        seed,
        spring_length: if spring_length > 0.0 {
            spring_length
        } else {
            d.spring_length
        },
        ..d
    };
    layout::layout(node_count as usize, edges, opts).map_err(|e| JsError::new(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_exposed_and_zero_means_default() {
        let d = layout_defaults();
        assert_eq!(d.len(), 5);
        assert_eq!(d[0], 300.0);
        // The wrapper's zero arguments behave like the defaults.
        let edges = [0, 1, 1, 2];
        let a = layout::layout(3, &edges, Options::default()).unwrap();
        let b = layout::layout(
            3,
            &edges,
            Options {
                iterations: 300,
                seed: 0,
                ..Options::default()
            },
        )
        .unwrap();
        // Seed 0 and seed 1 start identically (the generator never uses 0), so the picture is the same.
        assert_eq!(a, b);
    }
}
