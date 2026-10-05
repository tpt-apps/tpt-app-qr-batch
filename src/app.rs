//! App UI (tpt-appfront-dom), following the cutlist/fea-lite architecture:
//!
//! - the **input shell is mounted once** (`tpt_appfront_dom::mount`) — its
//!   nodes are never reconciled, so typing never loses focus; field values
//!   are read straight from the DOM when Generate is pressed;
//! - the **results section re-renders fine-grained** via
//!   `tpt_appfront_dom::render` driven by the `results` Signal, with a
//!   stable child shape (four containers at fixed positions) so in-place
//!   reconciliation never has to full-replace;
//! - generated artifacts (full-res PNG / SVG / batch specs) live in an
//!   `Rc<RefCell<Artifacts>>` captured by the dispatch closure, so the
//!   download buttons always export exactly what was last generated.

use std::cell::RefCell;
use std::rc::Rc;

use tpt_appfront_core::{ContainerBuilder, Signal, UITree};
use tpt_qr_batch_engine::{
    encode, render_png, render_rgba, render_svg, CodeSpec, LogoImage, QrEcc, RenderOptions,
    Symbology,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;

#[cfg(feature = "pro")]
use tpt_qr_batch_engine::{
    csv::csv_rows, decode_logo_png, encode_batch, pdf::{sheet_pdf, sheet_stats, SheetLayout, PageSize},
    zip::stored_zip, Rgb,
};

/// What the results section is showing.
#[derive(Clone)]
enum Outcome {
    Idle,
    Error { message: String, hint: String },
    Single {
        meta: String,
        note: String,
    },
    #[cfg(feature = "pro")]
    Batch {
        summary: String,
        errors: Vec<(usize, String)>,
        preview_rows: Vec<Vec<String>>,
        more: usize,
        sheet_summary: String,
    },
}

/// Full-resolution exports for the download buttons (kept out of the
/// reactive tree — only the small preview PNG goes into the DOM).
struct SingleArtifacts {
    png: Vec<u8>,
    svg: String,
    stem: String,
}

#[derive(Default)]
struct Artifacts {
    single: Option<SingleArtifacts>,
    #[cfg(feature = "pro")]
    batch: Vec<CodeSpec>,
}

#[derive(Debug, Clone)]
enum Msg {
    Generate,
    Clear,
    LoadExample,
    ToggleHelp,
    DownloadPng,
    DownloadSvg,
    #[cfg(feature = "pro")]
    UploadCsv,
    #[cfg(feature = "pro")]
    CsvChanged(String),
    #[cfg(feature = "pro")]
    DownloadZip,
    #[cfg(feature = "pro")]
    ExportPdf,
    #[cfg(feature = "pro")]
    UploadLogo,
    #[cfg(feature = "pro")]
    ClearLogo,
}

const EXAMPLE_DATA: &str = "https://tptsolutions.co.nz/tools/qr-batch";

#[cfg(not(feature = "pro"))]
const ENGINE_NOTE: &str = "Generation happens entirely in your browser — your data never leaves this page.";

#[cfg(feature = "pro")]
const ENGINE_NOTE: &str =
    "TPT QR Batch Pro — bulk CSV, branding and print sheets run entirely offline on this device.";

// ---------------------------------------------------------------------------
// mount
// ---------------------------------------------------------------------------

pub fn mount_app(container: &web_sys::Element) -> Result<(), JsValue> {
    let document = web_sys::window().expect("no window").document().expect("no document");

    upsert_style(&document, "qrb-app-styles", BASE_CSS);

    let results = Signal::new(Outcome::Idle);
    let help_open = Signal::new(false);
    let logo_status = Signal::new("No logo loaded".to_string());
    let help_open_for_view = help_open.clone();

    let artifacts: Rc<RefCell<Artifacts>> = Rc::new(RefCell::new(Artifacts::default()));

    #[cfg(feature = "pro")]
    let logo: Rc<RefCell<Option<(String, LogoImage)>>> = Rc::new(RefCell::new(None));

    // Restore remembered preferences.
    if let Some(sym) = storage_get("qrb-sym").and_then(|v| Symbology::from_name(&v)) {
        write_field(container, ".qrb-sym", sym.name());
    }
    if let Some(ecc) = storage_get("qrb-ecc").and_then(|v| QrEcc::from_name(&v)) {
        write_field(container, ".qrb-ecc", ecc.name());
    }
    if let Some(scale) = storage_get("qrb-scale") {
        if ["4", "6", "8", "12"].contains(&scale.as_str()) {
            write_field(container, ".qrb-scale", &scale);
        }
    }

    let dispatch: Rc<dyn Fn(Msg)> = {
        let container = container.clone();
        let results = results.clone();
        #[cfg_attr(not(feature = "pro"), allow(unused_variables))]
        let logo_status = logo_status.clone();
        let artifacts = artifacts.clone();
        #[cfg(feature = "pro")]
        let logo = logo.clone();
        Rc::new(move |msg| match msg {
            Msg::Generate => run_generate(
                &container,
                &results,
                &artifacts,
                #[cfg(feature = "pro")]
                current_logo(&logo).as_ref(),
            ),
            Msg::Clear => {
                write_field(&container, ".qrb-data", "");
                #[cfg(feature = "pro")]
                write_field(&container, ".qrb-csv", "");
                artifacts.borrow_mut().single = None;
                #[cfg(feature = "pro")]
                artifacts.borrow_mut().batch.clear();
                results.set(Outcome::Idle);
            }
            Msg::LoadExample => {
                write_field(&container, ".qrb-data", EXAMPLE_DATA);
                run_generate(
                    &container,
                    &results,
                    &artifacts,
                    #[cfg(feature = "pro")]
                    current_logo(&logo).as_ref(),
                );
            }
            Msg::ToggleHelp => help_open.set(!help_open.get()),
            Msg::DownloadPng => {
                let borrowed = artifacts.borrow();
                if let Some(single) = &borrowed.single {
                    download_bytes(&format!("{}.png", single.stem), "image/png", &single.png);
                }
            }
            Msg::DownloadSvg => {
                let borrowed = artifacts.borrow();
                if let Some(single) = &borrowed.single {
                    download_data_uri(&format!("{}.svg", single.stem), "image/svg+xml", &single.svg);
                }
            }
            #[cfg(feature = "pro")]
            Msg::UploadCsv => {
                let container = container.clone();
                let results = results.clone();
                let artifacts = artifacts.clone();
                let logo = logo.clone();
                pick_text_file(".csv,text/csv,text/plain", Rc::new(move |text| {
                    write_field(&container, ".qrb-csv", &text);
                    refresh_batch(&container, &results, &artifacts, &text, current_logo(&logo).as_ref());
                }));
            }
            #[cfg(feature = "pro")]
            Msg::CsvChanged(text) => {
                refresh_batch(&container, &results, &artifacts, &text, current_logo(&logo).as_ref())
            }
            #[cfg(feature = "pro")]
            Msg::DownloadZip => export_zip(&container, &artifacts, &logo),
            #[cfg(feature = "pro")]
            Msg::ExportPdf => export_pdf(&container, &artifacts, &logo),
            #[cfg(feature = "pro")]
            Msg::UploadLogo => {
                let logo_status = logo_status.clone();
                let logo = logo.clone();
                let container = container.clone();
                pick_binary_file("image/png,.png", Rc::new(move |bytes| {
                    match decode_logo_png(&bytes) {
                        Ok(image) => {
                            let name = "logo.png";
                            *logo.borrow_mut() = Some((name.to_string(), image.clone()));
                            // Logos eat into error correction — force ECC H.
                            write_field(&container, ".qrb-ecc", "H");
                            logo_status.set(format!(
                                "Logo loaded ({}×{} px) — error correction set to H",
                                image.width, image.height
                            ));
                        }
                        Err(error) => {
                            logo_status.set(format!("Logo rejected: {error}"));
                        }
                    }
                }));
            }
            #[cfg(feature = "pro")]
            Msg::ClearLogo => {
                *logo.borrow_mut() = None;
                logo_status.set("No logo loaded".to_string());
            }
        })
    };

    // 1. The static shell — mounted exactly once.
    let shell = shell_tree(&logo_status);
    let root = tpt_appfront_dom::mount(container, &shell, dispatch.clone())?;
    std::mem::forget(root);

    // Ctrl+Enter regenerates from anywhere.
    let dispatch_for_keys = dispatch.clone();
    let keydown = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
        move |event: web_sys::KeyboardEvent| {
            if event.ctrl_key() && !event.shift_key() && !event.alt_key() && event.key() == "Enter"
            {
                event.prevent_default();
                dispatch_for_keys(Msg::Generate);
            }
        },
    );
    document.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
    keydown.forget();

    // 2. Reactive sections.
    mount_reactive(
        container,
        ".qrb-help-root",
        dispatch.clone(),
        move || help_view(help_open_for_view.clone()),
    )?;
    let results_for_summary = results.clone();
    mount_reactive(
        container,
        ".qrb-results",
        dispatch.clone(),
        move || results_summary_view(results_for_summary.clone()),
    )?;
    let results_for_actions = results.clone();
    mount_reactive(
        container,
        ".qrb-results-actions",
        dispatch.clone(),
        move || results_actions_view(results_for_actions.clone()),
    )?;

    // The preview <img>: mounted once into its static host, outside every
    // reactive region — regeneration only ever sets its src.
    if let Some(host) = container.query_selector(".qrb-preview-host").ok().flatten() {
        if let Ok(img) = document.create_element("img") {
            let _ = img.set_attribute("class", "qrb-preview");
            let _ = img.set_attribute("alt", "Generated code preview");
            let _ = img.set_attribute("style", "display:none");
            let _ = host.append_child(&img);
        }
    }

    // Generate the example once on first mount so the page never looks
    // empty (mirrors the hub demo expectation).
    dispatch(Msg::Generate);

    #[cfg(not(feature = "pro"))]
    std::mem::forget(dispatch);

    Ok(())
}

fn mount_reactive(
    container: &web_sys::Element,
    selector: &str,
    dispatch: Rc<dyn Fn(Msg)>,
    view: impl Fn() -> UITree<Msg> + 'static,
) -> Result<(), JsValue> {
    let element = container
        .query_selector(selector)?
        .ok_or_else(|| JsValue::from_str("reactive placeholder missing"))?;
    let handle = tpt_appfront_dom::render(&element, Rc::new(view), dispatch)?;
    std::mem::forget(handle);
    Ok(())
}

// ---------------------------------------------------------------------------
// generation + exports
// ---------------------------------------------------------------------------

fn read_symbology(container: &web_sys::Element) -> Symbology {
    Symbology::from_name(&read_input(container, ".qrb-sym")).unwrap_or(Symbology::Qr)
}

fn read_ecc(container: &web_sys::Element) -> QrEcc {
    QrEcc::from_name(&read_input(container, ".qrb-ecc")).unwrap_or(QrEcc::Medium)
}

fn read_scale(container: &web_sys::Element) -> u32 {
    read_input(container, ".qrb-scale")
        .parse::<u32>()
        .ok()
        .filter(|&v| (1..=24).contains(&v))
        .unwrap_or(8)
}

#[cfg(feature = "pro")]
fn read_colors(container: &web_sys::Element) -> (Rgb, Rgb) {
    let fg = tpt_qr_batch_engine::parse_hex_color(&read_input(container, ".qrb-fg"))
        .unwrap_or([16, 20, 24]);
    let bg = tpt_qr_batch_engine::parse_hex_color(&read_input(container, ".qrb-bg"))
        .unwrap_or([255, 255, 255]);
    (fg, bg)
}

#[cfg(not(feature = "pro"))]
fn render_options(container: &web_sys::Element) -> RenderOptions {
    RenderOptions {
        scale: read_scale(container),
        quiet_modules: if read_symbology(container) == Symbology::Qr { 4 } else { 10 },
        ..Default::default()
    }
}

#[cfg(feature = "pro")]
fn render_options(container: &web_sys::Element) -> RenderOptions {
    let (foreground, background) = read_colors(container);
    RenderOptions {
        scale: read_scale(container),
        quiet_modules: if read_symbology(container) == Symbology::Qr { 4 } else { 10 },
        foreground,
        background,
    }
}

#[cfg(feature = "pro")]
fn current_logo(logo: &Rc<RefCell<Option<(String, LogoImage)>>>) -> Option<LogoImage> {
    logo.borrow().as_ref().map(|(_, image)| image.clone())
}

#[cfg_attr(not(feature = "pro"), allow(clippy::too_many_arguments))]
fn run_generate(
    container: &web_sys::Element,
    results: &Signal<Outcome>,
    artifacts: &Rc<RefCell<Artifacts>>,
    #[cfg(feature = "pro")] logo: Option<&LogoImage>,
) {
    let data = read_input(container, ".qrb-data");
    let symbology = read_symbology(container);
    #[cfg_attr(not(feature = "pro"), allow(unused_mut))]
    let mut ecc = read_ecc(container);
    let options = render_options(container);

    // A centre logo consumes error-correction budget: force ECC High.
    #[cfg(feature = "pro")]
    let logo_forced_high = logo.is_some();
    #[cfg(feature = "pro")]
    if logo_forced_high {
        ecc = QrEcc::High;
    }
    #[cfg(not(feature = "pro"))]
    let logo_forced_high = false;
    let _ = logo_forced_high;

    let spec = CodeSpec {
        data,
        symbology,
        ecc,
        label: None,
    };
    #[cfg(feature = "pro")]
    let logo_ref = logo;
    #[cfg(not(feature = "pro"))]
    let logo_ref: Option<&LogoImage> = None;
    match encode(&spec) {
        Ok(encoded) => {
            let preview_options = RenderOptions { scale: options.scale.min(6), ..options.clone() };
            let (width, height, _) = render_rgba(&encoded, &preview_options, logo_ref);
            let png = render_png(&encoded, &preview_options, logo_ref).unwrap_or_default();
            let svg = render_svg(&encoded, &options);
            let stem = file_stem(&spec.data);

            let meta = match &encoded {
                tpt_qr_batch_engine::Encoded::Qr { size, .. } => format!(
                    "{} · {}×{} modules · ECC {}{} · preview {}×{} px",
                    symbology.display(),
                    size,
                    size,
                    spec.ecc.name(),
                    if logo_forced_high { " (forced for logo)" } else { "" },
                    width,
                    height
                ),
                tpt_qr_batch_engine::Encoded::Linear { pattern } => format!(
                    "{} · {} modules · PNG {}×{} px",
                    symbology.display(),
                    pattern.len(),
                    width,
                    height
                ),
            };

            let full_png = render_png(&encoded, &options, logo_ref).unwrap_or_default();
            artifacts.borrow_mut().single = Some(SingleArtifacts { png: full_png, svg, stem });

            set_preview_src(
                container,
                Some(&format!("data:image/png;base64,{}", base64_encode(&png))),
            );
            results.set(Outcome::Single {
                meta,
                note: String::new(),
            });
        }
        Err(error) => {
            set_preview_src(container, None);
            results.set(Outcome::Error {
                message: error.to_string(),
                hint: format!("{}.", symbology.constraint()),
            });
        }
    }
}

/// Show/hide the raw preview <img> (outside the reactive tree — the DOM
/// backend's reconciler cannot update an Image node's src).
fn set_preview_src(container: &web_sys::Element, data_url: Option<&str>) {
    if let Some(img) = container
        .query_selector(".qrb-preview-host img")
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlImageElement>().ok())
    {
        match data_url {
            Some(url) => {
                let _ = img.set_attribute("src", url);
                let _ = img.set_attribute("style", "");
            }
            None => {
                let _ = img.set_attribute("style", "display:none");
            }
        }
    }
}

#[cfg(feature = "pro")]
fn refresh_batch(
    container: &web_sys::Element,
    results: &Signal<Outcome>,
    artifacts: &Rc<RefCell<Artifacts>>,
    text: &str,
    logo: Option<&LogoImage>,
) {
    set_preview_src(container, None);
    let symbology = read_symbology(container);
    let mut ecc = read_ecc(container);
    // Logos consume error-correction budget: force ECC High.
    if logo.is_some() {
        ecc = QrEcc::High;
    }
    let specs: Vec<CodeSpec> = csv_rows(text)
        .into_iter()
        .map(|row| CodeSpec {
            data: row[0].trim().to_string(),
            symbology,
            ecc,
            label: row.get(1).map(|l| l.trim().to_string()).filter(|l| !l.is_empty()),
        })
        .collect();

    let (ok, bad) = encode_batch(&specs);
    let total = specs.len();
    artifacts.borrow_mut().batch = ok.iter().map(|(_, spec, _)| spec.clone()).collect();

    let layout = read_sheet_layout(container);
    let stats = sheet_stats(ok.len().max(1), &layout);
    let summary = if ok.is_empty() && bad.is_empty() {
        "No rows yet — paste CSV or upload a file (one code per row: data,label).".to_string()
    } else {
        format!("{total} row{} · {} code{} ready", plural(total), ok.len(), plural(ok.len()))
    };
    let sheet_summary = if ok.is_empty() {
        String::new()
    } else {
        format!(
            "{} · {} columns · {} mm codes · {} per page · {} page{}",
            layout.page.name().to_uppercase(),
            layout.columns,
            layout.cell_mm as u32,
            stats.per_page,
            stats.pages,
            plural(stats.pages),
        )
    };

    let preview_rows: Vec<Vec<String>> = ok
        .iter()
        .take(10)
        .map(|(row, spec, _)| {
            vec![
                row.to_string(),
                truncate(&spec.data, 48),
                spec.label.clone().map(|l| truncate(&l, 28)).unwrap_or_default(),
            ]
        })
        .collect();
    let more = ok.len().saturating_sub(preview_rows.len());

    results.set(Outcome::Batch {
        summary,
        errors: bad
            .iter()
            .map(|(row, error)| (*row, error.to_string()))
            .collect(),
        preview_rows,
        more,
        sheet_summary,
    });
}

#[cfg(feature = "pro")]
fn read_sheet_layout(container: &web_sys::Element) -> SheetLayout {
    SheetLayout {
        page: PageSize::from_name(&read_input(container, ".qrb-page")).unwrap_or(PageSize::A4),
        columns: read_input(container, ".qrb-cols")
            .parse::<usize>()
            .ok()
            .filter(|c| (1..=8).contains(c))
            .unwrap_or(4),
        cell_mm: read_input(container, ".qrb-cell")
            .parse::<f64>()
            .ok()
            .filter(|mm| (10.0..=80.0).contains(mm))
            .unwrap_or(30.0),
        gap_mm: read_input(container, ".qrb-gap")
            .parse::<f64>()
            .ok()
            .filter(|mm| (0.0..=20.0).contains(mm))
            .unwrap_or(4.0),
        margin_mm: 10.0,
        show_labels: checkbox_checked(container, ".qrb-labels input")
            || checkbox_checked(container, ".qrb-labels"),
        cut_guides: checkbox_checked(container, ".qrb-guides input")
            || checkbox_checked(container, ".qrb-guides"),
    }
}

#[cfg(feature = "pro")]
fn export_zip(
    container: &web_sys::Element,
    artifacts: &Rc<RefCell<Artifacts>>,
    logo: &Rc<RefCell<Option<(String, LogoImage)>>>,
) {
    let specs = artifacts.borrow().batch.clone();
    if specs.is_empty() {
        return;
    }
    let options = render_options(container);
    let logo = current_logo(logo);
    let mut files: Vec<(String, Vec<u8>)> = Vec::with_capacity(specs.len());
    for (index, spec) in specs.iter().enumerate() {
        if let Ok(encoded) = encode(spec) {
            if let Ok(png) = render_png(&encoded, &options, logo.as_ref()) {
                let name = spec
                    .label
                    .as_deref()
                    .map(file_stem)
                    .unwrap_or_else(|| format!("code-{:03}", index + 1));
                files.push((format!("{:03}-{}.png", index + 1, name), png));
            }
        }
    }
    if files.is_empty() {
        return;
    }
    let archive = stored_zip(&files);
    download_bytes("qr-batch.zip", "application/zip", &archive);
}

#[cfg(feature = "pro")]
fn export_pdf(
    container: &web_sys::Element,
    artifacts: &Rc<RefCell<Artifacts>>,
    logo: &Rc<RefCell<Option<(String, LogoImage)>>>,
) {
    let specs = artifacts.borrow().batch.clone();
    if specs.is_empty() {
        return;
    }
    let layout = read_sheet_layout(container);
    let (foreground, background) = read_colors(container);
    let entries: Vec<(CodeSpec, tpt_qr_batch_engine::Encoded)> = specs
        .iter()
        .filter_map(|spec| encode(spec).ok().map(|encoded| (spec.clone(), encoded)))
        .collect();
    let pdf = sheet_pdf(&entries, &layout, foreground, background, current_logo(logo).as_ref());
    download_bytes("qr-batch-sheets.pdf", "application/pdf", &pdf);
}

fn file_stem(text: &str) -> String {
    let cleaned: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                '\u{0}'
            }
        })
        .filter(|&c| c != '\u{0}')
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        "qr-code".to_string()
    } else {
        cleaned.chars().take(40).collect()
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

// ---------------------------------------------------------------------------
// views
// ---------------------------------------------------------------------------

fn shell_tree(logo_status: &Signal<String>) -> UITree<Msg> {
    #[cfg(not(feature = "pro"))]
    let _ = logo_status;
    let mut root = ContainerBuilder::<Msg>::new();

    root.container(|c| {
        c.container(|header| {
            header.heading(2, "QR Batch");
            header
                .button("Help & formats")
                .class("qrb-btn qrb-ghost")
                .on_click(Msg::ToggleHelp);
        })
        .class("qrb-header");

        c.container(|s| {
            s.text("Generate QR codes and retail barcodes — free, instant, entirely in your browser.");
        })
        .class("qrb-sub");

        // ---- 1 · Code content ----
        c.container(|card| {
            card.heading(3, "1 · Code content");
            card.text("Data to encode (URL, text, digits — depends on the format below).")
                .class("qrb-label");
            card.input(EXAMPLE_DATA).class("qrb-input qrb-data");

            card.container(|row| {
                row.container(|field| {
                    field.text("Format").class("qrb-label");
                    field
                        .select(
                            [
                                ("qr", "QR code"),
                                ("code128", "Code 128"),
                                ("ean13", "EAN-13"),
                                ("code39", "Code 39"),
                            ],
                            "qr",
                        )
                        .class("qrb-input qrb-sym");
                })
                .class("qrb-field");

                row.container(|field| {
                    field.text("Error correction (QR)").class("qrb-label");
                    field
                        .select(
                            [
                                ("L", "L · 7%"),
                                ("M", "M · 15%"),
                                ("Q", "Q · 25%"),
                                ("H", "H · 30%"),
                            ],
                            "M",
                        )
                        .class("qrb-input qrb-ecc");
                })
                .class("qrb-field");

                row.container(|field| {
                    field.text("PNG size").class("qrb-label");
                    field
                        .select(
                            [
                                ("4", "Small · 4 px"),
                                ("6", "Medium · 6 px"),
                                ("8", "Large · 8 px"),
                                ("12", "Print · 12 px"),
                            ],
                            "8",
                        )
                        .class("qrb-input qrb-scale");
                })
                .class("qrb-field");
            })
            .class("qrb-row");

            #[cfg(feature = "pro")]
            card.container(|row| {
                row.container(|field| {
                    field.text("Code colour").class("qrb-label");
                    field
                        .input("#0a0f14")
                        .class("qrb-color qrb-fg")
                        .attr("type", "color");
                })
                .class("qrb-field");
                row.container(|field| {
                    field.text("Background").class("qrb-label");
                    field
                        .input("#ffffff")
                        .class("qrb-color qrb-bg")
                        .attr("type", "color");
                })
                .class("qrb-field");
                row.container(|field| {
                    field.text("Centre logo (PNG)").class("qrb-label");
                    field.container(|buttons| {
                        buttons
                            .button("Upload logo…")
                            .class("qrb-btn")
                            .on_click(Msg::UploadLogo);
                        buttons
                            .button("Remove")
                            .class("qrb-btn qrb-ghost")
                            .on_click(Msg::ClearLogo);
                    })
                    .class("qrb-logo-buttons");
                })
                .class("qrb-field");
            })
            .class("qrb-row qrb-branding-row");

            card.container(|actions| {
                actions
                    .button("Generate code")
                    .class("qrb-btn qrb-primary")
                    .on_click(Msg::Generate);
                actions.button("Clear").class("qrb-btn").on_click(Msg::Clear);
                actions
                    .button("Load example")
                    .class("qrb-btn qrb-ghost")
                    .on_click(Msg::LoadExample);
            })
            .class("qrb-actions");

            #[cfg(feature = "pro")]
            card.container(|status| {
                status.container(|inner| {
                    inner.text(logo_status_snapshot(logo_status));
                })
                .class("qrb-logo-status qrb-logo-live");
            })
            .class("qrb-hint");
        })
        .class("qrb-card");

        #[cfg(feature = "pro")]
        c.container(|card| {
            card.heading(3, "2 · Batch (CSV)");
            card.text("One code per row: data,label — the label prints under the code. First column is the code content.")
                .class("qrb-hint");
            card.textarea("")
                .class("qrb-textarea qrb-csv")
                .attr("placeholder", "https://example.com/menu,Table 1\n400638133393,Product SKU")
                .on_input(|value| Msg::CsvChanged(value));
            card.container(|actions| {
                actions.button("Upload CSV…").class("qrb-btn").on_click(Msg::UploadCsv);
                actions
                    .button("Clear batch")
                    .class("qrb-btn qrb-ghost")
                    .on_click(Msg::CsvChanged(String::new()));
            })
            .class("qrb-actions");
        })
        .class("qrb-card");

        #[cfg(feature = "pro")]
        c.container(|card| {
            card.heading(3, "3 · Print sheet");
            card.container(|row| {
                row.container(|field| {
                    field.text("Page").class("qrb-label");
                    field
                        .select([("a4", "A4"), ("letter", "US Letter")], "a4")
                        .class("qrb-input qrb-page");
                })
                .class("qrb-field");
                row.container(|field| {
                    field.text("Columns").class("qrb-label");
                    field
                        .select(
                            [("2", "2"), ("3", "3"), ("4", "4"), ("5", "5"), ("6", "6")],
                            "4",
                        )
                        .class("qrb-input qrb-cols");
                })
                .class("qrb-field");
                row.container(|field| {
                    field.text("Code size (mm)").class("qrb-label");
                    field
                        .select(
                            [
                                ("25", "25 mm"),
                                ("30", "30 mm"),
                                ("35", "35 mm"),
                                ("40", "40 mm"),
                                ("45", "45 mm"),
                                ("50", "50 mm"),
                            ],
                            "30",
                        )
                        .class("qrb-input qrb-cell");
                })
                .class("qrb-field");
                row.container(|field| {
                    field.text("Gap (mm)").class("qrb-label");
                    field
                        .select([("2", "2 mm"), ("4", "4 mm"), ("6", "6 mm")], "4")
                        .class("qrb-input qrb-gap");
                })
                .class("qrb-field");
            })
            .class("qrb-row");
            card.container(|row| {
                row.checkbox("Print labels under codes", true)
                    .class("qrb-check qrb-labels");
                row.checkbox("Cut guides", true).class("qrb-check qrb-guides");
            })
            .class("qrb-row qrb-checks");
        })
        .class("qrb-card");

        c.container(|_| {}).class("qrb-help-root");
        c.container(|_| {}).class("qrb-results");
        // Raw-DOM preview host (between the summary and actions regions):
        // the DOM backend's reconciler has no src-update arm for Image
        // nodes, so the <img> lives outside the reactive tree and run_
        // generate sets its src directly (fea-lite's canvas pattern).
        c.container(|_| {}).class("qrb-preview-host");
        c.container(|_| {}).class("qrb-results-actions");

        #[cfg(not(feature = "pro"))]
        c.container(|upsell| {
            upsell.heading(3, "Need hundreds of codes?");
            upsell
                .text("TPT QR Batch Pro ($9, one-time — no subscription) adds bulk generation from a CSV spreadsheet, your logo and brand colours on every code, and print-ready label sheets. A Windows desktop app that runs offline, forever.")
                .class("qrb-upsell-copy");
            upsell
                .link(
                    "https://tptsolutions.co.nz/tools/qr-batch",
                    "Get the Pro desktop edition →",
                )
                .class("qrb-upsell-link");
        })
        .class("qrb-card qrb-upsell");
    })
    .class("qrb-app");

    root.into_only_child().expect("shell builds exactly one root container")
}

#[cfg(feature = "pro")]
fn logo_status_snapshot(logo_status: &Signal<String>) -> String {
    logo_status.get()
}

fn help_view(help_open: Signal<bool>) -> UITree<Msg> {
    // Always exactly one root container (stable shape for reconciliation);
    // hidden by class when closed.
    let open = help_open.get();
    let mut root = ContainerBuilder::<Msg>::new();
    root.container(|c| {
        if open {
            c.heading(3, "Formats & tips");
            for tip in [
                "QR code — any text or URL; up to ~1 200 characters at ECC L, fewer at higher correction levels.",
                "Code 128 — any printable ASCII text; the workhorse for shipping and inventory labels.",
                "EAN-13 — 12 digits (the check digit is computed for you) or 13 with a valid check digit.",
                "Code 39 — A-Z, 0-9, space and - . $ / + %; classic asset tags.",
                "PNG is raster (pick Print size for labels); SVG is vector and stays crisp at any size.",
                "Quiet zones (the white border around codes) follow each format's spec — don't crop them when printing.",
                "CSV format (Pro): one code per row — data,label — extra columns are ignored, a data,label header row is skipped.",
                "Shortcut: Ctrl+Enter regenerates.",
            ] {
                c.container(|line| {
                    line.text(tip);
                })
                .class("qrb-help-line");
            }
        }
    })
    .class(if open { "qrb-card qrb-help" } else { "qrb-hidden" });
    root.into_only_child().expect("help view builds one root")
}

/// Summary region: status line, batch table and error rows (the code
/// preview itself is a raw <img> in .qrb-preview-host between the two
/// reactive regions).
fn results_summary_view(results: Signal<Outcome>) -> UITree<Msg> {
    let outcome = results.get();
    let mut root = ContainerBuilder::<Msg>::new();

    // Stable three-slot shape: [status][grid][errors] — same kinds at the
    // same positions in every state so reconciliation never full-replaces.
    // (One wrapper container: into_only_child needs exactly one root child.)
    root.container(|wrapper| {
        wrapper.container(|summary| match &outcome {
        Outcome::Idle => {
            summary.text("Enter your data and press Generate — or start from the loaded example.");
        }
        Outcome::Error { message, hint } => {
            summary.text(format!("✗ {message}"));
            summary
                .container(|h| {
                    h.text(hint.clone());
                })
                .class("qrb-hint");
        }
        Outcome::Single { meta, note } => {
            summary.text(format!("✓ {meta}"));
            summary
                .container(|h| {
                    h.text(note.clone());
                })
                .class("qrb-hint");
        }
        #[cfg(feature = "pro")]
        Outcome::Batch { summary: text, sheet_summary, .. } => {
            summary.text(format!("✓ {text}"));
            summary
                .container(|h| {
                    h.text(sheet_summary.clone());
                })
                .class("qrb-hint");
        }
        })
        .class("qrb-summary");

        // Batch preview table (pro): empty container in the free/other states.
        wrapper.container(|preview| match &outcome {
        #[cfg(feature = "pro")]
        Outcome::Batch { preview_rows, more, .. } => {
            if !preview_rows.is_empty() {
                preview.data_grid(["Row", "Data", "Label"], preview_rows.clone());
                if *more > 0 {
                    preview
                        .container(|m| {
                            m.text(format!("…and {more} more codes ready"));
                        })
                        .class("qrb-hint");
                }
            }
        }
        _ => {}
        })
        .class("qrb-preview-slot");

        // Invalid-row report (pro): empty container in the free/other states.
        wrapper.container(|slot| match &outcome {
        #[cfg(feature = "pro")]
        Outcome::Batch { errors, .. } => {
            if !errors.is_empty() {
                slot.container(|bad| {
                    bad.text(format!(
                        "Skipped {} invalid row{}:",
                        errors.len(),
                        plural(errors.len())
                    ));
                    for (row, message) in errors.iter().take(8) {
                        bad.container(|line| {
                            line.text(format!("row {row}: {message}"));
                        })
                        .class("qrb-error-line");
                    }
                    if errors.len() > 8 {
                        bad.container(|line| {
                            line.text(format!("…and {} more", errors.len() - 8));
                        })
                        .class("qrb-error-line");
                    }
                })
                .class("qrb-errors");
            }
        }
        _ => {}
        })
        .class("qrb-errors-slot");
    });

    root.into_only_child().expect("results summary builds one root")
}

/// Actions region: the download buttons for whichever outcome is live.
fn results_actions_view(results: Signal<Outcome>) -> UITree<Msg> {
    let outcome = results.get();
    let mut root = ContainerBuilder::<Msg>::new();
    root.container(|actions| match &outcome {
        Outcome::Single { .. } => {
            actions
                .button("Download PNG")
                .class("qrb-btn qrb-primary")
                .on_click(Msg::DownloadPng);
            actions
                .button("Download SVG")
                .class("qrb-btn")
                .on_click(Msg::DownloadSvg);
        }
        #[cfg(feature = "pro")]
        Outcome::Batch { preview_rows, .. } => {
            if !preview_rows.is_empty() {
                actions
                    .button("Download all PNGs (ZIP)")
                    .class("qrb-btn qrb-primary")
                    .on_click(Msg::DownloadZip);
                actions
                    .button("Export print sheet (PDF)")
                    .class("qrb-btn")
                    .on_click(Msg::ExportPdf);
            }
        }
        _ => {}
    })
    .class("qrb-actions");
    root.into_only_child().expect("results actions builds one root")
}

// ---------------------------------------------------------------------------
// DOM / browser helpers (cutlist/fea-lite patterns)
// ---------------------------------------------------------------------------

fn read_input(container: &web_sys::Element, selector: &str) -> String {
    container
        .query_selector(selector)
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.value())
        .or_else(|| {
            container
                .query_selector(selector)
                .ok()
                .flatten()
                .and_then(|el| el.dyn_into::<web_sys::HtmlSelectElement>().ok())
                .map(|select| select.value())
        })
        .or_else(|| {
            container
                .query_selector(selector)
                .ok()
                .flatten()
                .and_then(|el| el.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                .map(|area| area.value())
        })
        .unwrap_or_default()
}

fn write_field(container: &web_sys::Element, selector: &str, value: &str) {
    if let Some(el) = container.query_selector(selector).ok().flatten() {
        if let Some(input) = el.dyn_ref::<web_sys::HtmlInputElement>() {
            input.set_value(value);
        } else if let Some(select) = el.dyn_ref::<web_sys::HtmlSelectElement>() {
            select.set_value(value);
        } else if let Some(area) = el.dyn_ref::<web_sys::HtmlTextAreaElement>() {
            area.set_value(value);
        }
    }
}

fn checkbox_checked(container: &web_sys::Element, selector: &str) -> bool {
    container
        .query_selector(selector)
        .ok()
        .flatten()
        .and_then(|el| el.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.checked())
        .unwrap_or(false)
}

fn upsert_style(document: &web_sys::Document, id: &str, css: &str) {
    if let Ok(Some(existing)) = document.query_selector(&format!("style#{id}")) {
        existing.set_text_content(Some(css));
        return;
    }
    let Some(head) = document.head() else { return };
    if let Ok(style_el) = document.create_element("style") {
        let _ = style_el.set_attribute("id", id);
        style_el.set_text_content(Some(css));
        let _ = head.append_child(&style_el);
    }
}

/// Binary download: Blob -> object URL -> temporary anchor click.
fn download_bytes(filename: &str, mime: &str, bytes: &[u8]) {
    let Some(window) = web_sys::window() else { return };
    let array = js_sys::Array::new();
    array.push(&js_sys::Uint8Array::from(bytes));
    let properties = web_sys::BlobPropertyBag::new();
    properties.set_type(mime);
    let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&array, &properties)
    else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    if let Some(document) = window.document() {
        if let Ok(anchor) = document.create_element("a") {
            let _ = anchor.set_attribute("href", &url);
            let _ = anchor.set_attribute("download", filename);
            let _ = anchor.dyn_ref::<web_sys::HtmlAnchorElement>().map(|a| a.click());
        }
    }
    let _ = web_sys::Url::revoke_object_url(&url);
}

/// Text download (SVG) via a percent-encoded data URI.
fn download_data_uri(filename: &str, mime: &str, content: &str) {
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Ok(anchor) = document.create_element("a") else { return };
    let uri = format!("data:{mime};charset=utf-8,{}", percent_encode(content.as_bytes()));
    let _ = anchor.set_attribute("href", &uri);
    let _ = anchor.set_attribute("download", filename);
    if let Some(body) = document.body() {
        let _ = body.append_child(&anchor);
        if let Some(link) = anchor.dyn_ref::<web_sys::HtmlAnchorElement>() {
            link.click();
        }
        let _ = body.remove_child(&anchor);
    }
}

fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Base64 (standard alphabet, padded) for the PNG preview data URL.
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// File picker returning file contents as text (cutlist's blob.text()
/// pattern — the input keeps working detached).
fn pick_text_file(accept: &str, on_text: Rc<dyn Fn(String)>) {
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Ok(input) = document.create_element("input") else { return };
    let _ = input.set_attribute("type", "file");
    let _ = input.set_attribute("accept", accept);
    let _ = input.set_attribute("style", "display:none");
    let input_el: web_sys::HtmlInputElement = input.clone().dyn_into().expect("file input");
    let on_change = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
        move |event: web_sys::Event| {
            let target =
                event.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
            let Some(file) = target
                .as_ref()
                .and_then(|i| i.files())
                .and_then(|files| files.item(0))
            else {
                return;
            };
            let blob: &web_sys::Blob = file.unchecked_ref();
            let on_text = on_text.clone();
            let on_loaded = wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(
                move |value: JsValue| on_text(value.as_string().unwrap_or_default()),
            );
            let promise = blob.text();
            if let Some(then_fn) = js_sys::Reflect::get(&promise, &JsValue::from_str("then"))
                .ok()
                .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
            {
                let _ = then_fn.call1(&promise, on_loaded.as_ref());
            }
            on_loaded.forget();
            input_el.set_value("");
        },
    );
    let _ = input.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref());
    on_change.forget();
    if let Some(body) = document.body() {
        let _ = body.append_child(&input);
    }
    if let Some(el) = input.dyn_ref::<web_sys::HtmlInputElement>() {
        el.click();
    }
    if let Some(body) = document.body() {
        let _ = body.remove_child(&input);
    }
}

/// File picker returning file contents as raw bytes (Pro logo upload).
fn pick_binary_file(accept: &str, on_bytes: Rc<dyn Fn(Vec<u8>)>) {
    let Some(window) = web_sys::window() else { return };
    let Some(document) = window.document() else { return };
    let Ok(input) = document.create_element("input") else { return };
    let _ = input.set_attribute("type", "file");
    let _ = input.set_attribute("accept", accept);
    let _ = input.set_attribute("style", "display:none");
    let input_el: web_sys::HtmlInputElement = input.clone().dyn_into().expect("file input");
    let on_change = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
        move |event: web_sys::Event| {
            let target =
                event.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
            let Some(file) = target
                .as_ref()
                .and_then(|i| i.files())
                .and_then(|files| files.item(0))
            else {
                return;
            };
            let on_bytes = on_bytes.clone();
            let reader = web_sys::FileReader::new().ok();
            let Some(reader) = reader else { return };
            let reader_for_load = reader.clone();
            let onload = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(
                move |_event: web_sys::Event| {
                    if let Ok(result) = reader_for_load.result() {
                        let bytes = js_sys::Uint8Array::new(&result).to_vec();
                        on_bytes(bytes);
                    }
                },
            );
            reader.set_onload(Some(onload.as_ref().unchecked_ref()));
            onload.forget();
            let _ = reader.read_as_array_buffer(&file);
            input_el.set_value("");
        },
    );
    let _ = input.add_event_listener_with_callback("change", on_change.as_ref().unchecked_ref());
    on_change.forget();
    if let Some(body) = document.body() {
        let _ = body.append_child(&input);
    }
    if let Some(el) = input.dyn_ref::<web_sys::HtmlInputElement>() {
        el.click();
    }
    if let Some(body) = document.body() {
        let _ = body.remove_child(&input);
    }
}

fn storage_get(key: &str) -> Option<String> {
    web_sys::window().and_then(|w| {
        w.local_storage()
            .ok()
            .flatten()
            .and_then(|storage| storage.get_item(key).ok().flatten())
    })
}

#[allow(dead_code)]
fn storage_set(key: &str, value: &str) {
    let _ = web_sys::window().and_then(|w| {
        w.local_storage()
            .ok()
            .flatten()
            .map(|storage| storage.set_item(key, value))
    });
}

// ---------------------------------------------------------------------------
// styles
// ---------------------------------------------------------------------------

const BASE_CSS: &str = r#"
.qrb-app {
  --qrb-fg: #1b2129;
  --qrb-muted: #5b6572;
  --qrb-card: #ffffff;
  --qrb-page: #f2f4f7;
  --qrb-border: #d7dce3;
  --qrb-accent: #0f6ffc;
  --qrb-accent-ink: #ffffff;
  --qrb-error: #b3261e;
  max-width: 860px;
  margin: 0 auto;
  padding: 16px;
  font: 15px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif;
  color: var(--qrb-fg);
}
@media (prefers-color-scheme: dark) {
  .qrb-app {
    --qrb-fg: #e8ebef;
    --qrb-muted: #9aa3af;
    --qrb-card: #1a2029;
    --qrb-page: #12161c;
    --qrb-border: #333c48;
    --qrb-accent: #4d94ff;
    --qrb-accent-ink: #0b1220;
    --qrb-error: #ff8a80;
  }
  .qrb-app .qrb-preview { background: #fff; }
}
.qrb-header { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 4px; }
.qrb-header h2 { margin: 0; font-size: 24px; }
.qrb-sub { color: var(--qrb-muted); margin-bottom: 16px; }
.qrb-card { background: var(--qrb-card); border: 1px solid var(--qrb-border); border-radius: 10px; padding: 16px; margin-bottom: 14px; }
.qrb-card h3 { margin: 0 0 10px; font-size: 16px; }
.qrb-label { display: block; color: var(--qrb-muted); font-size: 13px; margin-bottom: 4px; }
.qrb-hint { color: var(--qrb-muted); font-size: 13px; margin-top: 6px; }
.qrb-input, .qrb-textarea {
  width: 100%; box-sizing: border-box;
  background: var(--qrb-page); color: var(--qrb-fg);
  border: 1px solid var(--qrb-border); border-radius: 6px;
  padding: 7px 9px; font: inherit;
}
.qrb-input:focus, .qrb-textarea:focus { outline: 2px solid var(--qrb-accent); outline-offset: -1px; }
.qrb-textarea { min-height: 96px; resize: vertical; font-family: ui-monospace, Consolas, monospace; font-size: 13px; }
.qrb-row { display: flex; flex-wrap: wrap; gap: 12px; margin-top: 10px; }
.qrb-field { flex: 1 1 150px; min-width: 140px; }
.qrb-color { width: 100%; height: 36px; padding: 2px; box-sizing: border-box; background: var(--qrb-page); border: 1px solid var(--qrb-border); border-radius: 6px; }
.qrb-logo-buttons { display: flex; gap: 8px; }
.qrb-btn {
  font: inherit; cursor: pointer;
  background: var(--qrb-card); color: var(--qrb-fg);
  border: 1px solid var(--qrb-border); border-radius: 6px;
  padding: 7px 14px;
}
.qrb-btn:hover { border-color: var(--qrb-accent); }
.qrb-primary { background: var(--qrb-accent); border-color: var(--qrb-accent); color: var(--qrb-accent-ink); font-weight: 600; }
.qrb-ghost { background: transparent; }
.qrb-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
.qrb-summary { margin-bottom: 10px; font-weight: 600; }
.qrb-preview-slot { text-align: center; margin: 8px 0; }
.qrb-preview {
  image-rendering: pixelated;
  background: #fff; border: 1px solid var(--qrb-border); border-radius: 8px;
  padding: 12px; max-width: 100%; height: auto;
}
.qrb-preview-slot table { margin: 0 auto 8px; border-collapse: collapse; font-size: 13px; }
.qrb-preview-slot th, .qrb-preview-slot td { border: 1px solid var(--qrb-border); padding: 4px 10px; text-align: left; }
.qrb-error-line { color: var(--qrb-error); font-size: 13px; }
.qrb-errors { margin-top: 8px; text-align: left; }
.qrb-help .qrb-help-line { color: var(--qrb-muted); font-size: 13px; margin-bottom: 4px; }
.qrb-hidden { display: none; }
.qrb-upsell { border-color: var(--qrb-accent); }
.qrb-upsell-copy { color: var(--qrb-muted); }
.qrb-upsell-link { display: inline-block; margin-top: 8px; color: var(--qrb-accent); font-weight: 600; text-decoration: none; }
.qrb-checks { align-items: center; }
.qrb-check { display: flex; align-items: center; gap: 6px; font-size: 14px; }
.qrb-check input { accent-color: var(--qrb-accent); }
.qrb-branding-row { align-items: end; }
.qrb-logo-live { margin-top: 6px; }
"#;
