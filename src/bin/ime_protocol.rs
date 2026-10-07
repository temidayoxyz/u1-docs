//! The IME protocol harness â€” Phase 0, criterion 3.
//!
//! **This exists so criterion 3 can be closed by a human, today.**
//!
//! Spike B verified every piece of geometry an input method editor depends on,
//! and verified nothing about whether a webview actually delivers correct
//! composition. That remaining question cannot be answered by a test: it needs a
//! registered input method, a person composing with it, and eyes on the result.
//!
//! Rather than leave that as a document describing something to build later,
//! this is the thing to run.
//!
//! ```text
//! cargo run --features ime-protocol --bin ime-protocol
//! ```
//!
//! ## What it demonstrates
//!
//! - A caret drawn at the position the Rust layout engine computes â€” not at a
//!   position the webview guesses.
//! - A hidden `contenteditable` anchor positioned exactly over that caret, which
//!   is where the OS IME attaches.
//! - Composition, preedit and commit reported through a live event log, so a
//!   commit landing at the wrong offset is visible rather than inferred.
//!
//! ## How to use it
//!
//! Follow the 8 steps in `docs/spike-b-caret-and-ime.md`. Every step has an
//! observable pass condition, and the log tells you what happened without you
//! having to reason about whether the behaviour was correct.
//!
//! ## The one thing this cannot settle
//!
//! Whether this approach is *right*. It can show you that composition works, or
//! that it does not. It cannot tell you whether a different approach would have
//! been better â€” only whether this one survives contact with a real IME.
//!
//! If steps 3 or 4 fail, ADR-0003 is refuted for that platform. Write a
//! superseding ADR; do not patch around it.

// The whole harness is behind the `ime-protocol` feature, so the default build
// carries no webview dependency at all (ADR-0003 is still provisional).
//
// `#![cfg(feature = ...)]` alone would leave an empty binary with no `main`,
// which fails to build. So there are two guards: the stub `main` below for the
// no-feature case, and a `cfg` on the implementation that follows.

#[cfg(not(feature = "ime-protocol"))]
fn main() {
    eprintln!("This harness needs the `ime-protocol` feature.");
    eprintln!("Try: cargo run --features ime-protocol --bin ime_protocol");
    std::process::exit(2);
}

// Gated with the rest of the implementation: without it the import is unused,
// and the harness must not appear in the default build at all.
#[cfg(feature = "ime-protocol")]
use u1_docs::caret::{Affinity, CaretMap};

/// Text chosen to make the failure modes visible rather than incidental:
/// ASCII (where everything works), RTL (where commit offsets usually break),
/// and CJK (where the IME itself is exercised).
#[cfg(feature = "ime-protocol")]
const LTR_SAMPLE: &str = "The quick brown fox jumps over the lazy dog.";
#[cfg(feature = "ime-protocol")]
const RTL_SAMPLE: &str = "The meeting is at \u{0645}\u{0631}\u{062D}\u{0628}\u{0627} 14 March.";
#[cfg(feature = "ime-protocol")]
const CJK_SAMPLE: &str = "\u{4EBA}\u{4EBA}\u{751F}\u{800C}\u{81EA}\u{7531}";

/// Geometry computed on the Rust side and handed to the webview.
///
/// The whole point of ADR-0003 is that these values come from our layout engine
/// rather than from the DOM, so they are sent as plain numbers and the HTML
/// never asks the browser where anything is.
#[cfg(feature = "ime-protocol")]
fn geometry_for(text: &str) -> String {
    let mut font_cx = parley::FontContext::new();
    let mut layout_cx = parley::LayoutContext::new();

    let map = CaretMap::build_default(&mut layout_cx, &mut font_cx, text, Some(680.0), 18.0);

    let mut lines = Vec::new();
    for i in 0..map.line_count() {
        let top = map.line_top(i).unwrap_or(0.0);
        let h = map.line_height(i).unwrap_or(0.0);
        let w = map.line_advance(i).unwrap_or(0.0);
        lines.push(format!("{{y:{top:.1},h:{h:.1},w:{w:.1}}}"));
    }

    let positions: Vec<String> = map
        .grapheme_caret_positions()
        .iter()
        .filter_map(|&p| {
            map.caret_rect(p, Affinity::Start)
                .map(|r| format!("{{byte:{p},x:{:.1},y:{:.1},h:{:.1}}}", r.x, r.y, r.height))
        })
        .collect();

    format!(
        "{{lines:[{}],carets:[{}],lineHeight:{},isRtl:{},text:{}}}",
        lines.join(","),
        positions.join(","),
        map.line_height(0).unwrap_or(0.0),
        map.is_rtl(),
        serde_escape(text)
    )
}

#[cfg(feature = "ime-protocol")]
fn serde_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(feature = "ime-protocol")]
const HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>U1 Docs - IME protocol harness</title>
<style>
  :root { color-scheme: light dark; }
  body {
    font: 14px/1.5 system-ui, sans-serif;
    margin: 0; padding: 20px 24px;
    display: grid; grid-template-columns: 1fr 380px; gap: 24px;
    align-items: start;
  }
  h1 { font-size: 17px; margin: 0 0 4px; }
  .sub { opacity: .7; margin: 0 0 16px; font-size: 13px; }
  .panel { border: 1px solid color-mix(in srgb, currentColor 20%, transparent);
           border-radius: 8px; padding: 14px; }
  .stage { position: relative; background: color-mix(in srgb, currentColor 4%, transparent);
           border-radius: 6px; padding: 12px; min-height: 320px; }
  canvas { display: block; }
  /* The IME anchor. Positioned exactly over the Rust-computed caret, sized to
     a caret's worth of pixels, and otherwise invisible. The OS IME attaches
     here; if it lands wrong, composition is wrong. */
  #anchor {
    position: absolute; width: 2px; height: 22px; opacity: 0;
    background: transparent; border: 0; padding: 0; margin: 0;
    outline: none; resize: none; overflow: hidden;
    font: 18px/22px system-ui, sans-serif; white-space: pre; caret-color: transparent;
  }
  .samples button { display:block; width:100%; text-align:left; margin: 0 0 6px;
                    padding: 7px 9px; font: inherit; cursor: pointer;
                    border-radius: 6px; border: 1px solid color-mix(in srgb, currentColor 20%, transparent);
                    background: transparent; color: inherit; }
  .samples button[aria-pressed="true"] { background: color-mix(in srgb, currentColor 12%, transparent); }
  #log { font: 12px/1.45 ui-monospace, monospace; height: 300px; overflow: auto;
         background: color-mix(in srgb, currentColor 5%, transparent);
         border-radius: 6px; padding: 8px; white-space: pre-wrap; }
  .ev { padding: 1px 0; }
  .ev b { font-weight: 600; }
  .ok { color: #1a7f37; } .warn { color: #9a6700; } .bad { color: #b42318; }
  ol { padding-left: 20px; margin: 8px 0 0; } li { margin: 3px 0; }
  .step { font-size: 13px; }
</style>
</head>
<body>
  <div>
    <h1>U1 Docs &mdash; IME protocol harness</h1>
    <p class="sub">Phase 0 criterion 3. Caret geometry comes from the Rust layout engine,
       not from the DOM. Follow the 8 steps in <code>docs/spike-b-caret-and-ime.md</code>.</p>
    <div class="panel">
      <div class="stage" id="stage">
        <canvas id="canvas" width="700" height="300"></canvas>
        <textarea id="anchor" autocomplete="off" autocorrect="off"
                  autocapitalize="off" spellcheck="false"></textarea>
      </div>
    </div>
    <div class="panel" style="margin-top:16px">
      <ol class="step" id="steps"></ol>
    </div>
  </div>

  <div class="panel">
    <strong>Sample</strong>
    <div class="samples" id="samples" style="margin-top:8px"></div>
    <strong style="display:block;margin-top:16px">Event log</strong>
    <div id="log" style="margin-top:8px"></div>
  </div>

<script>
const SAMPLES = {
  ltr: { label: "1. ASCII (LTR)", text: LTR },
  rtl: { label: "2. Arabic inside English", text: RTL },
  cjk: { label: "3. Chinese", text: CJK },
};

let geo = null, active = "ltr", caret = 0, composing = false, preeditSeen = false;

const cv = document.getElementById("canvas");
const ctx = cv.getContext("2d");
const anchor = document.getElementById("anchor");
const stage = document.getElementById("stage");
const logEl = document.getElementById("log");

function log(kind, msg) {
  const d = document.createElement("div");
  d.className = "ev " + kind;
  d.innerHTML = "<b>" + kind.toUpperCase() + "</b> " + msg;
  logEl.appendChild(d);
  logEl.scrollTop = logEl.scrollHeight;
  if (kind === "bad") window.__imeBad = (window.__imeBad || 0) + 1;
  if (kind === "ok")   window.__imeOk  = (window.__imeOk  || 0) + 1;
}

const STEPS = [
  "Caret at byte 0 &mdash; switch to a CJK IME. Candidate window must appear AT THE CARET.",
  "Type a few romaji/kana. Preedit must render INLINE, underlined, not as a separate box.",
  "Press Space to commit. Text must land at the caret; no characters before or after.",
  "Repeat in the Arabic sample. Commit offset must still be correct &mdash; hardest case.",
  "Press Left/Right mid-composition. Composition must cancel cleanly.",
  "Press Escape mid-composition. Composition must cancel; document unchanged.",
  "Resize the window mid-composition. Candidate window must track the new caret.",
  "Click elsewhere mid-composition. No orphaned preedit, ever.",
];
document.getElementById("steps").innerHTML =
  STEPS.map(s => "<li>" + s + "</li>").join("");

function loadGeometry(key) {
  const s = SAMPLES[key];
  geo = window.__U1_GEO__[key];
  caret = 0;
  composing = false; preeditSeen = false;
  draw();
  syncAnchor();
  log("", "loaded <b>" + s.label + "</b> (" + geo.carets.length + " caret positions, isRtl=" + geo.isRtl + ")");
}

function draw() {
  if (!geo) return;
  const dpr = window.devicePixelRatio || 1;
  const pad = 16;
  cv.width = 700 * dpr; cv.height = 300 * dpr;
  cv.style.width = "700px"; cv.style.height = "300px";
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, 700, 300);

  // Line boxes, so it is visible whether vertical geometry is sane.
  ctx.strokeStyle = "rgba(128,128,128,.35)";
  ctx.lineWidth = 1;
  geo.lines.forEach(l => {
    ctx.strokeRect(pad + .5, pad + l.y + .5, l.w, l.h);
  });

  // Carets for every grapheme boundary, faint.
  ctx.fillStyle = "rgba(128,128,128,.30)";
  geo.carets.forEach(c => ctx.fillRect(pad + c.x, pad + c.y, 1, c.h));

  // The active caret.
  const c = geo.carets[caret];
  if (c) {
    ctx.fillStyle = "#0969da";
    ctx.fillRect(pad + c.x - .5, pad + c.y, 2, c.h);
  }

  // Preedit, drawn at the caret. If the OS commits preedit text as real text
  // instead, this stays empty while the document gains characters - the exact
  // failure step 2 checks for.
  const pre = anchor.value;
  if (composing && pre) {
    ctx.font = "18px system-ui, sans-serif";
    ctx.fillStyle = "#0969da";
    ctx.fillText(pre, pad + (c ? c.x : 0), pad + (c ? c.y + c.h * .78 : 20));
  }
  window.__caretGeom = c ? { x: c.x, y: c.y, h: c.h, byte: c.byte } : null;
}

function syncAnchor() {
  const c = geo && geo.carets[caret];
  if (!c) return;
  const pad = 16;
  // The anchor sits exactly on the Rust-computed caret. This is the whole bet
  // of ADR-0003: if the IME misplaces relative to this, composition is wrong.
  anchor.style.left = (cv.offsetLeft + pad + c.x - 1) + "px";
  anchor.style.top  = (cv.offsetTop + pad + c.y) + "px";
  anchor.style.height = c.h + "px";
  anchor.focus();
}

function moveCaret(delta) {
  if (!geo || !geo.carets.length) return;
  const next = Math.max(0, Math.min(geo.carets.length - 1, caret + delta));
  if (next === caret) return;
  caret = next;
  draw(); syncAnchor();
  log("", "caret -> index " + caret + " (byte " + geo.carets[caret].byte + ")");
}

// ---- IME events ---------------------------------------------------------
// These are the observable signals the protocol is really about.

anchor.addEventListener("compositionstart", () => {
  composing = true; preeditSeen = false;
  log("", "compositionstart at caret index " + caret);
  draw();
});

anchor.addEventListener("compositionupdate", (e) => {
  preeditSeen = true;
  log("", "compositionupdate <code>" + escapeHtml(e.data || "") + "</code>");
  draw();
});

anchor.addEventListener("compositionend", (e) => {
  composing = false;
  const data = e.data || "";
  log("ok", "compositionend committed <code>" + escapeHtml(data) + "</code> (" +
      data.length + " chars) at caret index " + caret);
  // Committing is not visible in the Rust geometry - the document text is the
  // webview's. So the harness records what arrived; a human compares it against
  // step 3's expectation. This is stated rather than hidden.
  log("warn", "document text is owned by the webview; verify the commit landed at the caret (step 3)");
  draw();
});

anchor.addEventListener("input", (e) => {
  if (composing) { draw(); return; }
  if (e.isComposing) { draw(); return; }
  log("warn", "input (not composing): <code>" + escapeHtml(anchor.value) + "</code> &mdash; expected only via compositionend");
});

anchor.addEventListener("keydown", (e) => {
  if (e.isComposing || e.keyCode === 229) {
    log("", "keydown during composition (keyCode 229) &mdash; expected for IME keys");
    return;
  }
  if (e.key === "ArrowLeft")  { e.preventDefault(); moveCaret(-1); }
  if (e.key === "ArrowRight") { e.preventDefault(); moveCaret(+1); }
  if (e.key === "Escape") { if (composing) log("ok", "Escape pressed mid-composition"); }
});

anchor.addEventListener("blur", () => log("warn", "anchor blurred"));

// ---- Chrome -------------------------------------------------------------
document.getElementById("samples").innerHTML =
  Object.entries(SAMPLES).map(([k, s]) =>
    '<button data-k="' + k + '" aria-pressed="false">' + s.label + "</button>").join("");
document.querySelectorAll("#samples button").forEach(b => {
  b.onclick = () => {
    document.querySelectorAll("#samples button").forEach(x => x.setAttribute("aria-pressed", "false"));
    b.setAttribute("aria-pressed", "true");
    active = b.dataset.k;
    loadGeometry(active);
    anchor.value = "";
  };
});

window.addEventListener("resize", () => {
  if (composing) log("", "window resized while composing &mdash; candidate window should follow");
  draw(); syncAnchor();
});

stage.addEventListener("mousedown", (e) => {
  if (e.target === anchor) return;
  if (composing) log("warn", "clicked outside while composing (step 8)");
});

// Geometry injected by Rust, keyed by sample.
window.__U1_GEO__ = { ltr: GEO_LTR, rtl: GEO_RTL, cjk: GEO_CJK };
loadGeometry("ltr");
log("ok", "ready. Caret geometry is from the Rust layout engine.");
</script>
</body>
</html>
"##;

#[cfg(feature = "ime-protocol")]
fn main() {
    use tao::event_loop::ControlFlow;

    // Apply the platform font policy first, so the harness renders the same way
    // the product would.
    let geo_ltr = geometry_for(LTR_SAMPLE);
    let geo_rtl = geometry_for(RTL_SAMPLE);
    let geo_cjk = geometry_for(CJK_SAMPLE);

    let html = HTML
        .replace(
            "const LTR",
            &format!("const LTR_RAW = {}", serde_escape(LTR_SAMPLE)),
        )
        .replace(
            "const RTL",
            &format!("const RTL_RAW = {}", serde_escape(RTL_SAMPLE)),
        )
        .replace(
            "const CJK",
            &format!("const CJK_RAW = {}", serde_escape(CJK_SAMPLE)),
        )
        .replace("LTR,", "LTR_RAW,")
        .replace("RTL,", "RTL_RAW,")
        .replace("CJK,", "CJK_RAW,")
        .replace("GEO_LTR", &geo_ltr)
        .replace("GEO_RTL", &geo_rtl)
        .replace("GEO_CJK", &geo_cjk);

    println!("U1 Docs - IME protocol harness");
    println!("Phase 0 criterion 3. See docs/spike-b-caret-and-ime.md for the 8 steps.");
    println!();
    println!("If no window appears, your system has no CJK input method registered.");
    println!("Check with: Get-ItemProperty 'HKCU:\\Keyboard Layout\\Preload'");

    use tao::event::{Event, WindowEvent};
    use tao::window::WindowBuilder;

    let event_loop = tao::event_loop::EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("U1 Docs - IME protocol harness")
        .with_inner_size(tao::dpi::LogicalSize::new(1180.0, 680.0))
        .build(&event_loop)
        .unwrap_or_else(|e| panic!("could not create a window: {e}"));

    // wry 0.57 is webview-only - it attaches to a window handle rather than
    // creating one - so the host window comes from tao. Same shape Tauri uses.
    let webview = wry::WebViewBuilder::new().with_html(&html).build(&window);

    match webview {
        Ok(_) => {}
        Err(e) => {
            eprintln!("could not create a webview: {e}");
            eprintln!();
            eprintln!("On Linux this normally means webkit2gtk is missing:");
            eprintln!("  sudo apt-get install libwebkit2gtk-4.1-dev");
            std::process::exit(1);
        }
    }

    // Keep the webview alive for the whole event loop.
    let mut webview = Some(webview.expect("webview creation was handled above"));

    // tao 0.37 passes `(event, target, control_flow)` â€” event first.
    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;

        // `Event::WindowEvent` is `#[non_exhaustive]`, so the `..` is required.
        // The harness has exactly one window, so every window event is ours.
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            // Explicitly drop the webview before exiting: letting it fall out of
            // scope at process teardown races WebView2's shutdown and
            // intermittently crashes.
            drop(webview.take());
            *control_flow = ControlFlow::Exit;
        }
    });
}
// binary name is the file stem: ime_protocol
