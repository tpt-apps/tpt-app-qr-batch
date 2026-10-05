//! Pro edition: print-ready sheet PDF writer (PDF 1.4, vector, Helvetica
//! labels). Codes are drawn as vector rectangles (one per run of dark
//! modules), so they stay crisp at any print DPI. No external layout deps —
//! a text-stream emitter with a correct xref table, mirroring fea-lite's
//! report writer; content streams are Flate-compressed via flate2.

use std::io::Write;

use crate::{CodeSpec, Encoded, LogoImage, Rgb};

/// Millimetres -> PDF points.
const MM: f64 = 72.0 / 25.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    A4,
    Letter,
}

impl PageSize {
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "a4" => Some(PageSize::A4),
            "letter" => Some(PageSize::Letter),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            PageSize::A4 => "a4",
            PageSize::Letter => "letter",
        }
    }

    pub fn pts(&self) -> (f64, f64) {
        match self {
            // (width, height) in points.
            PageSize::A4 => (595.28, 841.89),
            PageSize::Letter => (612.0, 792.0),
        }
    }
}

/// Sheet geometry. `columns`/`cell_mm`/`gap_mm`/`margin_mm` are clamped to
/// sane ranges so hostile UI input can never produce a broken PDF.
#[derive(Debug, Clone)]
pub struct SheetLayout {
    pub page: PageSize,
    pub columns: usize,
    /// Code width per cell (QR cells are square; linear cells get a fixed
    /// 12 mm bar height).
    pub cell_mm: f64,
    pub gap_mm: f64,
    pub margin_mm: f64,
    pub show_labels: bool,
    pub cut_guides: bool,
}

impl Default for SheetLayout {
    fn default() -> Self {
        SheetLayout {
            page: PageSize::A4,
            columns: 4,
            cell_mm: 30.0,
            gap_mm: 4.0,
            margin_mm: 10.0,
            show_labels: true,
            cut_guides: true,
        }
    }
}

impl SheetLayout {
    fn clamped(&self) -> SheetLayout {
        let mut clamped = SheetLayout {
            page: self.page,
            columns: self.columns.clamp(1, 8),
            cell_mm: self.cell_mm.clamp(15.0, 70.0),
            gap_mm: self.gap_mm.clamp(0.0, 20.0),
            margin_mm: self.margin_mm.clamp(5.0, 25.0),
            show_labels: self.show_labels,
            cut_guides: self.cut_guides,
        };
        // Columns must also fit the page width — a 70 mm code at 8 columns
        // would run off the sheet, so cap by usable width.
        let (page_w, _) = clamped.page.pts();
        let usable_w = page_w - 2.0 * clamped.margin_mm * MM;
        let cell_w = clamped.cell_mm * MM;
        let gap = clamped.gap_mm * MM;
        let max_cols = (((usable_w + gap) / (cell_w + gap)).floor() as usize).max(1);
        clamped.columns = clamped.columns.min(max_cols);
        clamped
    }
}

/// How many codes fit per page and how many pages a batch needs — shown in
/// the UI before export.
pub struct SheetStats {
    pub per_page: usize,
    pub pages: usize,
}

pub fn sheet_stats(count: usize, layout: &SheetLayout) -> SheetStats {
    let layout = layout.clamped();
    let (_, page_h) = layout.page.pts();
    let usable_h = page_h - 2.0 * layout.margin_mm * MM;
    let (cell_h_pt, label_h) = cell_heights(&layout);
    let row_h = cell_h_pt + label_h + layout.gap_mm * MM;
    let rows_per_page = ((usable_h + layout.gap_mm * MM) / row_h).floor().max(1.0) as usize;
    let per_page = rows_per_page * layout.columns;
    SheetStats {
        per_page,
        pages: count.div_ceil(per_page).max(1),
    }
}

/// (cell height in points, label band height in points).
fn cell_heights(layout: &SheetLayout) -> (f64, f64) {
    // QR/Code39-style square cells vs the fixed-height linear band; the
    // caller mixes symbologies per batch, so the row height must fit the
    // taller kind: square cells dominate.
    let square = layout.cell_mm * MM;
    let linear = 12.0 * MM + if layout.show_labels { 4.0 * MM } else { 0.0 };
    let cell = square.max(linear);
    let label = if layout.show_labels { 4.0 * MM } else { 0.0 };
    (cell, label)
}

/// Render `entries` (spec + pre-encoded artifact pairs from
/// [`crate::encode_batch`]) onto print-ready pages. Colours follow the
/// branding controls; the page itself stays white. When a `logo` is given
/// it is embedded once as an image XObject and drawn centred on every QR
/// cell (1D cells stay logo-free — there is no error correction to lean on).
pub fn sheet_pdf(
    entries: &[(CodeSpec, Encoded)],
    layout: &SheetLayout,
    foreground: Rgb,
    background: Rgb,
    logo: Option<&LogoImage>,
) -> Vec<u8> {
    let layout = layout.clamped();
    let (page_w, page_h) = layout.page.pts();
    let margin = layout.margin_mm * MM;
    let cell_w = layout.cell_mm * MM;
    let (cell_h, label_h) = cell_heights(&layout);
    let gap = layout.gap_mm * MM;

    let usable_h = page_h - 2.0 * margin;
    let row_h = cell_h + label_h + gap;
    let rows_per_page = ((usable_h + gap) / row_h).floor().max(1.0) as usize;
    let per_page = rows_per_page * layout.columns;

    let pages: Vec<&[(CodeSpec, Encoded)]> = if entries.is_empty() {
        vec![&[]]
    } else {
        entries.chunks(per_page.max(1)).collect()
    };

    let mut page_numbers: Vec<usize> = Vec::new();
    let mut content_numbers: Vec<usize> = Vec::new();
    let mut objects: Vec<(usize, Vec<u8>)> = Vec::new();

    // Object numbering: 1 catalog, 2 pages, 3 font, optionally 4 the logo
    // image, then page/content pairs.
    let mut next = 4;
    let image_number = logo.map(|_| {
        let number = next;
        next += 1;
        number
    });
    for _ in pages.iter() {
        page_numbers.push(next);
        content_numbers.push(next + 1);
        next += 2;
    }

    let fg = rgb_to_pdf(foreground);
    let bg = rgb_to_pdf(background);

    for (page_index, entries_page) in pages.iter().enumerate() {
        // ---- content stream ----
        let mut ops = String::new();
        for (slot, (spec, encoded)) in entries_page.iter().enumerate() {
            let col = slot % layout.columns;
            let row = slot / layout.columns;
            let x0 = margin + col as f64 * (cell_w + gap);
            let top = page_h - margin - row as f64 * row_h;

            // Background band (quiet zone) then the code itself.
            match encoded {
                Encoded::Qr { size, dark } => {
                    let quiet = 4.0;
                    let module = cell_w / (*size as f64 + 2.0 * quiet);
                    let origin = quiet * module;
                    ops.push_str(&format!("\n{bg} rg\n"));
                    ops.push_str(&format!(
                        "{:.2} {:.2} {:.2} {:.2} re f\n",
                        x0,
                        top - cell_h,
                        cell_w,
                        cell_h
                    ));
                    ops.push_str(&format!("{fg} rg\n"));
                    for y in 0..*size {
                        let mut x = 0;
                        while x < *size {
                            if dark[y * size + x] {
                                let start = x;
                                while x < *size && dark[y * size + x] {
                                    x += 1;
                                }
                                // PDF y grows upward; render top-down rows.
                                let py = top - origin - (y as f64 + 1.0) * module;
                                ops.push_str(&format!(
                                    "{:.2} {:.2} {:.2} {:.2} re f\n",
                                    x0 + origin + start as f64 * module,
                                    py,
                                    (x - start) as f64 * module,
                                    module
                                ));
                            } else {
                                x += 1;
                            }
                        }
                    }
                    // Centre logo: drawn over the modules, sized inside the
                    // ECC-H damage budget (caller forces High with logos).
                    if let Some(image) = image_number {
                        let side = cell_w * 0.24;
                        let cx = x0 + (cell_w - side) / 2.0;
                        let cy = top - cell_h + (cell_h - side) / 2.0;
                        ops.push_str(&format!(
                            "q {side:.2} 0 0 {side:.2} {cx:.2} {cy:.2} cm /I{image} Do Q\n"
                        ));
                    }
                }
                Encoded::Linear { pattern } => {
                    let quiet = 10.0;
                    let module = cell_w / (pattern.len() as f64 + 2.0 * quiet);
                    let bar_h = 12.0 * MM;
                    let origin = quiet * module;
                    let y0 = top - bar_h;
                    ops.push_str(&format!("\n{bg} rg\n"));
                    ops.push_str(&format!(
                        "{:.2} {:.2} {:.2} {:.2} re f\n",
                        x0,
                        y0,
                        cell_w,
                        bar_h
                    ));
                    ops.push_str(&format!("{fg} rg\n"));
                    let mut x = 0;
                    while x < pattern.len() {
                        if pattern[x] {
                            let start = x;
                            while x < pattern.len() && pattern[x] {
                                x += 1;
                            }
                            ops.push_str(&format!(
                                "{:.2} {:.2} {:.2} {:.2} re f\n",
                                x0 + origin + start as f64 * module,
                                y0,
                                (x - start) as f64 * module,
                                bar_h
                            ));
                        } else {
                            x += 1;
                        }
                    }
                }
            }

            // Label under the code (WinAnsi, auto-shrunk to fit).
            if layout.show_labels {
                if let Some(label) = spec
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                {
                    let text = win_ansi(label);
                    let base_y = top - cell_h - 7.0;
                    let mut size = 7.0f64;
                    let est_width = |size: f64| text.chars().count() as f64 * size * 0.52;
                    while est_width(size) > cell_w && size > 3.5 {
                        size -= 0.5;
                    }
                    let x = x0 + ((cell_w - est_width(size)) / 2.0).max(0.0);
                    ops.push_str(&format!(
                        "BT /F1 {size:.1} Tf 0.25 0.25 0.25 rg 1 0 0 1 {x:.2} {base_y:.2} Tm ({text}) Tj ET\n"
                    ));
                }
            }
        }

        // Cut guides: dashed grid across the used area (also across an
        // empty final page so blank pages stay clean).
        if layout.cut_guides {
            let used_rows = entries_page
                .len()
                .div_ceil(layout.columns)
                .max(1);
            let grid_w = layout.columns as f64 * cell_w + (layout.columns as f64 - 1.0) * gap;
            let grid_h = used_rows as f64 * row_h - gap;
            let grid_top = page_h - margin;
            ops.push_str("0.72 0.72 0.72 RG 0.4 w [2 3] 0 d\n");
            for col in 0..=layout.columns {
                let x = margin + col as f64 * (cell_w + gap) - if col == 0 { 0.0 } else { gap / 2.0 };
                let x = if col == layout.columns { margin + grid_w } else { x };
                ops.push_str(&format!(
                    "{x:.2} {:.2} m {x:.2} {:.2} l S\n",
                    grid_top,
                    grid_top - grid_h
                ));
            }
            for row in 0..=used_rows {
                let y = grid_top - row as f64 * row_h + if row == 0 { 0.0 } else { gap / 2.0 };
                let y = if row == used_rows { grid_top - grid_h } else { y };
                ops.push_str(&format!(
                    "{margin:.2} {y:.2} m {:.2} {y:.2} l S\n",
                    margin + grid_w
                ));
            }
            ops.push_str("[] 0 d\n");
        }

        let mut compressed = Vec::new();
        {
            let mut encoder =
                flate2::write::ZlibEncoder::new(&mut compressed, flate2::Compression::default());
            let _ = encoder.write_all(ops.as_bytes());
            let _ = encoder.finish();
        }
        objects.push((content_numbers[page_index], content_object(&compressed)));
        objects.push((
            page_numbers[page_index],
            page_object(content_numbers[page_index], page_w, page_h, image_number),
        ));
    }

    // The shared logo image XObject: RGB, Flate-compressed, alpha
    // composited over white (PDF images have no cheap RGBA story).
    if let (Some(number), Some(logo)) = (image_number, logo) {
        let mut rgb = Vec::with_capacity((logo.width * logo.height * 3) as usize);
        for px in logo.rgba.chunks_exact(4) {
            let alpha = px[3] as u32;
            let blend = |channel: u8| -> u8 {
                ((channel as u32 * alpha + 255 * (255 - alpha)) / 255) as u8
            };
            rgb.push(blend(px[0]));
            rgb.push(blend(px[1]));
            rgb.push(blend(px[2]));
        }
        let mut compressed = Vec::new();
        {
            let mut encoder =
                flate2::write::ZlibEncoder::new(&mut compressed, flate2::Compression::default());
            let _ = encoder.write_all(&rgb);
            let _ = encoder.finish();
        }
        let body = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} \
             /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode >>\nstream\n",
            logo.width, logo.height
        );
        let mut object = body.into_bytes();
        object.extend_from_slice(&compressed);
        object.extend_from_slice(b"\nendstream");
        objects.push((number, object));
    }

    assemble_pdf(&page_numbers, objects)
}

fn rgb_to_pdf(rgb: Rgb) -> String {
    format!(
        "{:.3} {:.3} {:.3}",
        rgb[0] as f64 / 255.0,
        rgb[1] as f64 / 255.0,
        rgb[2] as f64 / 255.0
    )
}

/// Map to WinAnsi-safe text: PDFDocStrings escape specials; characters
/// outside Latin-1 become `?` (labels are near-always ASCII; the UI notes
/// this for the label field).
fn win_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        let code = c as u32;
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            _ if code >= 32 && code < 256 => out.push(c),
            _ => out.push('?'),
        }
    }
    out
}

fn content_object(compressed: &[u8]) -> Vec<u8> {
    let mut object = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", compressed.len())
        .into_bytes();
    object.extend_from_slice(compressed);
    object.extend_from_slice(b"\nendstream");
    object
}

fn page_object(content: usize, page_w: f64, page_h: f64, image: Option<usize>) -> Vec<u8> {
    let xobjects = match image {
        Some(number) => format!(" /XObject << /I{number} {number} 0 R >>"),
        None => String::new(),
    };
    format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w:.2} {page_h:.2}] \
         /Resources << /Font << /F1 3 0 R >>{xobjects} >> /Contents {content} 0 R >>"
    )
    .into_bytes()
}

fn assemble_pdf(page_numbers: &[usize], mut objects: Vec<(usize, Vec<u8>)>) -> Vec<u8> {
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    let kids: Vec<String> = page_numbers.iter().map(|n| format!("{n} 0 R")).collect();
    let pages_dict = format!(
        "<< /Type /Pages /Kids [{}] /Count {} >>",
        kids.join(" "),
        page_numbers.len()
    );
    let catalog = b"<< /Type /Catalog /Pages 2 0 R >>";
    let font =
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>";

    objects.push((2, pages_dict.into_bytes()));
    objects.push((1, catalog.to_vec()));
    objects.push((3, font.to_vec()));
    objects.sort_by_key(|(number, _)| *number);

    let mut offsets: Vec<(usize, usize)> = Vec::new();
    for (number, body) in &objects {
        offsets.push((*number, pdf.len()));
        pdf.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }

    let xref_offset = pdf.len();
    let max_object = objects.iter().map(|(n, _)| *n).max().unwrap_or(3);
    pdf.extend_from_slice(format!("xref\n0 {}\n", max_object + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    let mut by_number: std::collections::HashMap<usize, usize> = offsets.into_iter().collect();
    for number in 1..=max_object {
        match by_number.remove(&number) {
            Some(offset) => {
                pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            }
            None => {
                pdf.extend_from_slice(b"0000000000 65535 f \n");
            }
        }
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            max_object + 1
        )
        .as_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{encode, Symbology};
    use std::io::Read;

    fn layout() -> SheetLayout {
        SheetLayout::default()
    }

    fn entries(count: usize) -> Vec<(CodeSpec, Encoded)> {
        (0..count)
            .map(|i| {
                let spec = CodeSpec {
                    data: format!("https://example.com/{i}"),
                    symbology: Symbology::Qr,
                    label: Some(format!("Label {i}")),
                    ..Default::default()
                };
                let encoded = encode(&spec).unwrap();
                (spec, encoded)
            })
            .collect()
    }

    #[test]
    fn pdf_has_valid_skeleton() {
        let pdf = sheet_pdf(&entries(9), &layout(), [16, 20, 24], [255, 255, 255], None);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        assert!(pdf.windows(4).any(|w| w == b"xref"));
        // 9 codes at 4 columns => 3 rows => 1 page.
        let stats = sheet_stats(9, &layout());
        assert_eq!(stats.pages, 1);
        assert!(pdf.windows(12).any(|w| w == b"/Type /Pages"));
    }

    #[test]
    fn pages_flow_and_count_matches_stats() {
        let stats = sheet_stats(50, &layout());
        assert!(stats.pages > 1, "50 codes must span multiple pages");
        let pdf = sheet_pdf(&entries(50), &layout(), [16, 20, 24], [255, 255, 255], None);
        // "/Type /Page " (trailing space, 12 bytes) matches page objects
        // only, not the "/Type /Pages" catalog dict.
        let page_count = pdf
            .windows(12)
            .filter(|w| w == b"/Type /Page ")
            .count();
        assert_eq!(page_count, stats.pages);
    }

    #[test]
    fn content_streams_are_flate_round_trippable() {
        let pdf = sheet_pdf(&entries(3), &layout(), [16, 20, 24], [255, 255, 255], None);
        // Pull out one stream and inflate it back — proves the Filter
        // header matches what flate2 actually wrote.
        let marker = b"/Filter /FlateDecode >>\nstream\n";
        let start = pdf
            .windows(marker.len())
            .position(|w| w == marker)
            .expect("a flate stream");
        let body_start = start + marker.len();
        let end_marker = b"\nendstream";
        let body_end = pdf[body_start..]
            .windows(end_marker.len())
            .position(|w| w == end_marker)
            .expect("stream end")
            + body_start;
        let mut inflated = Vec::new();
        let mut decoder = flate2::read::ZlibDecoder::new(&pdf[body_start..body_end]);
        decoder.read_to_end(&mut inflated).unwrap();
        assert!(inferred_is_content(&inflated));
    }

    fn inferred_is_content(stream: &[u8]) -> bool {
        // Content streams here always start with a colour operator or a
        // background rect fill; all ops are ASCII.
        let text = String::from_utf8_lossy(stream);
        text.contains(" re f") || text.contains(" rg")
    }

    #[test]
    fn logo_embeds_as_image_xobject() {
        let logo = crate::LogoImage {
            width: 8,
            height: 8,
            rgba: (0..64).flat_map(|_| [200u8, 30, 30, 255]).collect(),
        };
        let pdf = sheet_pdf(&entries(2), &layout(), [16, 20, 24], [255, 255, 255], Some(&logo));
        // Dictionary text is plaintext even though streams are compressed.
        assert!(pdf.windows(15).any(|w| w == b"/Subtype /Image"));
        assert!(pdf.windows(11).any(|w| w == b"/XObject <<"));
        // The draw op lives inside a Flate stream — inflate every stream and
        // prove one references the shared XObject.
        assert!(any_stream_contains(&pdf, b" Do Q"));
    }

    /// Inflate every stream in `pdf` and return true if any contains `needle`.
    fn any_stream_contains(pdf: &[u8], needle: &[u8]) -> bool {
        let marker = b"stream\n";
        let end_marker = b"endstream";
        let mut index = 0;
        while let Some(offset) = pdf[index..]
            .windows(marker.len())
            .position(|w| w == marker)
        {
            let start = index + offset + marker.len();
            let Some(end) = pdf[start..]
                .windows(end_marker.len())
                .position(|w| w == end_marker)
            else {
                break;
            };
            let mut inflated = Vec::new();
            let ok = flate2::read::ZlibDecoder::new(&pdf[start..start + end])
                .read_to_end(&mut inflated)
                .is_ok();
            if ok && inflated.windows(needle.len()).any(|w| w == needle) {
                return true;
            }
            // Advance past this "endstream" — the marker also matches inside
            // the word itself.
            index = start + end + end_marker.len();
        }
        false
    }

    #[test]
    fn stats_clamp_hostile_layout() {
        let stats = sheet_stats(
            10,
            &SheetLayout {
                columns: 99,
                cell_mm: 500.0,
                gap_mm: -5.0,
                margin_mm: 0.0,
                ..SheetLayout::default()
            },
        );
        assert!(stats.per_page >= 1);
        // Clamped to 2 columns of 70 mm cells on A4 (~3 rows/page => 6 per
        // page), so 10 codes must span at least 2 pages — never off-sheet.
        assert!(stats.pages >= 2);
    }
}
