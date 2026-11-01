//! The Filter › Audio Effects › <effect>… dialogs (`photocraft_engine::cobalt_cmds`), laid out like
//! the Image as Audio app's layer sidebar instead of the generated one-row-per-parameter list, and
//! the Adjustments panel's row of audio effects with their colour icons.
//!
//! FORK-ONLY (Tomiscout/photocraft, branch `cobalt`). The dialog is routed from
//! `dialogs::filter_body`, so the filter dialog chrome (title, OK / Cancel, Alt+Reset, live canvas
//! preview, the in-dialog preview window, remembered values) stays PhotoCraft's own. Only the
//! parameter rows are drawn here:
//!
//! ```text
//! Encoding [16-bit ▾]   Sample Rate [44100 Hz]
//! Opacity  ───────●────  100 %
//! ───────────────────────────────
//! the effect's settings (group headings as in the app)
//! ```
//!
//! Compressor and Limiter get knobs and a transfer curve instead (`dynamics`).
//!
//! The values live in the dialog fields under the command's own keys, so OK, the preview and
//! recorded actions all send the same flat params.

use egui::{Color32, Vec2};
use photocraft_doc::LayerContent;
use photocraft_engine::cobalt_cmds::cobalt_core::dsp::SampleEncoding;
use photocraft_engine::cobalt_cmds::cobalt_core::effects::{EffectKind, OPACITY, ParamSpec, SAMPLE_RATE};
use photocraft_engine::cobalt_cmds::{self, ENCODING, OPACITY_KEY, SAMPLE_RATE_KEY};
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;

mod dynamics;
use crate::theme::Tokens;

/// The audio effect whose dialog has `fields`, if it is one of these.
pub fn owner(fields: &Map<String, Value>) -> Option<EffectKind> {
    fields.get("__command").and_then(Value::as_str).and_then(cobalt_cmds::kind_of)
}

fn number(f: &Map<String, Value>, key: &str, default: f32) -> f32 {
    f.get(key).and_then(Value::as_f64).map_or(default, |v| v as f32)
}

/// Places a step of `step` needs: 0.01 → 2.
fn decimals(step: f64) -> u32 {
    if step >= 1.0 { 0 } else { (-step.log10()).ceil().clamp(0.0, 4.0) as u32 }
}

fn round_to(v: f32, decimals: u32) -> f64 {
    let scale = 10f64.powi(decimals as i32);
    (f64::from(v) * scale).round() / scale
}

pub fn body(ui: &mut egui::Ui, kind: EffectKind, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let dim = |text: &str| egui::RichText::new(text).color(t.text_dim);

    // Encoding and sample rate on one line
    ui.horizontal(|ui| {
        ui.label(dim("Encoding"));
        let mut encoding = f.get(ENCODING).and_then(Value::as_str).unwrap_or(SampleEncoding::Pcm16Le.key()).to_string();
        let options: Vec<(String, &str)> = SampleEncoding::ALL.iter().map(|e| (e.key().to_string(), e.short_name())).collect();
        crate::widgets::dropdown(ui, "cobalt-encoding", "Encoding", &mut encoding, &options, 90.0);
        f.insert(ENCODING.into(), json!(encoding));
        ui.add_space(12.0);
        ui.label(dim("Sample Rate"));
        let mut rate = number(f, SAMPLE_RATE_KEY, SAMPLE_RATE.default);
        crate::widgets::value_field(ui, &mut rate, SAMPLE_RATE.min as f32..=SAMPLE_RATE.max as f32, "Hz", 84.0).accessible_name("Sample Rate");
        f.insert(SAMPLE_RATE_KEY.into(), json!(rate.round()));
    });
    ui.add_space(4.0);

    // Mixed in the signal (dry + (wet − dry) × opacity), as the app's layer opacity
    let mut opacity = number(f, OPACITY_KEY, OPACITY.default) * 100.0;
    let gestures = crate::widgets::RowGestures { reset: Some(100.0), ..Default::default() };
    crate::widgets::slider_row_with(ui, "Opacity", &mut opacity, 0.0..=100.0, "%", None, gestures);
    f.insert(OPACITY_KEY.into(), json!(round_to(opacity / 100.0, 2)));

    if !kind.params().is_empty() {
        ui.add_space(6.0);
        crate::widgets::hairline(ui);
        ui.add_space(6.0);
    }
    if dynamics::owns(kind) {
        dynamics::body(ui, kind, f);
    }
    let mut group = "";
    for spec in kind.params().iter().filter(|_| !dynamics::owns(kind)) {
        if spec.group != group {
            group = spec.group;
            if !group.is_empty() {
                ui.add_space(4.0);
                crate::widgets::section_label(ui, group);
            }
        }
        param_row(ui, f, kind, spec);
    }

    if let Some(mut preview) = f.get("__preview").and_then(Value::as_bool) {
        ui.add_space(6.0);
        crate::widgets::checkbox(ui, &mut preview, "Preview");
        f.insert("__preview".into(), json!(preview));
    }
}

/// `kind`'s colour icon (`cobalt_core::icons`), drawn untinted at `size` points.
pub fn icon(kind: EffectKind, size: f32) -> Option<egui::Image<'static>> {
    let bytes = cobalt_cmds::cobalt_core::icons::svg(kind)?;
    let source = egui::ImageSource::Bytes { uri: format!("bytes://cobalt/icons/{}.svg", kind.slug()).into(), bytes: egui::load::Bytes::Static(bytes) };
    Some(egui::Image::new(source).fit_to_exact_size(Vec2::splat(size)))
}

/// The Adjustments panel's audio row (hooked at the end of `panels::properties::adjustments_grid`):
/// one colour button per effect. Like Photoshop's adjustments, a click works on the layer without
/// destroying it: a pixel layer is converted for smart filters first (one undo step of its own),
/// then the effect's dialog opens and OK adds it as an editable smart filter.
pub fn adjustments_row(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let smart = app.session.active().and_then(|s| s.active_layer.and_then(|id| s.doc.layer(id))).is_some_and(|l| matches!(l.content, LayerContent::Smart(_)));
    let enabled = smart || crate::menus::is_enabled(app, "filter.convertForSmartFilters");
    ui.add_space(10.0);
    ui.label(egui::RichText::new(tl!("Add an audio effect (smart filter)")).color(t.text_dim));
    ui.add_space(4.0);
    let mut picked = None;
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            for kind in cobalt_cmds::effects() {
                if audio_button(ui, kind, 30.0).clicked() {
                    picked = Some(kind);
                }
            }
        });
    });
    if let Some(kind) = picked {
        if !smart && app.run("filter.convertForSmartFilters", json!({})).is_err() {
            return;
        }
        let _ = crate::menus::invoke(app, ui.ctx(), cobalt_cmds::command_id(kind), json!({}));
    }
}

/// A square button with `kind`'s colour icon (greyed when disabled) and its name as the tooltip.
fn audio_button(ui: &mut egui::Ui, kind: EffectKind, box_size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(box_size), egui::Sense::click());
    let enabled = ui.is_enabled();
    crate::icons::button_chrome(ui, rect, false, enabled && resp.hovered());
    if let Some(img) = icon(kind, (box_size * 0.6).round()) {
        let tint = if enabled { Color32::WHITE } else { Color32::from_white_alpha(90) };
        img.tint(tint).paint_at(ui, egui::Rect::from_center_size(rect.center(), Vec2::splat((box_size * 0.6).round())));
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, kind.name()));
    resp.on_hover_text(kind.name())
}

fn param_row(ui: &mut egui::Ui, f: &mut Map<String, Value>, kind: EffectKind, spec: &ParamSpec) {
    let t = Tokens::get(ui.ctx());
    let key = spec.ident(kind);
    if spec.toggle {
        let mut on = f.get(&key).and_then(Value::as_bool).unwrap_or(spec.default != 0.0);
        crate::widgets::checkbox(ui, &mut on, spec.label);
        f.insert(key, json!(on));
    } else if !spec.choices.is_empty() {
        let options: Vec<(String, &str)> = spec.choices.iter().map(|c| (cobalt_cmds::choice_value(c), *c)).collect();
        let default = options.get(spec.default as usize).map(|(v, _)| v.clone()).unwrap_or_default();
        let mut cur = f.get(&key).and_then(Value::as_str).map(str::to_string).filter(|v| options.iter().any(|(o, _)| o == v)).unwrap_or(default);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(spec.label).color(t.text_dim));
            crate::widgets::dropdown(ui, &format!("cobalt-{key}"), spec.label, &mut cur, &options, 170.0);
        });
        f.insert(key, json!(cur));
    } else {
        let (min, max) = (spec.min as f32, spec.max as f32);
        let mut v = number(f, &key, spec.default).clamp(min, max);
        let places = decimals(spec.step);
        // Wide ranges (frequencies, times) get fine steps at the low end of the track
        let curve = (spec.min > 0.0 && spec.max / spec.min >= 100.0).then_some(3.0);
        let gestures = crate::widgets::RowGestures { reset: Some(spec.default), curve, ..Default::default() };
        crate::widgets::slider_row_prec(ui, spec.label, &mut v, min..=max, "", None, gestures, Some(places));
        f.insert(key, json!(round_to(v, places)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_give_their_decimals() {
        assert_eq!([1.0, 0.1, 0.01, 0.001, 100.0].map(decimals), [0, 1, 2, 3, 0]);
    }

    /// Every value the dialog writes is one the command takes.
    #[test]
    fn fields_the_dialog_writes_parse() {
        for kind in cobalt_cmds::effects() {
            let mut f = Map::new();
            f.insert("__command".into(), json!(cobalt_cmds::command_id(kind)));
            f.insert(ENCODING.into(), json!("pcm8"));
            for spec in kind.params() {
                let v = if spec.toggle {
                    json!(true)
                } else if spec.choices.is_empty() {
                    json!(round_to(spec.default, decimals(spec.step)))
                } else {
                    json!(cobalt_cmds::choice_value(spec.choices[0]))
                };
                f.insert(spec.ident(kind), v);
            }
            assert_eq!(owner(&f), Some(kind));
            let effect = cobalt_cmds::effect_from_params(kind, &Value::Object(f)).unwrap();
            assert_eq!(effect.kind, kind);
            assert!(icon(kind, 18.0).is_some(), "{} has an icon", kind.name());
        }
    }
}
