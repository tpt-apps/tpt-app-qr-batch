//! TPT QR Batch engine — pure Rust, no UI deps, host-unit-testable.
//!
//! Pipeline:
//! 1. [`encode`] turns a [`CodeSpec`] (data string + symbology) into a
//!    resolution-free [`Encoded`] artifact (QR module matrix or a 1D bar
//!    pattern).
//! 2. Renderers turn that artifact into pixels ([`render_png`]) or vectors
//!    ([`render_svg`]), applying quiet zone, colours and (Pro) the
//!    centre-logo overlay.
//! 3. Batch inputs arrive as CSV ([`csv`]) — one code per row, optional
//!    label column.
//! 4. Pro outputs: [`pdf`] lays codes onto print-ready sheets (vector, so
//!    codes stay crisp at any print DPI), [`zip`] packages bulk PNGs.

pub mod csv;

#[cfg(feature = "pro")]
pub mod pdf;
#[cfg(feature = "pro")]
pub mod zip;

use qrcodegen::{QrCode, QrCodeEcc};

/// Symbologies offered. QR is the headline; the three 1D formats cover the
/// common retail/asset label cases from the spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symbology {
    Qr,
    Code128,
    Ean13,
    Code39,
}

impl Symbology {
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "qr" => Some(Symbology::Qr),
            "code128" => Some(Symbology::Code128),
            "ean13" => Some(Symbology::Ean13),
            "code39" => Some(Symbology::Code39),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Symbology::Qr => "qr",
            Symbology::Code128 => "code128",
            Symbology::Ean13 => "ean13",
            Symbology::Code39 => "code39",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Symbology::Qr => "QR code",
            Symbology::Code128 => "Code 128",
            Symbology::Ean13 => "EAN-13",
            Symbology::Code39 => "Code 39",
        }
    }

    /// What the symbology's input constraints are, shown in the UI help.
    pub fn constraint(&self) -> &'static str {
        match self {
            Symbology::Qr => "Any text or URL (up to ~1 200 characters)",
            Symbology::Code128 => "Any ASCII text (0-127)",
            Symbology::Ean13 => "12 digits (check digit added) or 13 with a valid check digit",
            Symbology::Code39 => "A-Z, 0-9, space and - . $ / + %",
        }
    }
}

/// QR error-correction level. Higher levels survive more damage — required
/// when overlaying a centre logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrEcc {
    Low,
    Medium,
    Quartile,
    High,
}

impl QrEcc {
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "l" | "low" => Some(QrEcc::Low),
            "m" | "medium" => Some(QrEcc::Medium),
            "q" | "quartile" => Some(QrEcc::Quartile),
            "h" | "high" => Some(QrEcc::High),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            QrEcc::Low => "L",
            QrEcc::Medium => "M",
            QrEcc::Quartile => "Q",
            QrEcc::High => "H",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            QrEcc::Low => "L · 7% recovery",
            QrEcc::Medium => "M · 15% recovery",
            QrEcc::Quartile => "Q · 25% recovery",
            QrEcc::High => "H · 30% recovery (logo-safe)",
        }
    }

    fn to_qrcodegen(self) -> QrCodeEcc {
        match self {
            QrEcc::Low => QrCodeEcc::Low,
            QrEcc::Medium => QrCodeEcc::Medium,
            QrEcc::Quartile => QrCodeEcc::Quartile,
            QrEcc::High => QrCodeEcc::High,
        }
    }
}

/// One code to generate.
#[derive(Debug, Clone)]
pub struct CodeSpec {
    pub data: String,
    pub symbology: Symbology,
    /// QR error-correction level (ignored by 1D symbologies). Forced to
    /// `High` by the UI when a centre logo is applied.
    pub ecc: QrEcc,
    /// Optional caption printed under the code (sheet label / second CSV
    /// column).
    pub label: Option<String>,
}

impl Default for CodeSpec {
    fn default() -> Self {
        CodeSpec {
            data: String::new(),
            symbology: Symbology::Qr,
            ecc: QrEcc::Medium,
            label: None,
        }
    }
}

/// Resolution-free encoded artifact. Rendering (quiet zone, colours, scale,
/// logo) is applied later so the same artifact feeds PNG, SVG and PDF.
#[derive(Debug, Clone, PartialEq)]
pub enum Encoded {
    /// `size` x `size` module matrix, row-major; `true` = dark module.
    Qr { size: usize, dark: Vec<bool> },
    /// 1D bar pattern (bars and spaces expanded to 1-unit modules, starting
    /// with a bar), no quiet zone.
    Linear { pattern: Vec<bool> },
}

impl Encoded {
    /// Modules across (linear patterns: total width in modules).
    pub fn modules_across(&self) -> usize {
        match self {
            Encoded::Qr { size, .. } => *size,
            Encoded::Linear { pattern } => pattern.len(),
        }
    }
}

/// Encoding failures, one per input row so CSV batches can report the row.
#[derive(Debug, Clone, PartialEq)]
pub enum EncodeError {
    EmptyInput,
    QrTooLong,
    BadAscii,
    BadEan13,
    BadCode39Chars,
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::EmptyInput => write!(f, "empty"),
            EncodeError::QrTooLong => write!(f, "too long for a QR code at this ECC level"),
            EncodeError::BadAscii => write!(f, "Code 128 accepts printable ASCII characters only"),
            EncodeError::BadEan13 => {
                write!(f, "EAN-13 needs 12 digits (or 13 with a valid check digit)")
            }
            EncodeError::BadCode39Chars => write!(f, "Code 39 allows A-Z 0-9 space and - . $ / + %"),
        }
    }
}

/// Standard EAN-13 check digit (weighted sum: odd positions x1, even x3).
pub fn ean13_check_digit(twelve: &str) -> Option<u8> {
    if twelve.len() != 12 || !twelve.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let sum: u32 = twelve
        .bytes()
        .enumerate()
        .map(|(i, b)| {
            let digit = (b - b'0') as u32;
            if i % 2 == 0 {
                digit
            } else {
                digit * 3
            }
        })
        .sum();
    Some(((10 - (sum % 10)) % 10) as u8)
}

/// Encode one spec into a resolution-free artifact.
pub fn encode(spec: &CodeSpec) -> Result<Encoded, EncodeError> {
    let data = spec.data.trim();
    if data.is_empty() {
        return Err(EncodeError::EmptyInput);
    }
    match spec.symbology {
        Symbology::Qr => {
            let code = QrCode::encode_text(data, spec.ecc.to_qrcodegen())
                .map_err(|_| EncodeError::QrTooLong)?;
            let size = code.size() as usize;
            let mut dark = vec![false; size * size];
            for y in 0..size {
                for x in 0..size {
                    dark[y * size + x] = code.get_module(x as i32, y as i32);
                }
            }
            Ok(Encoded::Qr { size, dark })
        }
        Symbology::Code128 => {
            if !data.is_ascii() {
                return Err(EncodeError::BadAscii);
            }
            // barcoders 2.0 requires an explicit start character-set; pick
            // it automatically: C for even-length digit runs (double
            // density), B for printable ASCII, A for control-character
            // payloads (uppercase-only).
            let prefixed: String = if data.len() % 2 == 0
                && data.bytes().all(|b| b.is_ascii_digit())
            {
                format!("\u{0106}{data}")
            } else if data.bytes().all(|b| (32..=126).contains(&b)) {
                format!("\u{0181}{data}")
            } else if data.bytes().all(|b| b < 32 || (32..=95).contains(&b)) {
                format!("\u{00C0}{data}")
            } else {
                return Err(EncodeError::BadAscii);
            };
            let barcode = barcoders::sym::code128::Code128::new(&prefixed)
                .map_err(|_| EncodeError::BadAscii)?;
            Ok(pattern_from_digits(&barcode.encode()))
        }
        Symbology::Ean13 => {
            let normalized = if data.len() == 12 {
                // 12 digits: append the check digit ourselves so behaviour is
                // independent of the encoder's acceptance rules.
                let check = ean13_check_digit(data).ok_or(EncodeError::BadEan13)?;
                format!("{data}{check}")
            } else {
                data.to_string()
            };
            if normalized.len() != 13 {
                return Err(EncodeError::BadEan13);
            }
            // Validate a supplied check digit before encoding.
            let provided = normalized.as_bytes()[12] - b'0';
            let computed = ean13_check_digit(&normalized[..12]).ok_or(EncodeError::BadEan13)?;
            if provided != computed {
                return Err(EncodeError::BadEan13);
            }
            let barcode = barcoders::sym::ean13::EAN13::new(&normalized)
                .map_err(|_| EncodeError::BadEan13)?;
            Ok(pattern_from_digits(&barcode.encode()))
        }
        Symbology::Code39 => {
            let barcode =
                barcoders::sym::code39::Code39::new(data).map_err(|_| EncodeError::BadCode39Chars)?;
            Ok(pattern_from_digits(&barcode.encode()))
        }
    }
}

fn pattern_from_digits(digits: &[u8]) -> Encoded {
    // barcoders returns a flat sequence of 0/1 digits (bars and spaces
    // already expanded to single modules); keep `nonzero` = dark so any
    // internal encoding quirk still yields a scannable alternation.
    Encoded::Linear {
        pattern: digits.iter().map(|&d| d != 0).collect(),
    }
}

/// Encode a whole batch, returning (row index + spec + artifact) pairs for
/// the valid rows and (row index + error) pairs for the invalid ones. Row
/// indices are 1-based, matching spreadsheet rows.
pub fn encode_batch(
    specs: &[CodeSpec],
) -> (Vec<(usize, CodeSpec, Encoded)>, Vec<(usize, EncodeError)>) {
    let mut ok = Vec::with_capacity(specs.len());
    let mut bad = Vec::new();
    for (index, spec) in specs.iter().enumerate() {
        match encode(spec) {
            Ok(encoded) => ok.push((index + 1, spec.clone(), encoded)),
            Err(error) => bad.push((index + 1, error)),
        }
    }
    (ok, bad)
}

/// RGB colour (branding).
pub type Rgb = [u8; 3];

/// Parse `#rgb` or `#rrggbb` (as produced by `<input type="color">`).
pub fn parse_hex_color(hex: &str) -> Option<Rgb> {
    let hex = hex.trim().strip_prefix('#')?;
    match hex.len() {
        3 => {
            let v: Vec<Option<u8>> = hex
                .chars()
                .map(|c| c.to_digit(16).map(|d| (d * 17) as u8))
                .collect();
            Some([v[0]?, v[1]?, v[2]?])
        }
        6 => {
            let byte = |range: std::ops::Range<usize>| {
                u8::from_str_radix(hex.get(range)?, 16).ok()
            };
            Some([byte(0..2)?, byte(2..4)?, byte(4..6)?])
        }
        _ => None,
    }
}

/// Colours and geometry applied at render time (free build: fixed dark on
/// white; the Pro build exposes these to the branding controls).
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Pixels per module.
    pub scale: u32,
    /// Quiet-zone width in modules (QR spec minimum 4; 1D recommended 10).
    pub quiet_modules: u32,
    pub foreground: Rgb,
    pub background: Rgb,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            scale: 8,
            quiet_modules: 4,
            foreground: [16, 20, 24],
            background: [255, 255, 255],
        }
    }
}

/// A decoded RGBA logo (Pro: uploaded PNG, composited over the code centre).
#[derive(Debug, Clone)]
pub struct LogoImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[cfg(feature = "pro")]
/// Decode an uploaded PNG logo into RGBA pixels. PNG only — every design
/// tool exports it, and skipping JPEG keeps the WASM payload small.
pub fn decode_logo_png(bytes: &[u8]) -> Result<LogoImage, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    let width = info.width;
    let height = info.height;
    let rgba = to_rgba(
        &buffer[..width as usize * height as usize * info.color_type.samples()],
        info.color_type,
    );
    Ok(LogoImage { width, height, rgba })
}

/// Expand a decoded PNG frame (after EXPAND/STRIP_16 transformations) to
/// RGBA regardless of its colour type. (Pro logo decoding only.)
#[cfg(feature = "pro")]
fn to_rgba(pixels: &[u8], color: png::ColorType) -> Vec<u8> {
    match color {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .chunks_exact(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .chunks_exact(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        // After EXPAND palettes never reach here.
        _ => pixels.to_vec(),
    }
}

/// Render the artifact to an RGBA raster (with quiet zone; logo applied if
/// given). Shared by the PNG path, the in-app preview and the decode tests.
pub fn render_rgba(
    encoded: &Encoded,
    options: &RenderOptions,
    logo: Option<&LogoImage>,
) -> (u32, u32, Vec<u8>) {
    let scale = options.scale.max(1);
    let quiet = options.quiet_modules;
    let fg = options.foreground;
    let bg = options.background;

    // QR renders square (modules + quiet zone); linear codes render one
    // repeated row of bars with a readable height band.
    let is_qr = matches!(encoded, Encoded::Qr { .. });
    let (cell_cols, cell_rows): (u32, u32) = match encoded {
        Encoded::Qr { size, .. } => (*size as u32, *size as u32),
        Encoded::Linear { pattern } => (pattern.len() as u32, 1),
    };
    let cols = (cell_cols + 2 * quiet) * scale;
    let rows = if is_qr {
        (cell_rows + 2 * quiet) * scale
    } else {
        (cols / 2).clamp(60, 400)
    };

    let dark_cell = |cx: u32, cy: u32| -> bool {
        let x = (cx - quiet) as usize;
        let y = (cy - quiet) as usize;
        match encoded {
            Encoded::Qr { size, dark } => y < *size && x < *size && dark[y * size + x],
            Encoded::Linear { pattern } => x < pattern.len() && pattern[x],
        }
    };

    let mut rgba = vec![0u8; (cols * rows * 4) as usize];
    for y in 0..rows {
        let cell_y = y / scale;
        let inside_y = cell_y >= quiet && (!is_qr || cell_y < cell_rows + quiet);
        for x in 0..cols {
            let cell_x = x / scale;
            let dark = inside_y
                && cell_x >= quiet
                && cell_x < cell_cols + quiet
                && dark_cell(cell_x, cell_y);
            let color = if dark { fg } else { bg };
            let offset = ((y * cols + x) * 4) as usize;
            rgba[offset..offset + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }

    if let (Encoded::Qr { .. }, Some(logo)) = (encoded, logo) {
        paste_logo(&mut rgba, cols, rows, logo);
    }

    (cols, rows, rgba)
}

/// Paste the logo centred over the code, with a small quiet ring of the
/// background colour so busy logo edges don't touch modules directly.
/// Sized to ~22% of the code width — comfortably inside the ~30% damage
/// budget of ECC level H.
fn paste_logo(rgba: &mut [u8], cols: u32, rows: u32, logo: &LogoImage) {
    let target = ((cols.min(rows) as f64 * 0.22) as u32).max(8);
    let pad = (target / 14).max(2);
    let box_size = target + 2 * pad;
    let box_x0 = cols.saturating_sub(box_size) / 2;
    let box_y0 = rows.saturating_sub(box_size) / 2;

    // Backing box (background colour, full alpha).
    for y in box_y0..box_y0 + box_size {
        for x in box_x0..box_x0 + box_size {
            let offset = ((y * cols + x) * 4) as usize;
            if offset + 4 <= rgba.len() {
                rgba[offset] = 255;
                rgba[offset + 1] = 255;
                rgba[offset + 2] = 255;
                rgba[offset + 3] = 255;
            }
        }
    }

    // Nearest-neighbour scale of the RGBA logo onto the centre.
    let lx0 = box_x0 + pad;
    let ly0 = box_y0 + pad;
    for y in 0..target {
        let source_y = (y as u64 * logo.height as u64 / target as u64) as u32;
        for x in 0..target {
            let source_x = (x as u64 * logo.width as u64 / target as u64) as u32;
            let source = ((source_y * logo.width + source_x) * 4) as usize;
            let a = logo.rgba[source + 3];
            if a == 0 {
                continue;
            }
            let offset = (((ly0 + y) * cols + (lx0 + x)) * 4) as usize;
            if offset + 4 <= rgba.len() && source + 4 <= logo.rgba.len() {
                let alpha = a as u32;
                let inv = 255 - alpha;
                rgba[offset] = ((logo.rgba[source] as u32 * alpha
                    + rgba[offset] as u32 * inv)
                    / 255) as u8;
                rgba[offset + 1] = ((logo.rgba[source + 1] as u32 * alpha
                    + rgba[offset + 1] as u32 * inv)
                    / 255) as u8;
                rgba[offset + 2] = ((logo.rgba[source + 2] as u32 * alpha
                    + rgba[offset + 2] as u32 * inv)
                    / 255) as u8;
                rgba[offset + 3] = 255;
            }
        }
    }
}

/// Render to PNG bytes (quiet zone + colours + optional logo).
pub fn render_png(
    encoded: &Encoded,
    options: &RenderOptions,
    logo: Option<&LogoImage>,
) -> Result<Vec<u8>, String> {
    let (width, height, rgba) = render_rgba(encoded, options, logo);
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// Render to a compact SVG (vector: crisp at any size, ideal for print).
/// Consecutive dark modules in a row are merged into one rect.
pub fn render_svg(encoded: &Encoded, options: &RenderOptions) -> String {
    let quiet = options.quiet_modules;
    let fg = format!("#{:02x}{:02x}{:02x}", options.foreground[0], options.foreground[1], options.foreground[2]);

    let (cells_w, cells_h, dark_at): (u32, u32, Box<dyn Fn(u32, u32) -> bool + '_>) = match encoded {
        Encoded::Qr { size, dark } => {
            let size = *size;
            (
                size as u32,
                size as u32,
                Box::new(move |x, y| dark[(y as usize) * size + x as usize]),
            )
        }
        Encoded::Linear { pattern } => (
            pattern.len() as u32,
            1,
            Box::new(move |x, _| pattern[x as usize]),
        ),
    };

    let total_w = cells_w + 2 * quiet;
    let total_h = if matches!(encoded, Encoded::Qr { .. }) {
        cells_h + 2 * quiet
    } else {
        // Give linear SVGs a sensible height ratio (1:2 portrait of width).
        ((cells_w + 2 * quiet) / 2).max(30)
    };

    let mut rects = String::new();
    for y in 0..cells_h {
        let mut x = 0;
        while x < cells_w {
            if dark_at(x, y) {
                let start = x;
                while x < cells_w && dark_at(x, y) {
                    x += 1;
                }
                rects.push_str(&format!(
                    "M{},{}h{}v1h-{}z",
                    start + quiet,
                    y + quiet,
                    x - start,
                    x - start
                ));
            } else {
                x += 1;
            }
        }
    }

    format!(
        concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" ",
            "shape-rendering=\"crispEdges\">",
            "<rect width=\"{w}\" height=\"{h}\" fill=\"#ffffff\"/>",
            "<path d=\"{rects}\" fill=\"{fg}\"/>",
            "</svg>"
        ),
        w = total_w,
        h = total_h,
        rects = rects,
        fg = fg,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(data: &str, symbology: Symbology) -> CodeSpec {
        CodeSpec { data: data.to_string(), symbology, ..Default::default() }
    }

    #[test]
    fn ean13_check_digits() {
        assert_eq!(ean13_check_digit("400638133393"), Some(1));
        assert_eq!(ean13_check_digit("750103133693"), Some(7));
        assert_eq!(ean13_check_digit("12345"), None);
    }

    #[test]
    fn qr_matrix_is_square_and_scannable_shape() {
        let encoded = encode(&spec("https://tptsolutions.co.nz", Symbology::Qr)).unwrap();
        match encoded {
            Encoded::Qr { size, dark } => {
                assert!(size >= 21 && size <= 177, "version out of range: {size}");
                assert_eq!(dark.len(), size * size);
            }
            _ => panic!("QR spec encoded as linear"),
        }
    }

    #[test]
    fn qr_rejects_empty_and_huge() {
        assert_eq!(encode(&spec("   ", Symbology::Qr)), Err(EncodeError::EmptyInput));
        let huge = "x".repeat(5000);
        assert_eq!(encode(&spec(&huge, Symbology::Qr)), Err(EncodeError::QrTooLong));
    }

    #[test]
    fn ean13_normalizes_and_validates() {
        // 12 digits: check digit appended by the engine.
        let encoded = encode(&spec("400638133393", Symbology::Ean13)).unwrap();
        match encoded {
            Encoded::Linear { pattern } => {
                // EAN-13 glyphs: 95 modules (3+6*7+5+6*7+3).
                assert_eq!(pattern.len(), 95);
            }
            _ => panic!("EAN spec encoded as QR"),
        }
        // 13 digits with a bad check digit is rejected.
        assert_eq!(
            encode(&spec("4006381333932", Symbology::Ean13)),
            Err(EncodeError::BadEan13)
        );
        // And with the good one it passes.
        assert!(encode(&spec("4006381333931", Symbology::Ean13)).is_ok());
    }

    #[test]
    fn code128_requires_ascii() {
        assert!(encode(&spec("Hello-123", Symbology::Code128)).is_ok());
        assert_eq!(
            encode(&spec("héllo", Symbology::Code128)),
            Err(EncodeError::BadAscii)
        );
    }

    #[test]
    fn code39_charset_enforced() {
        assert!(encode(&spec("ABC-123", Symbology::Code39)).is_ok());
        assert_eq!(
            encode(&spec("abc", Symbology::Code39)),
            Err(EncodeError::BadCode39Chars)
        );
    }

    #[test]
    fn hex_colors_parse() {
        assert_eq!(parse_hex_color("#0a141e"), Some([10, 20, 30]));
        assert_eq!(parse_hex_color("#fff"), Some([255, 255, 255]));
        assert_eq!(parse_hex_color("fff"), None);
    }

    #[test]
    fn render_rgba_dimensions_and_quiet_zone() {
        let encoded = encode(&spec("test", Symbology::Qr)).unwrap();
        let options = RenderOptions { scale: 4, quiet_modules: 4, ..Default::default() };
        let (w, h, rgba) = render_rgba(&encoded, &options, None);
        assert_eq!(w, h); // QR renders square
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // Top-left quiet-zone pixel must be background.
        assert_eq!(&rgba[0..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn linear_render_has_bar_height() {
        let encoded = encode(&spec("400638133393", Symbology::Ean13)).unwrap();
        let options = RenderOptions::default();
        let (w, h, _) = render_rgba(&encoded, &options, None);
        assert!(h >= 60 && h <= 400, "linear height out of band: {h}");
        assert!(w > h);
    }

    #[test]
    fn svg_contains_runs_and_viewbox() {
        let encoded = encode(&spec("svg test", Symbology::Qr)).unwrap();
        let svg = render_svg(&encoded, &RenderOptions::default());
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("viewBox="));
        assert!(svg.contains("<path"));
        // Runs are merged: far fewer path segments than dark modules.
        let Encoded::Qr { dark, .. } = encoded else { panic!() };
        let dark_count = dark.iter().filter(|&&d| d).count();
        let m_count = svg.matches('M').count();
        assert!(m_count <= dark_count);
    }

    #[test]
    fn batch_reports_row_numbers() {
        let specs = vec![
            spec("ok-1", Symbology::Qr),
            spec("", Symbology::Qr),
            spec("ok-2", Symbology::Qr),
        ];
        let (ok, bad) = encode_batch(&specs);
        assert_eq!(ok.len(), 2);
        assert_eq!(bad, vec![(2, EncodeError::EmptyInput)]);
    }
}
