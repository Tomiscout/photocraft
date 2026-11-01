//! Filter › Audio Effects › <effect>…: Cobalt's ("cobalt") audio effects as built-in
//! filters, one command per effect (`filter.audio.wahwah`, `filter.audio.reverb`, …).
//!
//! FORK-ONLY (Tomiscout/photocraft, branch `cobalt`); upstream never sees this file. The effects
//! themselves are not here: they are the `cobalt-core` crate of the Cobalt app
//! (`../cobalt/crates/cobalt-core`, a path dependency), the same code the app runs. This
//! module only maps command params to an `cobalt_core::effects::EffectInstance` and runs
//! `cobalt_core::dsp::apply_effect` over the layer, natively and on all cores. The commands are
//! generated from `EffectKind::ALL`, so an effect added to the app shows up here by itself. How it
//! is hooked into PhotoCraft: `.claude/skills/cobalt-fork/SKILL.md`.
//!
//! The layer's colour channels over the canvas are read as 8-bit bytes in row order (`R,G,B,R,G,B…`
//! for RGB, without alpha), exactly the byte stream the app reads from an image, and turned into
//! one mono signal. Alpha is never touched. Bytes the effect leaves as they were keep their
//! original value (16-bit and 32-bit documents keep their precision there). The selection mixes
//! the result in like built-in filters; the effect still runs over the whole canvas, so the
//! signal is the same with or without one. On a smart object the command records a smart filter
//! (what the Adjustments panel's audio row makes, see `ui-egui/src/dialogs/cobalt_ui.rs`).

use std::sync::LazyLock;

/// The app's effect crate, for the dialog and icons (`ui-egui/src/dialogs/cobalt_ui.rs`).
pub use cobalt_core;

use cobalt_core::dsp::SampleEncoding;
use cobalt_core::effects::{EffectInstance, EffectKind, OPACITY, ParamSpec, SAMPLE_RATE, slug};
use photocraft_doc::{LayerContent, SmartFilter};
use photocraft_geom::{Rect, TILE_SIZE};
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

/// Every command id is this plus the effect's slug (also what smart filters are recorded under).
pub const PREFIX: &str = "filter.audio.";
/// Param keys besides each effect's own (`ParamSpec::ident`, e.g. `reverbRoomSize`).
pub const ENCODING: &str = "encoding";
pub const SAMPLE_RATE_KEY: &str = "sampleRate";
pub const OPACITY_KEY: &str = "opacity";

/// The audio effects, in the app's order (the image adjustments are left out: PhotoCraft has them).
pub fn effects() -> impl Iterator<Item = EffectKind> {
    EffectKind::ALL.into_iter().filter(|k| !k.is_image())
}

/// An option's value in params: its slug, as on the app's command line ("Bouncing Ball" ->
/// "bouncingball").
pub fn choice_value(name: &str) -> String {
    slug(name)
}

/// One effect's command: its id, menu label and params notation (owned here, borrowed as
/// `&'static str` by its `CommandSpec`).
struct Entry {
    kind: EffectKind,
    id: String,
    label: String,
    params: String,
}

static ENTRIES: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    effects().map(|kind| Entry { kind, id: format!("{PREFIX}{}", kind.slug()), label: format!("{}…", kind.name()), params: params_notation(kind) }).collect()
});

/// The command id of `kind` (`filter.audio.reverb`).
pub fn command_id(kind: EffectKind) -> &'static str {
    ENTRIES.iter().find(|e| e.kind == kind).map_or("", |e| e.id.as_str())
}

/// The audio effect `command` runs, if it is one of these.
pub fn kind_of(command: &str) -> Option<EffectKind> {
    ENTRIES.iter().find(|e| e.id == command).map(|e| e.kind)
}

/// One parameter in the registry's notation.
fn notation(key: &str, spec: &ParamSpec) -> String {
    if spec.toggle {
        format!("\"{key}\":bool={}", spec.default != 0.0)
    } else if !spec.choices.is_empty() {
        let options: Vec<String> = spec.choices.iter().map(|c| choice_value(c)).collect();
        let default = options.get(spec.default as usize).cloned().unwrap_or_default();
        format!("\"{key}\":\"{}\"={default}", options.join("|"))
    } else {
        format!("\"{key}\":{}..{}={}", spec.min, spec.max, spec.default)
    }
}

/// `kind`'s params notation, generated from the effect tables so it follows the app.
fn params_notation(kind: EffectKind) -> String {
    let encodings: Vec<&str> = SampleEncoding::ALL.iter().map(|e| e.key()).collect();
    let mut parts = vec![
        format!("\"{ENCODING}\":\"{}\"={}", encodings.join("|"), SampleEncoding::Pcm16Le.key()),
        notation(SAMPLE_RATE_KEY, &SAMPLE_RATE),
        notation(OPACITY_KEY, &OPACITY),
    ];
    parts.extend(kind.params().iter().map(|spec| notation(&spec.ident(kind), spec)));
    format!("{{{}}} (Cobalt: the colour bytes read as a mono signal)", parts.join(","))
}

fn bad(kind: EffectKind, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: command_id(kind).into(), msg: msg.into() }
}

/// The `kind` effect `p` describes. Missing keys take the app's defaults; numbers are clamped to
/// the app's ranges; anything of the wrong type or an unknown option is an error.
pub fn effect_from_params(kind: EffectKind, p: &Value) -> Result<EffectInstance> {
    let bad = |msg: String| bad(kind, msg);
    let mut effect = EffectInstance::new(kind);
    let p = match p {
        Value::Object(m) => m,
        Value::Null => return Ok(effect),
        _ => return Err(bad("params must be an object".into())),
    };
    if let Some(v) = p.get(ENCODING) {
        let key = v.as_str().ok_or_else(|| bad("`encoding` must be a string".into()))?;
        effect.encoding = SampleEncoding::ALL.into_iter().find(|e| e.key() == key).ok_or_else(|| bad(format!("unknown encoding {key:?}")))?;
    }
    let number = |key: &str, spec: &ParamSpec| -> Result<Option<f32>> {
        match p.get(key) {
            None => Ok(None),
            Some(v) => {
                let n = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| bad(format!("`{key}` must be a number")))?;
                Ok(Some(n.clamp(spec.min, spec.max) as f32))
            }
        }
    };
    if let Some(rate) = number(SAMPLE_RATE_KEY, &SAMPLE_RATE)? {
        effect.sample_rate = rate;
    }
    if let Some(opacity) = number(OPACITY_KEY, &OPACITY)? {
        effect.opacity = opacity;
    }
    for (value, spec) in effect.values.iter_mut().zip(kind.params()) {
        let key = spec.ident(kind);
        let Some(v) = p.get(&key) else { continue };
        *value = if spec.toggle {
            match v {
                Value::Bool(on) => f32::from(u8::from(*on)),
                _ => return Err(bad(format!("`{key}` must be true or false"))),
            }
        } else if !spec.choices.is_empty() {
            let name = v.as_str().ok_or_else(|| bad(format!("`{key}` must be a string")))?;
            let i = spec.choices.iter().position(|c| choice_value(c) == slug(name)).ok_or_else(|| bad(format!("unknown `{key}` {name:?}")))?;
            i as f32
        } else {
            match number(&key, spec)? {
                Some(n) => n,
                None => continue,
            }
        };
    }
    Ok(effect)
}

/// Bytes a run may read (the canvas's colour channels at one byte each): 2 GiB.
const MAX_BYTES: usize = 2 << 30;

/// Runs `effect` over the colour channels of `surf` inside `canvas` and returns the new surface;
/// `selection` (coverage in channel 0) mixes the result in.
pub fn render(surf: &Surface, canvas: Rect, selection: Option<&Surface>, effect: &EffectInstance) -> Result<Surface> {
    let fmt = surf.format();
    let n = fmt.channels();
    let colours = n.saturating_sub(usize::from(fmt.alpha));
    if canvas.is_empty() || colours == 0 {
        return Ok(surf.clone());
    }
    let (w, h) = (canvas.width() as usize, canvas.height() as usize);
    let len = w
        .checked_mul(h)
        .and_then(|px| px.checked_mul(colours))
        .filter(|&len| len <= MAX_BYTES)
        .ok_or_else(|| EngineError::Other("the canvas is too large for the audio effects (more than 2 GiB of colour bytes)".into()))?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).map_err(|e| EngineError::Other(format!("not enough memory for the audio effects ({e})")))?;
    // Tile-row strips keep the float copy small; the signal itself is the whole canvas.
    let strips = || (canvas.y0..canvas.y1).step_by(TILE_SIZE as usize).map(|y| Rect::new(canvas.x0, y, canvas.x1, (y + TILE_SIZE).min(canvas.y1)));
    let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    for strip in strips() {
        for px in surf.read_region(strip).chunks_exact(n) {
            bytes.extend(px.iter().take(colours).map(|&v| to_byte(v)));
        }
    }
    let processed = cobalt_core::dsp::apply_effect(&bytes, effect);

    let mut out = surf.clone();
    let mut at = 0usize;
    for strip in strips() {
        let mut data = surf.read_region(strip);
        let coverage: Option<Vec<f32>> = selection.map(|s| {
            let sn = s.format().channels().max(1);
            s.read_region(strip).chunks_exact(sn).map(|p| p.first().copied().unwrap_or(0.0).clamp(0.0, 1.0)).collect()
        });
        for (i, px) in data.chunks_exact_mut(n).enumerate() {
            let k = coverage.as_ref().map_or(1.0, |c| c.get(i).copied().unwrap_or(0.0));
            for v in px.iter_mut().take(colours) {
                let (Some(&before), Some(&after)) = (bytes.get(at), processed.get(at)) else { break };
                at += 1;
                if after != before {
                    let new = f32::from(after) / 255.0;
                    *v += (new - *v) * k;
                }
            }
        }
        out.write_region_par(strip, &data);
    }
    out.prune();
    Ok(out)
}

fn run(s: &mut Session, kind: EffectKind, p: &Value) -> Result<Value> {
    // Bad parameters fail before anything is touched.
    let effect = effect_from_params(kind, p)?;
    let layer = crate::commands::layer_param(s, p)?;
    // What a smart filter records: the resolved values, so it re-renders the same.
    let recorded = params_of(&effect);
    s.edit(kind.name(), |doc, _| {
        let selection = doc.selection.clone();
        let canvas = doc.bounds();
        if let Some(surf) = crate::channel_cmds::channel_surface_for_filter(doc, Some(layer), p)? {
            *surf = render(surf, canvas, selection.as_ref(), &effect)?;
            return Ok(());
        }
        crate::commands::check_pixels_unlocked(doc, layer)?;
        let l = doc.layer_mut(layer).ok_or(EngineError::NoLayer(layer))?;
        match &mut l.content {
            // Alpha is never written, so Lock Transparency holds by itself.
            LayerContent::Raster(surf) => {
                *surf = render(surf, canvas, selection.as_ref(), &effect)?;
                Ok(())
            }
            LayerContent::Smart(_) => {
                let sf =
                    SmartFilter { command: command_id(kind).into(), params: recorded, blend: photocraft_color::BlendMode::Normal, opacity: 1.0, visible: true };
                crate::smart_cmds::add_smart_filter(doc, layer, sf, selection.as_ref())
            }
            other => Err(EngineError::Other(format!("audio effects need a pixel layer (active layer is {} {} layer)", other.article(), other.kind_name()))),
        }
    })?;
    Ok(json!({"layer": layer.0, "effect": kind.slug()}))
}

/// `CommandSpec::run` is a plain `fn`, so each effect gets one by its index in [`effects`].
fn run_nth<const N: usize>(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = effects().nth(N).ok_or_else(|| EngineError::Other("no such audio effect".into()))?;
    run(s, kind, p)
}

/// One per effect; more than the app has (a test checks), so a new effect needs no change here.
const RUNS: [fn(&mut Session, &Value) -> Result<Value>; 16] = [
    run_nth::<0>,
    run_nth::<1>,
    run_nth::<2>,
    run_nth::<3>,
    run_nth::<4>,
    run_nth::<5>,
    run_nth::<6>,
    run_nth::<7>,
    run_nth::<8>,
    run_nth::<9>,
    run_nth::<10>,
    run_nth::<11>,
    run_nth::<12>,
    run_nth::<13>,
    run_nth::<14>,
    run_nth::<15>,
];

/// `effect` as command params (every key of its effect, so nothing depends on defaults later).
pub fn params_of(effect: &EffectInstance) -> Value {
    let mut m = serde_json::Map::new();
    m.insert(ENCODING.into(), json!(effect.encoding.key()));
    m.insert(SAMPLE_RATE_KEY.into(), json!(effect.sample_rate));
    m.insert(OPACITY_KEY.into(), json!(effect.opacity));
    for (spec, &v) in effect.kind.params().iter().zip(&effect.values) {
        let value = if spec.toggle {
            json!(v != 0.0)
        } else if !spec.choices.is_empty() {
            json!(spec.choices.get(v as usize).map(|c| choice_value(c)).unwrap_or_default())
        } else {
            json!(v)
        };
        m.insert(spec.ident(effect.kind), value);
    }
    Value::Object(m)
}

/// Smart-filter re-render (hooked into `filters::apply_filter_to_surface`). `None` for other
/// commands or params that no longer parse (the smart filter is then skipped, as unknown ones are).
pub fn apply_to_surface(command: &str, params: &Value, surf: &Surface, selection: Option<&Surface>, canvas: Rect) -> Option<Surface> {
    let effect = effect_from_params(kind_of(command)?, params).ok()?;
    render(surf, canvas, selection, &effect).ok()
}

pub fn specs() -> Vec<CommandSpec> {
    ENTRIES
        .iter()
        .zip(RUNS)
        .map(|(e, run)| CommandSpec {
            id: e.id.as_str(),
            label: e.label.as_str(),
            menu: &["Filter", "Audio Effects"],
            shortcut: None,
            params: e.params.as_str(),
            enabled: crate::filters::has_filterable_layer,
            run,
            journal: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::PixelFormat;

    fn rgba_gradient(w: i32, h: i32) -> Surface {
        let mut s = Surface::new(PixelFormat::RGBA8);
        let r = Rect::new(0, 0, w, h);
        let data: Vec<f32> = (0..w * h).flat_map(|i| [(i % 251) as f32 / 255.0, (i % 13) as f32 / 255.0, 0.5, 0.25]).collect();
        s.write_region(r, &data);
        s
    }

    #[test]
    fn params_round_trip_and_reject_bad_input() {
        for kind in effects() {
            let effect = EffectInstance::new(kind);
            assert_eq!(effect_from_params(kind, &params_of(&effect)).unwrap(), effect, "{}", kind.name());
        }
        let e = effect_from_params(EffectKind::Reverb, &json!({"reverbRoomSize": 9.0, "encoding": "mulaw", "delayDelayType": "bouncingball"})).unwrap();
        assert_eq!((e.kind, e.values[0], e.encoding), (EffectKind::Reverb, 1.0, SampleEncoding::MuLaw));
        for p in [json!({"encoding": 3}), json!({"encoding": "pcm24"}), json!({"wahWahDepth": "x"}), json!({"wahWahDepth": f64::NAN}), json!([1])] {
            assert!(effect_from_params(EffectKind::WahWah, &p).is_err(), "{p}");
        }
    }

    /// One command per audio effect, each running its own effect, all under Filter › Audio Effects.
    #[test]
    fn every_effect_has_its_own_command() {
        let specs = specs();
        assert!(effects().count() <= RUNS.len(), "add entries to RUNS");
        assert_eq!(specs.len(), effects().count());
        for (spec, kind) in specs.iter().zip(effects()) {
            assert_eq!(kind_of(spec.id), Some(kind));
            assert_eq!(spec.id, command_id(kind));
            assert_eq!(spec.label, format!("{}…", kind.name()));
            assert_eq!(spec.menu, ["Filter", "Audio Effects"]);
        }
        assert_eq!(command_id(EffectKind::Reverb), "filter.audio.reverb");
        assert_eq!(kind_of("filter.audio.levels"), None);
        let reverb = &specs[1].params;
        assert!(reverb.contains("\"reverbRoomSize\":0..1=0.6"), "{reverb}");
        assert!(!reverb.contains("wahWah"), "{reverb}");
        let tremolo = specs.iter().find(|s| s.id == "filter.audio.tremolo").map(|s| s.params).unwrap_or_default();
        assert!(tremolo.contains("\"tremoloWaveform\":\"sine|triangle|sawtooth|inversesawtooth|square\"=sine"), "{tremolo}");
    }

    /// A pixel layer gets the effect in its pixels; a smart object a smart filter under the
    /// effect's own command, which re-renders to the same pixels.
    #[test]
    fn commands_filter_pixels_and_record_smart_filters() {
        let (w, h) = (64, 32);
        let canvas = Rect::new(0, 0, w, h);
        let start = || {
            let mut s = Session::new();
            s.execute("file.new", json!({"width": w, "height": h})).unwrap();
            s.execute("layer.new.layer", json!({})).unwrap();
            s.edit("ramp", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                *surf = rgba_gradient(w, h);
                Ok(())
            })
            .unwrap();
            s
        };
        let active = |s: &Session| {
            let d = s.active().unwrap();
            d.doc.layer(d.active_layer.unwrap()).unwrap().clone()
        };
        let mut s = start();
        let before = active(&s).surface().unwrap().read_region(canvas);
        assert_eq!(s.execute("filter.audio.reverb", json!({"reverbRoomSize": 0.9})).unwrap()["effect"], "reverb");
        let filtered = active(&s).surface().unwrap().read_region(canvas);
        assert_ne!(filtered, before);

        let mut t = start();
        t.execute("layer.smartObjects.convertToSmartObject", json!({})).unwrap();
        t.execute("filter.audio.reverb", json!({"reverbRoomSize": 0.9})).unwrap();
        let LayerContent::Smart(sm) = &active(&t).content else { panic!("not a smart object") };
        assert_eq!(sm.smart_filters.len(), 1);
        assert_eq!(sm.smart_filters[0].command, "filter.audio.reverb");
        assert_eq!(sm.smart_filters[0].params["reverbRoomSize"], json!(0.9f32));
        assert_eq!(sm.cache.as_ref().unwrap().read_region(canvas), filtered);
    }

    /// The bytes PhotoCraft hands the DSP are the app's: RGB in row order, without alpha.
    #[test]
    fn renders_like_the_app_and_keeps_alpha() {
        let (w, h) = (300, 40);
        let surf = rgba_gradient(w, h);
        let canvas = Rect::new(0, 0, w, h);
        let effect = EffectInstance::new(EffectKind::WahWah);
        let out = render(&surf, canvas, None, &effect).unwrap();
        let before = surf.read_region(canvas);
        let rgb: Vec<u8> = before.as_chunks::<4>().0.iter().flat_map(|p| p[..3].iter().map(|v| (v * 255.0).round() as u8)).collect();
        let expected = cobalt_core::dsp::apply_effect(&rgb, &effect);
        let after = out.read_region(canvas);
        let got: Vec<u8> = after.as_chunks::<4>().0.iter().flat_map(|p| p[..3].iter().map(|v| (v * 255.0).round() as u8)).collect();
        assert_eq!(got, expected);
        assert_ne!(got, rgb);
        assert!(after.as_chunks::<4>().0.iter().zip(before.as_chunks::<4>().0.iter()).all(|(a, b)| a[3] == b[3]));
    }

    #[test]
    fn empty_selection_changes_nothing() {
        let surf = rgba_gradient(64, 64);
        let canvas = Rect::new(0, 0, 64, 64);
        let sel = Surface::new(PixelFormat::GRAY8);
        let out = render(&surf, canvas, Some(&sel), &EffectInstance::new(EffectKind::Reverb)).unwrap();
        assert_eq!(out.read_region(canvas), surf.read_region(canvas));
    }
}
