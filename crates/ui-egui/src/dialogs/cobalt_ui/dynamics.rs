//! The Compressor and Limiter settings, laid out after Audacity's dynamics editors instead of a
//! column of sliders: knobs with a number field under each, the time controls and the level
//! controls in two groups, and the transfer curve beside them.
//!
//! ```text
//!  Attack   Release │ Threshold  Ratio      │ -36 -30 -24 -18 -12  -6   0
//!   (◔)      (◔)    │   (◔)       (◔)       │ ┌───────────────────────┐  0
//!  [3 ms]  [100 ms] │ [-12 dB]  [4 :1]      │ │              ___----──│ -6
//!                   │                       │ │         __--          │ -12
//!  Lookahead        │ Knee Width Make-up    │ │     _--               │ …
//!   (◔)             │   (◔)       (◔)       │ │ _--                   │
//!  [3 ms]           │ [6 dB]    [9 dB]      │ └───────────────────────┘
//! ```
//!
//! The knobs and the graph follow Audacity's (`KnobControl.qml`, `WarpingTransformer`,
//! `CompressionCurve.qml`): defaults at twelve o'clock, drag in any direction, -36..0 dB axes.
//!
//! FORK-ONLY (Tomiscout/photocraft, branch `cobalt`). The curve comes from `cobalt-core`'s own
//! `Compressor::output_db`, so it is the effect's characteristic, not a copy of it. The values
//! are written to the same field keys as the slider rows, so the command's params don't change.

use egui::{Align, Align2, CursorIcon, FontId, Key, Layout, Mesh, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};
use photocraft_engine::cobalt_cmds::cobalt_core::dsp::Compressor;
use photocraft_engine::cobalt_cmds::cobalt_core::effects::{EffectKind, ParamSpec};
use serde_json::{Map, Value, json};

use super::{decimals, number, param_row, round_to};
use crate::theme::Tokens;

/// Time controls, in Audacity's order: two to a row.
const TIMES: &[&str] = &["Attack", "Release", "Lookahead"];
/// Level controls; the limiter has Make-up Target where the compressor has Make-up Gain.
const LEVELS: &[&str] = &["Threshold", "Ratio", "Knee Width", "Make-up Gain", "Make-up Target"];

const KNOB: f32 = 40.0;
const CELL_W: f32 = 94.0;
const FIELD_W: f32 = 74.0;
const ROW_GAP: f32 = 10.0;
/// Two rows of label, knob and field: the height the graph matches.
const AREA_H: f32 = 2.0 * (18.0 + KNOB + 24.0 + 8.0) + ROW_GAP;
/// Pixels of drag that sweep a knob end to end (three times as many with Shift), as Audacity.
const DRAG_SPAN: f32 = 200.0;
/// The graph's dB range on both axes.
const DB_MIN: f32 = -36.0;
const GRID_DB: f32 = 6.0;
const PLOT_W: f32 = 300.0;

/// Whether `kind` gets this editor instead of slider rows.
pub fn owns(kind: EffectKind) -> bool {
    matches!(kind, EffectKind::Compressor | EffectKind::Limiter)
}

pub fn body(ui: &mut Ui, kind: EffectKind, f: &mut Map<String, Value>) {
    let specs = kind.params();
    let grouped = |names: &[&str]| -> Vec<&ParamSpec> { names.iter().filter_map(|n| specs.iter().find(|s| s.label == *n)).collect() };
    let (times, levels) = (grouped(TIMES), grouped(LEVELS));

    ui.horizontal_top(|ui| {
        knob_grid(ui, f, kind, &times);
        crate::widgets::vline(ui, AREA_H);
        knob_grid(ui, f, kind, &levels);
        ui.add_space(10.0);
        let values: Vec<f32> = specs.iter().map(|s| value_of(f, kind, s)).collect();
        if let Some(c) = curve_of(kind, &values) {
            transfer_graph(ui, &c);
        }
    });

    // Anything an `cobalt-core` update adds that this layout doesn't place yet still gets a row.
    for spec in specs.iter().filter(|s| !TIMES.contains(&s.label) && !LEVELS.contains(&s.label)) {
        param_row(ui, f, kind, spec);
    }
}

/// The value of `spec` in the fields, finite and inside its range.
fn value_of(f: &Map<String, Value>, kind: EffectKind, spec: &ParamSpec) -> f32 {
    let (min, max) = (spec.min as f32, spec.max as f32);
    let v = number(f, &spec.ident(kind), spec.default);
    if v.is_finite() { v.max(min).min(max) } else { spec.default }
}

/// The effect's processor, for its transfer curve. `values` are in `kind.params()` order; a
/// count that doesn't match (an `cobalt-core` change) draws no graph rather than reaching the
/// constructors' length check.
fn curve_of(kind: EffectKind, values: &[f32]) -> Option<Compressor> {
    match (kind, values.len()) {
        (EffectKind::Compressor, 7) => Some(Compressor::compressor(values, 44_100.0)),
        (EffectKind::Limiter, 5) => Some(Compressor::limiter(values, 44_100.0)),
        _ => None,
    }
}

/// Two knobs to a row. Plain rows rather than an `egui::Grid`, which sizes rows from the frame
/// before and settled the column below the graph.
fn knob_grid(ui: &mut Ui, f: &mut Map<String, Value>, kind: EffectKind, specs: &[&ParamSpec]) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, ROW_GAP);
        for row in specs.chunks(2) {
            ui.horizontal_top(|ui| {
                for spec in row {
                    knob_cell(ui, f, kind, spec);
                }
            });
        }
    });
}

fn unit(label: &str) -> &'static str {
    if TIMES.contains(&label) {
        "ms"
    } else if label == "Ratio" {
        ":1"
    } else {
        "dB"
    }
}

/// `Label / knob / [field]`, centred in a fixed-width column.
fn knob_cell(ui: &mut Ui, f: &mut Map<String, Value>, kind: EffectKind, spec: &ParamSpec) {
    let t = Tokens::get(ui.ctx());
    let key = spec.ident(kind);
    let places = decimals(spec.step);
    let mut v = value_of(f, kind, spec);
    ui.allocate_ui_with_layout(vec2(CELL_W, 0.0), Layout::top_down(Align::Center), |ui| {
        ui.set_width(CELL_W);
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add(egui::Label::new(egui::RichText::new(tl!(spec.label)).color(t.text_dim)).wrap_mode(egui::TextWrapMode::Extend));
        knob(ui, &key, spec, &mut v);
        crate::widgets::value_field_prec(ui, &mut v, spec.min as f32..=spec.max as f32, unit(spec.label), FIELD_W, places).accessible_name(tl!(spec.label));
    });
    f.insert(key, json!(round_to(v, places)));
}

/// Audacity's knob warping (`WarpingTransformer`): `u = (e^(C·turn) − 1) / (e^C − 1)` with `C`
/// chosen so the default value sits at twelve o'clock. Short times, low ratios and other ranges
/// whose default is far from the middle get most of the turn around the default. `None` keeps the
/// knob linear: a default at the middle or at an end, where `C` is 0 or infinite.
fn warp_c(spec: &ParamSpec) -> Option<f64> {
    let span = spec.max - spec.min;
    if span.is_nan() || span <= 0.0 {
        return None;
    }
    let m = (f64::from(spec.default) - spec.min) / span;
    (m > 0.0 && m < 1.0 && (m - 0.5).abs() > 1e-6).then(|| 2.0 * ((1.0 - m) / m).ln()).filter(|c| c.is_finite())
}

/// A value's position along the knob's turn, 0..=1.
fn to_turn(spec: &ParamSpec, v: f32) -> f32 {
    let span = spec.max - spec.min;
    if span.is_nan() || span <= 0.0 || !v.is_finite() {
        return 0.0;
    }
    let u = ((f64::from(v) - spec.min) / span).clamp(0.0, 1.0);
    let turn = match warp_c(spec) {
        Some(c) => (u * (c.exp() - 1.0) + 1.0).ln() / c,
        None => u,
    };
    turn.clamp(0.0, 1.0) as f32
}

/// The value at a position along the knob's turn.
fn from_turn(spec: &ParamSpec, turn: f32) -> f32 {
    let span = (spec.max - spec.min).max(0.0);
    let turn = if turn.is_finite() { f64::from(turn).clamp(0.0, 1.0) } else { 0.0 };
    let u = match warp_c(spec) {
        Some(c) => ((c * turn).exp() - 1.0) / (c.exp() - 1.0),
        None => turn,
    };
    (spec.min + span * u.clamp(0.0, 1.0)) as f32
}

/// One arrow-key step from `v`: a hundredth of the turn, or at least one `step` of the value, so
/// rounding can't leave a knob stuck where the warp packs values tight.
fn key_step(spec: &ParamSpec, v: f32, dir: f32) -> f32 {
    let (min, max) = (spec.min as f32, spec.max as f32);
    let places = decimals(spec.step);
    let turned = round_to(from_turn(spec, to_turn(spec, v) + dir * 0.01), places) as f32;
    let stepped = if (turned - v).abs() < spec.step as f32 * 0.5 { v + dir * spec.step as f32 } else { turned };
    stepped.max(min).min(max)
}

/// How a drag moves the turn, as Audacity's `KnobControl`: by the distance moved in any direction,
/// a third of it with Shift, up or right turning up, and 200 px for the whole turn.
fn drag_turn(delta: Vec2, fine: bool) -> f32 {
    let dist = delta.length() / if fine { 3.0 } else { 1.0 };
    let sign = if delta.y < delta.x { 1.0 } else { -1.0 };
    sign * dist / DRAG_SPAN
}

/// Audacity's rotary knob: drag in any direction (up or right turns it up, Shift is finer), the
/// mouse wheel, arrow keys when focused, double-click for the default. A 280° track, filled from
/// its start up to the value, a ring inside it and a handle pointing at the value.
fn knob(ui: &mut Ui, key: &str, spec: &ParamSpec, v: &mut f32) -> Response {
    let t = Tokens::get(ui.ctx());
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::splat(KNOB), Sense::click_and_drag());
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, enabled, tl!(spec.label)));
    // The drag's own turn, unclamped like Audacity's: pulled past an end, the knob waits there
    // until the pointer comes back. Re-deriving it from the rounded value each frame would also
    // swallow small moves where the warp packs values tight.
    let turn_id = resp.id.with(key).with("turn");

    if resp.drag_started() {
        ui.ctx().data_mut(|d| d.insert_temp(turn_id, to_turn(spec, *v)));
    }
    if resp.dragged() {
        let delta = resp.drag_delta();
        if delta != Vec2::ZERO {
            let fine = ui.input(|i| i.modifiers.shift);
            let turn = ui.ctx().data(|d| d.get_temp::<f32>(turn_id)).unwrap_or_else(|| to_turn(spec, *v)) + drag_turn(delta, fine);
            ui.ctx().data_mut(|d| d.insert_temp(turn_id, turn));
            *v = from_turn(spec, turn);
            resp.mark_changed();
        }
    }
    // The wheel turns the knob a notch at a time (PhotoCraft's own notch counting, as the
    // sliders), and isn't passed on to the dialog's scroll area.
    let notches = crate::widgets::wheel_notches_of(ui, resp.id, resp.hovered() && !resp.dragged(), 1.0);
    if resp.hovered() {
        ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
    }
    if notches != 0.0 {
        for _ in 0..notches.abs().min(50.0) as usize {
            *v = key_step(spec, *v, notches.signum());
        }
        resp.mark_changed();
    }
    if resp.double_clicked() {
        *v = spec.default;
        resp.mark_changed();
    }
    if resp.clicked() {
        resp.request_focus();
    }
    if resp.has_focus() {
        let dir = ui.input(|i| {
            (i.num_presses(Key::ArrowUp) + i.num_presses(Key::ArrowRight)) as f32 - (i.num_presses(Key::ArrowDown) + i.num_presses(Key::ArrowLeft)) as f32
        });
        if dir != 0.0 {
            *v = key_step(spec, *v, dir.signum());
            resp.mark_changed();
        }
    }
    if resp.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }

    let p = ui.painter();
    let c = rect.center();
    let r = KNOB / 2.0 - 2.0;
    let (outer_w, inner_w) = (r / 5.0, r / 8.0);
    // Angles from twelve o'clock, clockwise; ±140° are the ends of the track.
    let turn = to_turn(spec, *v);
    let angle = -140.0 + 280.0 * turn;
    let at = |deg: f32, radius: f32| {
        let a = (deg - 90.0).to_radians();
        c + vec2(a.cos(), a.sin()) * radius
    };
    let arc = |from: f32, to: f32, radius: f32| -> Vec<Pos2> {
        let n = ((to - from).abs() / 4.0).ceil().max(1.0) as usize;
        (0..=n).map(|i| at(from + (to - from) * i as f32 / n as f32, radius)).collect()
    };
    let track_r = r - outer_w / 2.0;
    p.add(Shape::line(arc(-140.0, 140.0, track_r), Stroke::new(outer_w, t.field_border)));
    if turn > 0.0 {
        let accent = if enabled { t.accent } else { t.text_faint };
        p.add(Shape::line(arc(-140.0, angle, track_r - 0.25), Stroke::new(outer_w + 0.5, accent)));
    }
    let ring_r = r - (outer_w + inner_w / 2.0);
    if resp.hovered() || resp.dragged() {
        p.circle_filled(c, ring_r, t.hover);
    }
    p.circle_stroke(c, ring_r, Stroke::new(inner_w, t.text.gamma_multiply(0.5)));
    let tip = r - outer_w - inner_w - 2.0;
    p.line_segment([at(angle, tip), at(angle, tip - r / 2.0)], Stroke::new((r / 8.0).max(2.0), t.text));
    if resp.has_focus() {
        p.circle_stroke(c, r + 2.0, Stroke::new(1.0, t.accent));
    }
    resp.on_hover_text(format!("{} (drag or scroll, Shift to fine-tune, double-click resets)", tl!(spec.label)))
}

/// Screen x or y for `db` on the graph's `DB_MIN..=0` axis across `lo..=hi`. Not clamped: the
/// curve runs on past the plot's edges and is clipped there, as Audacity's canvas does.
fn db_to(db: f32, lo: f32, hi: f32) -> f32 {
    lo + (db - DB_MIN) / -DB_MIN * (hi - lo)
}

/// The static transfer curve as Audacity draws it: input level across, output level up, both
/// -36..0 dB with a line every 6 dB, labels along the top and right, the area under the curve
/// filled.
fn transfer_graph(ui: &mut Ui, c: &Compressor) {
    let t = Tokens::get(ui.ctx());
    let small = FontId::proportional(10.5);
    let (top_gap, right_gap, side_pad) = (16.0, 28.0, 10.0);
    let size = vec2(side_pad + PLOT_W + right_gap, AREA_H);
    let (outer, resp) = ui.allocate_exact_size(size, Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, tl!("Transfer curve")));
    let g = Rect::from_min_max(pos2(outer.left() + side_pad, outer.top() + top_gap), pos2(outer.right() - right_gap, outer.bottom() - 7.0));
    let x = |db: f32| db_to(db, g.left(), g.right());
    let y = |db: f32| db_to(db, g.bottom(), g.top());
    let labels = ui.painter_at(outer.expand(1.0));
    let plot = ui.painter_at(g);

    plot.rect_filled(g, 0.0, t.histogram_bg);
    let mut db = DB_MIN;
    while db <= 0.0 {
        let grid = Stroke::new(1.0, t.separator);
        plot.line_segment([pos2(x(db), g.top()), pos2(x(db), g.bottom())], grid);
        plot.line_segment([pos2(g.left(), y(db)), pos2(g.right(), y(db))], grid);
        let label = format!("{db:.0}");
        labels.text(pos2(x(db), g.top() - 4.0), Align2::CENTER_BOTTOM, &label, small.clone(), t.text_dim);
        labels.text(pos2(g.right() + 5.0, y(db)), Align2::LEFT_CENTER, &label, small.clone(), t.text_dim);
        db += GRID_DB;
    }

    // One point per pixel column, so the knee is a smooth bend at any size
    let steps = g.width().ceil().max(2.0) as usize;
    let pts: Vec<Pos2> = (0..=steps)
        .map(|i| {
            let db_in = DB_MIN + -DB_MIN * i as f32 / steps as f32;
            let out = c.output_db(db_in);
            // Far off the plot is as good as infinitely far, and keeps the mesh finite
            let out = if out.is_finite() { out.clamp(4.0 * DB_MIN, -4.0 * DB_MIN) } else { 4.0 * DB_MIN };
            pos2(x(db_in), y(out))
        })
        .collect();
    let fill = t.accent.gamma_multiply(0.22);
    let mut mesh = Mesh::default();
    for w in pts.windows(2) {
        let [a, b] = [w[0], w[1]];
        let i = mesh.vertices.len() as u32;
        for q in [a, b, pos2(b.x, g.bottom()), pos2(a.x, g.bottom())] {
            mesh.colored_vertex(q, fill);
        }
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i, i + 2, i + 3);
    }
    plot.add(Shape::mesh(mesh));
    plot.add(Shape::line(pts, Stroke::new(1.5, t.accent)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: EffectKind, label: &str) -> ParamSpec {
        *kind.params().iter().find(|s| s.label == label).unwrap()
    }

    #[test]
    fn every_dynamics_setting_has_a_knob() {
        for kind in [EffectKind::Compressor, EffectKind::Limiter] {
            for s in kind.params() {
                assert!(TIMES.contains(&s.label) || LEVELS.contains(&s.label), "{} {} has no place", kind.name(), s.label);
            }
            assert!(curve_of(kind, &kind.params().iter().map(|s| s.default).collect::<Vec<_>>()).is_some());
        }
        assert!(curve_of(EffectKind::Compressor, &[0.0; 3]).is_none(), "a wrong count draws nothing instead of panicking");
    }

    #[test]
    fn turns_round_trip_and_clamp() {
        for kind in [EffectKind::Compressor, EffectKind::Limiter] {
            for s in kind.params() {
                for v in [s.min as f32, s.default, s.max as f32] {
                    assert!((from_turn(s, to_turn(s, v)) - v).abs() < 1e-3 * (s.max - s.min) as f32, "{} {v}", s.label);
                }
                // As in Audacity, every default sits at twelve o'clock
                assert!((to_turn(s, s.default) - 0.5).abs() < 1e-4, "{} default at {}", s.label, to_turn(s, s.default));
                assert_eq!(to_turn(s, f32::NAN), 0.0);
                assert_eq!(to_turn(s, s.max as f32 * 10.0 + 10.0), 1.0);
                assert_eq!(from_turn(s, f32::INFINITY), s.min as f32);
                assert_eq!(from_turn(s, 2.0), s.max as f32);
            }
        }
        // A default at the middle or an end leaves the knob linear instead of dividing by zero
        let mut s = spec(EffectKind::Compressor, "Make-up Gain");
        s.default = 0.0;
        assert!(warp_c(&s).is_none());
        assert_eq!(from_turn(&s, 0.25), -15.0);
        s.default = s.min as f32;
        assert!(warp_c(&s).is_none());
    }

    #[test]
    fn drags_turn_like_audacity() {
        assert_eq!(drag_turn(vec2(0.0, -200.0), false), 1.0);
        assert_eq!(drag_turn(vec2(200.0, 0.0), false), 1.0);
        assert_eq!(drag_turn(vec2(0.0, 200.0), false), -1.0);
        assert_eq!(drag_turn(vec2(-200.0, 0.0), false), -1.0);
        assert!((drag_turn(vec2(0.0, -60.0), true) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn arrow_keys_never_stick_at_zero() {
        let attack = spec(EffectKind::Compressor, "Attack");
        let up = key_step(&attack, 0.0, 1.0);
        assert!(up > 0.0);
        assert_eq!(key_step(&attack, 0.0, -1.0), 0.0);
        assert_eq!(key_step(&attack, attack.max as f32, 1.0), attack.max as f32);
    }

    #[test]
    fn graph_axis_maps_without_clamping() {
        assert_eq!(db_to(-36.0, 0.0, 100.0), 0.0);
        assert_eq!(db_to(0.0, 0.0, 100.0), 100.0);
        assert_eq!(db_to(-18.0, 100.0, 0.0), 50.0);
        assert!(db_to(6.0, 0.0, 100.0) > 100.0, "the curve runs past the top and is clipped");
    }

    /// The editor draws without panicking and keeps every value in range, even from garbage.
    #[test]
    fn draws_from_bad_fields() {
        for kind in [EffectKind::Compressor, EffectKind::Limiter] {
            let mut f = Map::new();
            for s in kind.params() {
                f.insert(s.ident(kind), json!(-1e9));
            }
            let mut h = egui_kittest::Harness::new_ui_state(|ui, f: &mut Map<String, Value>| body(ui, kind, f), f);
            h.run();
            let f = h.state().clone();
            for s in kind.params() {
                let v = f.get(&s.ident(kind)).and_then(Value::as_f64).unwrap();
                assert!(v >= s.min && v <= s.max, "{} {v}", s.label);
            }
        }
    }

    /// Dragging a knob up raises its value.
    #[test]
    fn dragging_a_knob_up_raises_it() {
        use egui_kittest::kittest::Queryable;
        let kind = EffectKind::Compressor;
        let key = spec(kind, "Threshold").ident(kind);
        let mut h = egui_kittest::Harness::new_ui_state(|ui, f: &mut Map<String, Value>| body(ui, kind, f), Map::new());
        h.run();
        let before = h.state().get(&key).and_then(Value::as_f64).unwrap();
        let knob = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Threshold");
        let from = knob.rect().center();
        let from = egui::pos2(from.x, from.y);
        h.drag_at(from);
        h.run();
        h.hover_at(from - vec2(0.0, 40.0));
        h.run();
        h.drop_at(from - vec2(0.0, 40.0));
        h.run();
        let after = h.state().get(&key).and_then(Value::as_f64).unwrap();
        assert!(after > before + 5.0, "{before} → {after}");
    }
}
