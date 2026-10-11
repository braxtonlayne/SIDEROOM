//! Upload hygiene: size-limit policy, content sniffing, metadata stripping.

pub mod exif;
pub mod policy;
pub mod sniff;

pub use exif::{StripError, strip_metadata};
pub use policy::{SizeDecision, SizeError, SizePolicy};
pub use sniff::sniff_mime;
