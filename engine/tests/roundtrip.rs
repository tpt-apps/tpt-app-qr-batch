//! Round-trip scan tests: every rendered QR must decode back to its payload
//! through an independent decoder (rqrr, the same class of algorithm phone
//! cameras use), including with the Pro logo overlay applied. Also proves
//! the stored-ZIP writer interoperates with a real unzip implementation
//! and the PNG encoder with a real PNG decoder.

use image::GrayImage;
use tpt_qr_batch_engine::{
    encode, render_png, render_rgba, render_svg, CodeSpec, LogoImage, RenderOptions, Symbology,
};

/// Render to an rqrr-scannable grayscale image (phone-camera stand-in).
fn scan_image(payload_data: &str) -> GrayImage {
    let spec = CodeSpec {
        data: payload_data.to_string(),
        symbology: Symbology::Qr,
        ..Default::default()
    };
    let encoded = encode(&spec).unwrap();
    let options = RenderOptions { scale: 4, quiet_modules: 4, ..Default::default() };
    let (width, height, rgba) = render_rgba(&encoded, &options, None);
    let gray: Vec<u8> = rgba
        .chunks_exact(4)
        .map(|px| {
            // Luma approximation (ITU-R BT.601).
            ((px[0] as u32 * 299 + px[1] as u32 * 587 + px[2] as u32 * 114) / 1000) as u8
        })
        .collect();
    GrayImage::from_raw(width, height, gray).expect("dimensions match buffer")
}

fn decode_first(image: GrayImage) -> Option<String> {
    let mut prepared = rqrr::PreparedImage::prepare(image);
    let grids = prepared.detect_grids();
    let grid = grids.into_iter().next()?;
    let (_meta, text) = grid.decode().ok()?;
    Some(text)
}

#[test]
fn rendered_qr_scans_back_to_payload() {
    for payload in [
        "https://tptsolutions.co.nz/tools/qr-batch",
        "HELLO WORLD 123",
        "Table 12 — scan me",
        "x".repeat(300).as_str(),
    ] {
        let image = scan_image(payload);
        assert_eq!(decode_first(image).as_deref(), Some(payload), "payload: {payload}");
    }
}

#[test]
fn rendered_qr_with_logo_still_scans() {
    // 64x64 solid-red logo with a transparent corner — typical uploaded mark.
    let logo = LogoImage {
        width: 64,
        height: 64,
        rgba: (0..64 * 64)
            .flat_map(|i| {
                if i < 64 {
                    vec![200, 30, 30, 0] // transparent first row
                } else {
                    vec![200, 30, 30, 255]
                }
            })
            .collect(),
    };
    let spec = CodeSpec {
        data: "https://example.com/menu".to_string(),
        symbology: Symbology::Qr,
        ecc: tpt_qr_batch_engine::QrEcc::High,
        ..Default::default()
    };
    let encoded = encode(&spec).unwrap();
    let options = RenderOptions { scale: 6, quiet_modules: 4, ..Default::default() };
    let (width, height, rgba) = render_rgba(&encoded, &options, Some(&logo));
    // Logo pixels actually landed in the centre (red present).
    let centre = ((height as usize / 2) * width as usize + width as usize / 2) * 4;
    assert!(rgba[centre] > 150 && rgba[centre + 1] < 100, "logo not composited");
    let gray: Vec<u8> = rgba
        .chunks_exact(4)
        .map(|px| ((px[0] as u32 * 299 + px[1] as u32 * 587 + px[2] as u32 * 114) / 1000) as u8)
        .collect();
    let image = GrayImage::from_raw(width, height, gray).unwrap();
    assert_eq!(
        decode_first(image).as_deref(),
        Some("https://example.com/menu"),
        "logo-overlaid QR must still scan (ECC High absorbs the damage)"
    );
}

#[test]
fn png_output_decodes_with_real_decoder() {
    let spec = CodeSpec {
        data: "png round trip".to_string(),
        symbology: Symbology::Qr,
        ..Default::default()
    };
    let encoded = encode(&spec).unwrap();
    let bytes = render_png(&encoded, &RenderOptions::default(), None).unwrap();
    // Decode with the image crate (independent of our writer path).
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap();
    assert_eq!(decoded.to_rgba8().dimensions(), (232, 232)); // (21 + 8) modules * 8 px
    // And the same QR must scan straight out of the PNG.
    assert_eq!(
        decode_first(decoded.to_luma8()).as_deref(),
        Some("png round trip"),
        "PNG-encoded QR must still scan"
    );
}

#[test]
fn svg_output_is_well_formed_enough_for_browsers() {
    let spec = CodeSpec {
        data: "svg check".to_string(),
        symbology: Symbology::Qr,
        ..Default::default()
    };
    let encoded = encode(&spec).unwrap();
    let svg = render_svg(&encoded, &RenderOptions::default());
    assert!(svg.ends_with("</svg>"));
    // Unescaped ampersands/quotes would break XML parsing in browsers.
    assert!(!svg.contains('&'));
}

#[cfg(feature = "pro")]
mod pro {
    use super::*;
    use tpt_qr_batch_engine::{
        csv::csv_rows, pdf::{sheet_pdf, sheet_stats, SheetLayout}, zip::stored_zip,
    };

    #[test]
    fn stored_zip_round_trips_through_zip_crate() {
        let files: Vec<(String, Vec<u8>)> = (0..5)
            .map(|i| (format!("code-{i}.png"), vec![0x89, b'P', b'N', b'G', i, i, i]))
            .collect();
        let archive = stored_zip(&files);
        let reader = std::io::Cursor::new(&archive);
        let mut zip = zip::ZipArchive::new(reader).unwrap();
        assert_eq!(zip.len(), 5);
        for (i, expected) in files.iter().enumerate() {
            let mut entry = zip.by_index(i).unwrap();
            assert_eq!(entry.name(), expected.0);
            let mut contents = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut contents).unwrap();
            assert_eq!(contents, expected.1);
        }
    }

    #[test]
    fn sheet_pdf_from_csv_batch() {
        // Third row has empty data — reported as invalid, not exported.
        let csv = "https://example.com/1,Table 1\nhttps://example.com/2,Table 2\n,x";
        let specs: Vec<CodeSpec> = csv_rows(csv)
            .into_iter()
            .map(|row| CodeSpec {
                data: row[0].clone(),
                symbology: Symbology::Qr,
                label: row.get(1).cloned(),
                ..Default::default()
            })
            .collect();
        let encoded: Vec<(CodeSpec, tpt_qr_batch_engine::Encoded)> = specs
            .iter()
            .filter_map(|spec| encode(spec).ok().map(|encoded| (spec.clone(), encoded)))
            .collect();
        assert_eq!(encoded.len(), 2);
        let layout = SheetLayout::default();
        let pdf = sheet_pdf(&encoded, &layout, [16, 20, 24], [255, 255, 255], None);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert_eq!(sheet_stats(encoded.len(), &layout).pages, 1);
    }
}
