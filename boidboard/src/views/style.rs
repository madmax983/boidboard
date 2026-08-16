//! The embedded stylesheet.
//!
//! Its own module for one reason: it is seventy lines of CSS in a raw string,
//! and interleaving that with rendering functions made both harder to read.
//! Nothing here is Rust logic, so nothing here needs to sit near any.
//!
//! Embedded rather than served as a file, and deliberately: there is no external
//! host anywhere in the document, which is how AC-43's "no SPA framework, no
//! WASM" is enforced — by there being nothing to load.

/// Deliberately small and dependency-free.
pub(super) const STYLESHEET: &str = r"
:root { color-scheme: light dark; --ink:#16181d; --dim:#666e7a; --line:#d8dce3;
        --bg:#fbfbfd; --panel:#fff; --accent:#2f6fed; }
@media (prefers-color-scheme: dark) {
  :root { --ink:#e8eaf0; --dim:#9aa3b2; --line:#2b3140; --bg:#101319; --panel:#171b23; }
}
* { box-sizing: border-box; }
body { margin:0; background:var(--bg); color:var(--ink);
       font:15px/1.5 ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif; }
.masthead { display:flex; gap:1.5rem; align-items:baseline;
            padding:.9rem 1.4rem; border-bottom:1px solid var(--line); }
.brand { font-weight:700; letter-spacing:-.02em; text-decoration:none; color:var(--ink); }
.masthead-nav a { margin-right:1rem; color:var(--dim); text-decoration:none; }
.page { max-width:76rem; margin:0 auto; padding:1.4rem; }
h1 { font-size:1.4rem; margin:0 0 1rem; letter-spacing:-.02em; }
h2 { font-size:1rem; margin:0 0 .6rem; letter-spacing:-.01em; }
table.runs { width:100%; border-collapse:collapse; font-variant-numeric:tabular-nums; }
table.runs th, table.runs td { text-align:left; padding:.5rem .6rem;
                               border-bottom:1px solid var(--line); }
table.runs th { font-size:.78rem; text-transform:uppercase; letter-spacing:.06em;
                color:var(--dim); }
.status-badge { display:inline-block; padding:.1rem .5rem; border-radius:99px;
                font-size:.78rem; border:1px solid var(--line); }
.status-badge[data-status='running'] { border-color:var(--accent); color:var(--accent); }
.status-badge[data-status='failed'], .status-badge[data-status='budget_exceeded']
                { border-color:#c0392b; color:#c0392b; }
.status-badge[data-status='completed'] { border-color:#2e7d4f; color:#2e7d4f; }
.panels { display:grid; gap:1.2rem; grid-template-columns:repeat(auto-fit,minmax(20rem,1fr)); }
.panel { background:var(--panel); border:1px solid var(--line); border-radius:.6rem;
         padding:1rem; }
svg.flock, svg.trajectories { width:100%; height:auto; display:block;
                              background:var(--panel); border-radius:.4rem; }
.world-bounds { fill:none; stroke:var(--line); }
.agent { fill:var(--accent); stroke:none; }
.obstacle { fill:rgba(192,57,43,.15); stroke:#c0392b; }
.trail { fill:none; stroke:var(--accent); stroke-width:.6; opacity:.65; }
.trail-dot { fill:var(--accent); opacity:.65; }
.goal { fill:none; stroke:#2e7d4f; stroke-dasharray:3 3; }
.metrics { display:grid; gap:.9rem; grid-template-columns:repeat(auto-fit,minmax(12rem,1fr)); }
.metric-card { margin:0; }
.metric-card figcaption { font-size:.78rem; color:var(--dim); text-transform:uppercase;
                          letter-spacing:.06em; }
svg.sparkline { width:100%; height:2.4rem; display:block; }
.spark-line { fill:none; stroke:var(--accent); stroke-width:1.2; }
.spark-baseline { stroke:var(--line); stroke-width:.5; }
.metric-latest { font-variant-numeric:tabular-nums; font-size:1.1rem; }
.provenance dl { display:grid; grid-template-columns:auto 1fr; gap:.3rem .9rem; margin:0; }
.provenance dt { color:var(--dim); font-size:.82rem; }
.provenance dd { margin:0; font-family:ui-monospace, SFMono-Regular, Menlo, monospace;
                 font-size:.82rem; word-break:break-all; }
.prov-pending { color:var(--dim); font-style:italic; }
.presets { display:grid; gap:.9rem; list-style:none; margin:0 0 1.2rem; padding:0;
           grid-template-columns:repeat(auto-fit,minmax(16rem,1fr)); }
.preset-card { background:var(--panel); border:1px solid var(--line); border-radius:.6rem;
               padding:.9rem; }
.preset-name { font-weight:600; }
.preset-description { color:var(--dim); font-size:.86rem; }
.preset-params { display:grid; grid-template-columns:auto 1fr; gap:.1rem .6rem;
                 font-size:.78rem; color:var(--dim); margin:.6rem 0 0; }
.preset-params dd { margin:0; font-variant-numeric:tabular-nums; }
.compare { display:grid; gap:1.2rem; }
.compare-sides { display:grid; gap:1.2rem; grid-template-columns:repeat(auto-fit,minmax(20rem,1fr)); }
table.config-diff { border-collapse:collapse; width:100%; font-variant-numeric:tabular-nums; }
table.config-diff th, table.config-diff td { text-align:left; padding:.35rem .6rem;
                                             border-bottom:1px solid var(--line); }
.run-progress { display:flex; gap:.8rem; align-items:center; }
progress.progress-bar { width:14rem; }
.cancel-run { display:flex; gap:.8rem; align-items:baseline; margin:.8rem 0 1.2rem; }
.cancel-run-button { border:1px solid #c0392b; color:#c0392b; background:none;
                     border-radius:.4rem; padding:.3rem .8rem; font:inherit; cursor:pointer; }
.cancel-run-note { margin:0; color:var(--dim); font-size:.82rem; }
.stuck-badge { margin:.8rem 0; padding:.5rem .8rem; border-radius:.4rem;
               border:1px solid #c0392b; color:#c0392b; font-size:.86rem; }
.empty { color:var(--dim); font-style:italic; }
";
