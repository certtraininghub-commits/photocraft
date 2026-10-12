//! JPEG Options: the quality File › Save As and Save a Copy ask for before writing a JPEG, as
//! Photoshop's JPEG Options does. Quality runs 0–12 with the Low, Medium, High and Maximum
//! presets; 7 and up keep full colour detail (4:4:4, no chroma subsampling) as Photoshop does,
//! and below that colour is stored at half resolution (4:2:0) for a smaller file. The last
//! quality chosen is remembered (Preferences › Export, `jpegSaveQuality`).
//!
//! Only a name picked in the save dialog asks: a script or control call that passes a path
//! saves at once, and one that passes `jpegQuality` (0–12) uses it. The save is parked in
//! [`Prompt`] while the modal is up (egui is immediate-mode) and written once answered.

use crate::{ExportSettings, PhotocraftApp};

/// Encoder quality (1–100) for each step of Photoshop's 0–12 scale.
const ENCODER_QUALITY: [u8; 13] = [20, 30, 40, 50, 56, 62, 68, 74, 80, 86, 92, 96, 100];

/// The presets of Photoshop's Quality menu.
const PRESETS: [(u8, &str); 4] = [(3, "Low"), (5, "Medium"), (8, "High"), (10, "Maximum")];

/// A save waiting on the JPEG Options answer.
pub struct Prompt {
    /// The document that requested the save, even if tabs change while this is open.
    doc: photocraft_doc::DocId,
    /// Where the document goes.
    pub path: String,
    /// Quality 0–12.
    pub quality: u8,
    /// Save a Copy: the document keeps its path and unsaved changes.
    pub copy: bool,
    /// File › Save As: the answered save is recorded as a `file.saveAs` step (#2032).
    pub record: bool,
}

/// `true` when `path` names a JPEG (`.jpg`, `.jpeg`, `.jpe`).
pub fn is_jpeg_path(path: &str) -> bool {
    photocraft_engine::file_cmds::extension(path).and_then(|e| photocraft_codecs::from_extension(&e)) == Some(photocraft_codecs::Format::Jpeg)
}

/// The encoder settings for quality `q` (0–12): (encoder quality 1–100, 4:2:0 chroma subsampling).
pub fn encoder(q: u8) -> (u8, bool) {
    let q = q.min(12);
    (ENCODER_QUALITY[q as usize], q < 7)
}

/// The quality's preset name: Low (0–4), Medium (5–7), High (8–9) or Maximum (10–12).
pub fn preset_name(q: u8) -> &'static str {
    match q {
        0..=4 => "Low",
        5..=7 => "Medium",
        8..=9 => "High",
        _ => "Maximum",
    }
}

/// What a save at quality `q` writes, on top of the usual save settings.
pub fn settings(app: &PhotocraftApp, q: u8) -> ExportSettings {
    let (quality, subsample) = encoder(q);
    ExportSettings { jpeg_quality: Some(quality), jpeg_subsampling: Some(subsample), ..app.save_settings() }
}

/// The JPEG quality a command or control call asks for (`"jpegQuality": 0–12`), which skips the prompt.
pub fn quality_param(params: &serde_json::Value) -> Result<Option<u8>, String> {
    match params.get("jpegQuality") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(q @ 0..=12) => Ok(Some(q as u8)),
            _ => Err(format!("jpegQuality must be a whole number from 0 to 12, not {v}")),
        },
    }
}

/// The quality the prompt starts at: the last one chosen.
pub fn remembered(app: &PhotocraftApp) -> u8 {
    app.session.prefs().export.jpeg_save_quality.min(12) as u8
}

/// Parks the save behind the prompt. A prompt already waiting is never replaced.
pub fn park(app: &mut PhotocraftApp, path: String, copy: bool, record: bool) -> Result<(), String> {
    if app.save_options_open() {
        return Err("Answer the save options before starting another save".into());
    }
    let doc = app.active_doc_id()?;
    let quality = remembered(app);
    app.jpeg_options = Some(Prompt { doc, path, quality, copy, record });
    Ok(())
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(p) = &mut app.jpeg_options else { return };
    let (mut ok, mut cancel) = (false, false);
    let mut q = p.quality;
    let modal = egui::Modal::new(egui::Id::new("jpeg-options")).show(ctx, |ui| {
        ui.set_max_width(380.0);
        ui.label(egui::RichText::new(tl!("JPEG Options")).font(crate::theme::semibold(15.0)));
        ui.add_space(4.0);
        crate::widgets::hairline(ui);
        ui.add_space(8.0);
        crate::widgets::section_label(ui, tl!("Image Options"));
        ui.horizontal(|ui| {
            ui.label(tl!("Quality"));
            let mut steps = q as u32;
            if ui.add(egui::DragValue::new(&mut steps).range(0..=12).speed(0.1)).changed() {
                q = steps.min(12) as u8;
            }
            let mut preset = PRESETS.iter().rev().find(|(v, _)| q >= *v).map_or(PRESETS[0].0, |(v, _)| *v);
            let shown = preset;
            let options: Vec<(u8, &str)> = PRESETS.to_vec();
            if crate::widgets::dropdown(ui, "jpeg-quality-preset", &mut preset, &options, 110.0) && preset != shown {
                q = preset;
            }
        });
        let mut slider = q as f32;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!("small file")).small());
            if ui.add(egui::Slider::new(&mut slider, 0.0..=12.0).step_by(1.0).show_value(false)).changed() {
                q = slider.round().clamp(0.0, 12.0) as u8;
            }
            ui.label(egui::RichText::new(tl!("large file")).small());
        });
        ui.add_space(6.0);
        let note = if q >= 7 { tl!("Full color detail (4:4:4).") } else { tl!("Color stored at half size (4:2:0) for a smaller file.") };
        ui.label(egui::RichText::new(format!("{} · {}", tl!(preset_name(q)), note)).small());
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ok = crate::widgets::primary_button(ui, tl!("OK"), 84.0).clicked();
            cancel = crate::widgets::secondary_button(ui, tl!("Cancel"), 84.0).clicked();
        });
    });
    cancel |= modal.should_close();
    p.quality = q;
    if ok || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        confirm(app, ctx);
    } else if cancel {
        app.jpeg_options = None;
        app.ui.status = tl!("Save cancelled").into();
        app.ui.status_error = false;
    }
}

/// OK in JPEG Options: remembers the quality and writes the parked save.
pub(crate) fn confirm(app: &mut PhotocraftApp, ctx: &egui::Context) {
    {
        let Some(p) = app.jpeg_options.take() else { return };
        app.session.edit_prefs(|prefs| prefs.export.jpeg_save_quality = p.quality as u32);
        let settings = settings(app, p.quality);
        let step = serde_json::json!({"path": p.path, "jpegQuality": p.quality});
        let written = app.with_document(p.doc, |app| app.write_document(p.path, &settings, p.copy));
        if written.is_ok() && p.record {
            // An action being recorded keeps the Save As with this answer (#2032).
            app.session.journal.push(("file.saveAs".into(), step));
        }
        match written {
            Err(e) => {
                app.ui.status = crate::i18n::fmt(tl!("Save failed: {error}"), &[("error", &e)]);
                app.ui.status_error = true;
            }
            Ok(Some(_)) if !p.copy => crate::discard_ui::saved_document(app, ctx, p.doc),
            // A background save releases the close prompt when it finishes (`jobs_ui`).
            Ok(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_names() {
        assert!(is_jpeg_path("a.jpg"));
        assert!(is_jpeg_path("dir/sub/a.JPEG"));
        assert!(is_jpeg_path("C:\\x\\y.jpg"));
        assert!(!is_jpeg_path("a.jpg.psd"));
        assert!(!is_jpeg_path(".jpg"));
        assert!(!is_jpeg_path("a.png"));
    }

    #[test]
    fn quality_scale_matches_photoshop() {
        assert_eq!(encoder(12), (100, false));
        assert_eq!(encoder(10), (92, false));
        assert_eq!(encoder(7), (74, false), "7 and up keep full colour detail");
        assert_eq!(encoder(6), (68, true));
        assert_eq!(encoder(0), (20, true));
        assert_eq!(encoder(40), (100, false), "clamped to 12");
        for q in 1..=12u8 {
            assert!(encoder(q).0 > encoder(q - 1).0, "quality rises with every step");
        }
        assert_eq!(preset_name(3), "Low");
        assert_eq!(preset_name(5), "Medium");
        assert_eq!(preset_name(8), "High");
        assert_eq!(preset_name(10), "Maximum");
        assert_eq!(preset_name(12), "Maximum");
    }

    #[test]
    fn quality_params() {
        use serde_json::json;
        assert_eq!(quality_param(&json!({})), Ok(None));
        assert_eq!(quality_param(&json!({"jpegQuality": null})), Ok(None));
        assert_eq!(quality_param(&json!({"jpegQuality": 0})), Ok(Some(0)));
        assert_eq!(quality_param(&json!({"jpegQuality": 12})), Ok(Some(12)));
        for bad in [json!(13), json!(-1), json!("8"), json!(8.5), json!(true)] {
            assert!(quality_param(&json!({"jpegQuality": bad})).is_err(), "{bad}");
        }
    }
}
