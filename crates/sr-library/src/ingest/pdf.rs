//! PDF intake (LIB-ING-004, first half): check that an upload is a PDF worth keeping.
//!
//! The original is stored as uploaded for download. Rasterizing pages for image-mode
//! reading is **not** implemented: ADR-0005 names `pdfium-render`, which needs a native
//! pdfium library shipped beside the server, and no pure-Rust rasterizer is practical. See
//! `docs/open-questions.md` (PDF page rasterization).
//!
//! This reads only the first KiB and the last 64 KiB, never parses object streams and
//! never renders anything, so a hostile PDF costs a few reads. It is a sanity check, not
//! a validator: it refuses files that are too big, are not PDFs, are encrypted (the
//! reader could not open them) or are visibly truncated.

use std::io::{Read, Seek, SeekFrom};

use super::IngestError;

/// Largest PDF kept (matches the per-archive default).
pub const MAX_PDF_BYTES: u64 = 512 * 1024 * 1024;
const HEAD: u64 = 1024;
const TAIL: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfInfo {
    /// `"1.7"`, `"2.0"`, ... from the header.
    pub version: String,
    pub size: u64,
    pub warnings: Vec<String>,
}

fn io(e: std::io::Error) -> IngestError {
    IngestError::Archive(e.to_string())
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Accept or refuse a PDF upload.
pub fn validate<R: Read + Seek>(mut r: R, max_bytes: u64) -> Result<PdfInfo, IngestError> {
    let size = r.seek(SeekFrom::End(0)).map_err(io)?;
    if size > max_bytes {
        return Err(IngestError::TooLarge("PDF file"));
    }
    r.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut head = Vec::new();
    (&mut r).take(HEAD).read_to_end(&mut head).map_err(io)?;
    // The header may follow a few bytes of junk, but must be near the start.
    let at = head
        .windows(5)
        .position(|w| w == b"%PDF-")
        .ok_or_else(|| IngestError::Metadata("PDF: no %PDF- header".into()))?;
    let version: String = head[at + 5..]
        .iter()
        .take_while(|b| b.is_ascii_digit() || **b == b'.')
        .map(|b| char::from(*b))
        .collect();
    if !(version.starts_with("1.") || version.starts_with("2.")) {
        return Err(IngestError::Metadata(format!(
            "PDF: unsupported version {version:?}"
        )));
    }
    let tail_from = size.saturating_sub(TAIL);
    r.seek(SeekFrom::Start(tail_from)).map_err(io)?;
    let mut tail = Vec::new();
    r.take(TAIL).read_to_end(&mut tail).map_err(io)?;
    let mut warnings = Vec::new();
    if !contains(&tail, b"%%EOF") {
        return Err(IngestError::Metadata(
            "PDF: no %%EOF marker (the file looks truncated)".into(),
        ));
    }
    // Classic trailers and xref-stream dictionaries both name `/Encrypt` outside any
    // compressed object stream.
    if contains(&tail, b"/Encrypt") {
        return Err(IngestError::Encrypted("(pdf)".into()));
    }
    if !contains(&tail, b"startxref") {
        warnings.push("no startxref: readers will have to rebuild the cross-reference".into());
    }
    Ok(PdfInfo {
        version,
        size,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const MINIMAL: &[u8] =
        b"%PDF-1.7\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\nstartxref\n0\n%%EOF\n";

    fn v(b: &[u8]) -> Result<PdfInfo, IngestError> {
        validate(Cursor::new(b.to_vec()), MAX_PDF_BYTES)
    }

    #[test]
    fn a_plain_pdf_is_accepted() {
        let i = v(MINIMAL).unwrap();
        assert_eq!(i.version, "1.7");
        assert_eq!(i.size, MINIMAL.len() as u64);
        assert!(i.warnings.is_empty());
        assert_eq!(v(b"%PDF-2.0\nstartxref\n0\n%%EOF").unwrap().version, "2.0");
        // Junk before the header is tolerated.
        assert!(v(b"\xEF\xBB\xBF%PDF-1.4\nstartxref\n0\n%%EOF").is_ok());
    }

    #[test]
    fn non_pdfs_encrypted_and_truncated_files_are_refused() {
        assert!(matches!(
            v(b"PK\x03\x04 not a pdf"),
            Err(IngestError::Metadata(_))
        ));
        assert!(matches!(
            v(b"%PDF-9.9\n%%EOF"),
            Err(IngestError::Metadata(_))
        ));
        assert!(matches!(
            v(b"%PDF-1.7\n1 0 obj<<>>endobj\n"),
            Err(IngestError::Metadata(m)) if m.contains("truncated")
        ));
        assert!(matches!(
            v(b"%PDF-1.7\ntrailer<</Encrypt 5 0 R>>\nstartxref\n0\n%%EOF"),
            Err(IngestError::Encrypted(_))
        ));
        assert!(matches!(v(b""), Err(IngestError::Metadata(_))));
    }

    #[test]
    fn size_is_capped_and_missing_startxref_is_a_warning() {
        assert!(matches!(
            validate(Cursor::new(MINIMAL.to_vec()), 10),
            Err(IngestError::TooLarge(_))
        ));
        let i = v(b"%PDF-1.7\nbody\n%%EOF").unwrap();
        assert_eq!(i.warnings.len(), 1);
    }

    #[test]
    fn only_the_tail_is_searched_for_markers() {
        // A big body with the trailer at the very end.
        let mut big = b"%PDF-1.7\n".to_vec();
        big.extend(vec![b'x'; 300_000]);
        big.extend(b"\nstartxref\n0\n%%EOF");
        assert!(v(&big).is_ok());
        // `/Encrypt` far from the end is ignored (it is inside the body, not the trailer).
        let mut body = b"%PDF-1.7\n/Encrypt ".to_vec();
        body.extend(vec![b'x'; 300_000]);
        body.extend(b"\nstartxref\n0\n%%EOF");
        assert!(v(&body).is_ok());
    }
}
