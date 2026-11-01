---
name: cobalt-fork
description: The `cobalt` fork of PhotoCraft, which adds the Image as Audio ("cobalt") app's audio effects as Filter › Audio Effects › <effect>… and as smart filters from the Adjustments panel. Use it for any work on this fork, and always when merging upstream PhotoCraft into it (`scripts/cobalt-update.sh` runs this skill headless), resolving merge conflicts, fixing the fork after an upstream or Image as Audio change, or answering how the audio effects are hooked into PhotoCraft.
---

# The cobalt fork

## What this is and why

- **This repo:** `Tomiscout/photocraft` (remote `origin`). The work lives on branch `cobalt`.
  `main` mirrors upstream PhotoCraft (`storytold/photocraft`, remote `upstream`).
- **Purpose:** PhotoCraft with the audio effects of the owner's own app, **Image as Audio**
  (codename *cobalt*; GTK app at `../image-as-audio`, `Tomiscout/image-as-audio`), built in.
  Those effects read an image's colour bytes as a mono audio signal, run audio DSP over them
  (wah-wah, reverb…) and write the result back as pixels.
- **This fork will never be merged upstream.** It is a personal build that gets upstream
  merged into it from time to time. The effects are GPL-3.0-or-later (ported from Audacity),
  while PhotoCraft is MIT/Apache, so the fork's binaries are GPL. Never open a PR from it
  to storytold/photocraft.
- **Design goal: as few changed lines in upstream files as possible,** so merging upstream
  stays cheap. Everything the fork adds lives in fork-only files. Upstream files carry only
  one-line hooks, each marked `// cobalt` (`# cobalt` in TOML).
- **One copy of the effects.** The DSP is not copied into this repo. The engine depends on the
  app's `cobalt-core` crate **by path** (`../image-as-audio/crates/cobalt-core`), so the fork always
  builds the app's current effects, bit for bit. That needs the app checked out next to this
  repo: `~/code/image-as-audio` beside `~/code/photocraft`.
- The WebAssembly plug-in route (Filter › Plug-ins) was tried and dropped: sandboxed bands
  broke effects with memory and the generated dialog couldn't be laid out. Ignore wasm.

## The effects

`EffectKind::ALL` in `cobalt-core/src/effects.rs` (the audio ones, not `is_image()`) is the
source of truth. Each effect is its own command, `filter.audio.<slug>`, in
Filter › Audio Effects. Every effect also takes the shared keys `encoding`, `sampleRate` and
`opacity`.

| Effect | Command `filter.audio.…` | What it does to the byte stream | DSP kind |
|---|---|---|---|
| Wah Wah | `wahwah` | LFO-swept resonant band-pass | stream (`Effect`) |
| Reverb | `reverb` | Freeverb (mono), room size, damping, wet/dry | stream |
| Compressor | `compressor` | Audacity's compressor, with lookahead | whole (`WholeEffect`) |
| Limiter | `limiter` | Compressor with infinite ratio and zero attack | whole |
| Amplify | `amplify` | Gain in dB, optionally without clipping | whole |
| Paulstretch | `paulstretch` | FFT time-stretch, cut back to the input length | whole |
| Parametric EQ | `parametriceq` | Low shelf, 3 peaks, high shelf (RBJ biquads) | whole |
| Delay | `delay` | Audacity's Delay: regular or bouncing-ball echoes, pitch change | whole |
| Tremolo | `tremolo` | Audacity's Tremolo: LFO gain, 5 wave shapes | whole |
| Normalize | `normalize` | DC offset removal, then peak normalisation | whole |

- **Encodings** (`encoding`): `pcm16` (16-bit little-endian, the default), `pcm8`, `mulaw`.
- **Sample rate** (`sampleRate`): 8000–192000, default 44100. It sets how many bytes an LFO
  cycle or a reverb tail spans; it works like a seed for the pattern.
- **Opacity** (`opacity`): 0–1. Mixed in the signal, `dry + (wet − dry) × opacity`, not in
  pixels.
- **Effect parameter keys** are `ParamSpec::ident(kind)`, the effect name plus the group and
  label in camelCase: `reverbRoomSize`, `wahWahLfoFreq`, `parametricEqMid1Gain`.
- **Options** are slugs: `delayDelayType: "bouncingball"`, `tremoloWaveform: "square"`.
- **Toggles** are bools: `normalizeRemoveDcOffset: false`.
- Keys that are missing take the app's defaults. Numbers are clamped to the app's ranges. A
  wrong type, an unknown encoding or option is a `BadParams` error. Other effects' keys are ignored.

```sh
photocraft-cli run in.png --cmd filter.audio.reverb \
  --params '{"reverbRoomSize":0.9,"encoding":"mulaw"}' --out out.png
```

Each command's key list is its params notation, generated from the effect tables (see
`photocraft-cli commands`, or `params_notation` in `cobalt_cmds.rs`).

**How a layer becomes a signal** (`cobalt_cmds::render`):
1. The layer's colour channels over the canvas are read as 8-bit bytes in row order, leaving
   alpha out. For RGB that's `R,G,B,R,G,B…`, exactly the stream the app reads from an image file.
2. `cobalt_core::dsp::apply_effect` runs the effect on all cores, with the app's own threading and
   warm-up logic, so the results are the app's to the byte.
3. Changed bytes are written back as `byte / 255`. Unchanged ones keep their original value, so
   16- and 32-bit documents lose no precision where nothing changed. Alpha is never written.
4. A selection mixes the result in, as built-in filters do. The effect still runs over the
   whole canvas, so the signal doesn't depend on the selection.

**What it works on:**
- A targeted channel or mask (`channel_cmds::channel_surface_for_filter`).
- Raster layers.
- Smart objects: the run is recorded as a smart filter under the effect's command, with every
  value resolved, and re-rendered through the `filters.rs` hook. The Layers panel names it
  after the command label ("Reverb"); double-clicking reopens the effect's dialog.

**Audio effects as layers.** PhotoCraft's adjustment layers are evaluated per tile, at reduced
size when zoomed out, and on the GPU, while an audio effect needs the whole full-resolution
image as one signal. So they are not adjustment layers. They are smart filters, the owner's
choice: the Adjustments panel gets a second row, "Add an audio effect (smart filter)", with one
colour icon per effect (`cobalt_ui::adjustments_row`). A click converts the active layer with
Filter › Convert for Smart Filters (its own undo step) unless it is already a smart object, then
opens the effect's dialog; OK adds the smart filter, Cancel leaves the converted layer.

**Icons** are full-colour 24×24 SVGs in the app repo, `cobalt-core/icons/<slug>.svg`, returned by
`cobalt_core::icons::svg(kind)`. `cobalt_ui::icon` loads them through egui's SVG loader
(`bytes://cobalt/icons/<slug>.svg`), untinted. PhotoCraft's own icons are white, tinted
per use (`icons.rs`); ours don't go through that set.
- It is one undo step, labelled with the effect's name.

## Where the fork's code is

**Fork-only files** (upstream never touches them, so they never conflict):

| File | Role |
|---|---|
| `crates/engine/src/cmd/cobalt_cmds.rs` | One `filter.audio.<slug>` command per effect, generated from `EffectKind::ALL` (`ENTRIES`; `RUNS` gives each its plain `fn` through a const generic, 16 slots), `kind_of`/`command_id`, params ↔ `EffectInstance`, the params notation, `render`, the smart-filter re-render, `pub use cobalt_core` for the UI, tests |
| `crates/ui-egui/src/dialogs/cobalt_ui.rs` | The dialog body: Encoding + Sample Rate on one row, Opacity, hairline, then the effect's settings with group headings, in PhotoCraft's widgets (`widgets::dropdown`, `slider_row_prec`, `checkbox`, `section_label`). Also the Adjustments panel's audio row and the colour icon loader |
| `crates/ui-egui/src/dialogs/cobalt_ui/dynamics.rs` | Compressor and Limiter editor (submodule of `cobalt_ui`, no hook): Audacity's knobs (`KnobControl.qml`, `WarpingTransformer`: default at twelve o'clock, drag any direction, Shift ÷3, wheel, arrows, double-click default) with a value field each, time and level groups, and the -36..0 dB transfer curve (`CompressionCurve.qml`) drawn from `Compressor::output_db`. Audacity's source for comparison: `~/code/random/audacity/src/effects/builtin_collection/dynamics/` |
| `.claude/skills/cobalt-fork/` | This skill and `compare.py` (app vs PhotoCraft, byte for byte) |
| `scripts/cobalt-update.sh` | Merges upstream through a headless Claude run of this skill |
| `scripts/cobalt-install.sh` | Release build and per-user install as "PhotoCraft - Cobalt" (see Installing) |

**Hooks in upstream files.** `git grep -n cobalt -- crates` lists them. There should be
exactly these:

| File | Hook | Why |
|---|---|---|
| `crates/engine/Cargo.toml` | `cobalt-core = { path = "../../../image-as-audio/crates/cobalt-core" } # cobalt` after `photocraft-plugins` | The effects |
| `crates/engine/src/cmd.rs` | `pub mod cobalt_cmds; // cobalt` after `pub mod plugin_cmds;` | Declares the module |
| `crates/engine/src/cmd.rs` | `v.extend(crate::cobalt_cmds::specs()); // cobalt` after the `plugin_cmds::specs()` line in the command list | Registers the command. Menu, CLI, MCP and the control channel come with it |
| `crates/engine/src/lib.rs` | `pub use cmd::cobalt_cmds; // cobalt` after `pub use cmd::plugin_cmds;` | `crate::cobalt_cmds` paths, and the UI's access |
| `crates/engine/src/filters.rs` | In `apply_filter_to_surface`, before the plug-in smart filters: `if let Some(out) = crate::cobalt_cmds::apply_to_surface(...) { return Ok(Some(out)); } // cobalt` | Smart-filter re-render |
| `crates/ui-egui/src/dialogs.rs` | `pub mod cobalt_ui; // cobalt` after `pub mod guide_layout_ui;` | Declares the dialog module |
| `crates/ui-egui/src/dialogs.rs` | In `filter_body`, before the `bitmap_dialog::owns` branch: `if let Some(kind) = cobalt_ui::owner(fields) { cobalt_ui::body(ui, kind, fields) } else // cobalt` | Draws our body inside PhotoCraft's filter dialog |
| `crates/ui-egui/src/dialogs/filter_dialog.rs` | In `full_resolution_preview`, after `FULL_RESOLUTION_PREVIEW.contains(&command)`: `\|\| photocraft_engine::cobalt_cmds::kind_of(command).is_some() // cobalt` | The preview must not run on a reduced proxy: audio effects on downscaled bytes look nothing like the result |
| `crates/ui-egui/src/panels/properties.rs` | Last line of `adjustments_grid`: `crate::dialogs::cobalt_ui::adjustments_row(app, ui); // cobalt` | The audio row under Photoshop's adjustment buttons, in the docked and the floating Adjustments tab |
| `crates/ui-egui/src/os_window.rs` | `APP_ID` reads `option_env!("PHOTOCRAFT_APP_ID")`, defaulting to upstream's id `// cobalt` | Lets the installed build carry its own window id. Without the variable nothing changes, and upstream's test that the `.desktop` file's `StartupWMClass` equals `APP_ID` still passes |
| `Cargo.lock` | The `cobalt-core` package entry | Generated; never resolve it by hand |

**What comes for free from PhotoCraft:**
- The command id starts with `filter.`, so `filter_dialog::has_dialog` gives it a filter dialog.
  That brings live canvas preview, the in-dialog preview window, Alt+Reset to defaults,
  last-used values and OK / Cancel.
- `menu: &["Filter", "Audio Effects"]` makes an Audio Effects submenu at the end of the Filter menu (`menus/items.rs`), one item per effect.
- Smart-filter rows, their dialogs and Filter › Last Filter work per effect, since each is its own command.
- `enabled` reuses `filters::has_filterable_layer`.

No hook is needed in `xtask/src/layers.rs`. `cobalt-core` is not a workspace member, and the layer
check only classifies members. Nothing else in upstream should change. If you find another
upstream file changed on `cobalt`, either it's a mistake or this table is out of date: fix one
or the other.

## Installing (PhotoCraft - Cobalt)

`scripts/cobalt-install.sh` installs it for the current user, next to an official PhotoCraft (the
user also has the official AppImage), and `--uninstall` removes it. Run it again after every
update. It does the following:
- **Build:** a release build (`-p photocraft -p photocraft-cli --features heif`, as upstream's
  `packaging/linux/package.sh` does) in `target/cobalt`, with `PHOTOCRAFT_APP_ID=ai.storyteller.photocraft.cobalt`.
- **Binaries:** go in `~/.local/lib/photocraft-cobalt/`.
- **Wrappers:** `~/.local/bin/photocraft-cobalt` and `photocraft-cobalt-cli` set
  `PHOTOCRAFT_CONFIG_DIR=~/.config/photocraft-cobalt`. That folder holds Cobalt's own settings
  and its own single-instance lock. With a shared lock, opening Cobalt while the official app
  runs would hand the files to the official window.
- **Icons:** PhotoCraft's icon with the background recoloured violet (ImageMagick), in
  `~/.local/share/icons/hicolor/*/apps/ai.storyteller.photocraft.cobalt.png`.
- **Launcher:** `~/.local/share/applications/ai.storyteller.photocraft.cobalt.desktop`, made
  from upstream's `packaging/linux/ai.storyteller.photocraft.desktop` with `sed`. The script
  stops if upstream's entry changed so that a substitution no longer applies; update its `sed`
  lines then.

## Updating from upstream (the main job)

`scripts/cobalt-update.sh` checks the preconditions, runs `git fetch upstream`, turns on
`rerere`, then runs this skill headless. When asked to update:

1. **Check the start state.** You're on `cobalt` with a clean tree, `../image-as-audio/crates/cobalt-core`
   exists, and `git rev-list --count HEAD..upstream/main` is above 0 (if it's 0, say so and stop).
2. **Merge, don't rebase:** `git merge --no-edit upstream/main`. Merging keeps the `cobalt`
   history and makes the next merge cheap.
3. **Resolve conflicts** (`git diff --name-only --diff-filter=U`):
   - `Cargo.lock`: `git checkout --theirs Cargo.lock`, then let cargo add `cobalt-core` back
     (the next `cargo build` or `cargo update -p cobalt-core` does it).
   - A hook line: take upstream's side of the hunk completely, then put the one hook line back
     at the equivalent place, with its `// cobalt` marker. Never keep an old version of
     upstream code to make a hook fit.
   - A fork-only file can't conflict. If it does, upstream added a file with the same name:
     rename ours, keeping the `cobalt_` prefix, and update its hooks.
4. **Fix silent breakage.** Most upstream changes break the fork without conflicting: a
   renamed function or type, a moved module, a changed signature. `cargo build -p photocraft-engine -p photocraft-ui-egui`
   and fix only the **fork-only files** to follow the new API. If a hook's anchor moved or
   disappeared, find upstream's new equivalent. Search for how `plugin_cmds` / `plugin.run`
   hooks in now, since our hooks mirror it: filter dialog routing, the full-resolution preview
   list, smart-filter re-render, command registration. Move the hook there.
5. **Verify** (all must pass; fix and repeat):
   ```sh
   cargo test -p photocraft-engine --lib cobalt_cmds
   cargo test -p photocraft-ui-egui --lib cobalt_ui
   cargo clippy -p photocraft-engine -p photocraft-ui-egui --lib -- -D warnings
   cargo xtask layers
   cargo build -p photocraft-cli && (cd ../image-as-audio && cargo build --release) \
     && .claude/skills/cobalt-fork/compare.py        # needs Pillow; every effect 0 bytes differ
   git grep -n cobalt -- crates                      # exactly the hooks in the table above
   git diff upstream/main --stat -- crates Cargo.toml # only the hook files and the fork-only files
   ```
   If the dialog's code changed, look at it:
   ```sh
   cargo run -p photocraft-ui-egui --example snapshot -- --out /tmp/audio.png --size 1440x900 \
     --scale 1 --open <image> --script '[["ui.menu.invoke",{"id":"filter.audio.reverb"}]]'
   ```
   Then read the PNG.
6. **Commit.** If the merge had conflicts or fixes, finish it with `git commit --no-edit` after
   staging. Commit follow-up fixes to fork-only files separately ("cobalt: follow upstream
   <what changed>"). Never push: the script pushes when asked (`--push`).
7. **Update this skill** if a hook moved or a step was wrong, in the same commit. This
   file is the map for the next update, so keep it true.

If something can't be resolved without changing upstream behaviour (for example, upstream
removed filter dialogs as they work now), stop. Leave the merge in progress (or abort it with
`git merge --abort` if nothing is salvageable) and explain what changed and what you suggest.

## When the Image as Audio app changes

The fork builds `../image-as-audio/crates/cobalt-core` as it is on disk, so app changes arrive
with the next build. These are the parts of `cobalt-core` the fork uses:
- `effects::{EffectKind, EffectInstance, ParamSpec, OPACITY, SAMPLE_RATE, slug}`
- `ParamSpec::ident`
- `EffectKind::{ALL, is_image, name, slug, params}`
- `dsp::{SampleEncoding (ALL, key, short_name), apply_effect}`
- `dsp::Compressor::{compressor, limiter, output_db}` (the dynamics editor's curve; `output_db` was added for it)
- `icons::svg`

- **A new audio effect:** gets its command, menu item, dialog and Adjustments button without
  any change here (up to 16 effects; past that, add entries to `RUNS`). Give it an icon in the
  app (`cobalt-core/icons/`, `icons::svg`; the icon test fails until it has one), and add a row to
  `compare.py` and to the table above.
- **A renamed or removed API:** fix `cobalt_cmds.rs` / `cobalt_ui.rs`, or change the app so it keeps
  that API.
- **Effect-specific editors:** the app's Parametric EQ has a draggable response graph
  (`eq_graph.rs`, GTK) that isn't ported. `cobalt_core::dsp::equalizer::{coefficients, response_db}`
  is what an egui version would draw from. It would go in a new `cobalt_ui/` submodule, as
  `dynamics.rs` (Compressor, Limiter) does.

## Rules for any work on this fork

- New behaviour goes in fork-only files. Upstream files get one-line, `// cobalt`-marked hooks
  next to an equivalent upstream hook (usually the `plugin_cmds` one). Never reformat,
  reorder or refactor upstream code. Don't run `cargo fmt` over upstream files: it would
  rewrite the hook lines and other code and cause conflicts. Format only fork-only files
  (`rustfmt --edition 2024 <file>`).
- Follow PhotoCraft's own rules (`AGENTS.md`) inside fork-only files: no panics on any input
  (no `unwrap`/`expect` outside tests; `panic_hunt` fuzzes every command), errors as
  `EngineError`, widgets from `crate::widgets`, colours from `theme::Tokens`.
- The effects themselves (DSP, parameter ranges, defaults) belong in the app repo's `cobalt-core`,
  never in this repo.
- Don't push, open PRs, or touch `main`.
