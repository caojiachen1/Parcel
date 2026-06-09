//! `parcel preview` — generate a standalone HTML preview and open in browser.

use anyhow::{Context, Result};
use parcel_core::config::ParcelConfig;

pub fn run() -> Result<()> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("parcel.json");

    if !config_path.exists() {
        anyhow::bail!(
            "parcel.json not found. Run `parcel init` first to create a configuration file."
        );
    }

    let config = ParcelConfig::load(&config_path)
        .context("Failed to parse parcel.json")?;

    println!("Generating installer preview for {} v{}…", config.app.name, config.app.version);

    // Read EULA.
    let eula_text = if let Some(ref eula_path) = config.eula.file {
        std::fs::read_to_string(cwd.join(eula_path)).unwrap_or_default()
    } else {
        String::new()
    };

    // Generate a self-contained HTML preview.
    let html = generate_preview_html(&config, &eula_text);

    let preview_path = cwd.join("dist").join("parcel-preview.html");
    std::fs::create_dir_all(cwd.join("dist"))?;
    std::fs::write(&preview_path, &html)?;

    println!("Preview generated: {}", preview_path.display());

    // Open in default browser.
    open_in_browser(&preview_path);

    Ok(())
}

/// Generate a self-contained HTML file with the installer UI and config baked in.
fn generate_preview_html(config: &ParcelConfig, eula_text: &str) -> String {
    let config_json = serde_json::to_string(&serde_json::json!({
        "app_name": config.app.name,
        "app_version": config.app.version,
        "publisher": config.app.publisher,
        "publisher_url": config.app.publisher_url,
        "eula_text": eula_text,
        "appearance": {
            "theme": config.appearance.theme,
            "colors": config.appearance.colors,
            "border_radius": config.appearance.border_radius,
            "page_animation": config.appearance.page_animation,
            "font_family": config.appearance.font_family,
        },
        "strings": config.appearance.strings,
        "install_options": {
            "default_dir": config.install.default_dir
                .replace("{localappdata}", "%LOCALAPPDATA%")
                .replace("{programfiles}", "%PROGRAMFILES%")
                .replace("{name}", &config.app.name),
            "allow_custom_dir": config.install.allow_custom_dir,
            "desktop_shortcut_default": config.install.shortcuts.desktop_default,
            "desktop_shortcut_optional": config.install.shortcuts.desktop_optional,
            "start_menu_shortcut_default": config.install.shortcuts.start_menu_default,
            "start_menu_shortcut_optional": config.install.shortcuts.start_menu_optional,
            "auto_start_enabled": config.install.auto_start.enabled,
            "auto_start_default": config.install.auto_start.default_value,
            "file_associations": config.install.file_associations,
        },
        "parcel_version": env!("CARGO_PKG_VERSION"),
        "is_preview": true,
    }))
    .unwrap_or_else(|_| "{}".into());

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{name} Setup — Preview</title>
<style>
{css}
</style>
</head>
<body>
<div id="app">
  <aside id="sidebar">
    <div id="sidebar-logo">
      <img id="logo-img" src="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 128 128'%3E%3Crect width='128' height='128' rx='24' fill='%2360CDFF'/%3E%3Ctext x='64' y='80' text-anchor='middle' font-family='Arial' font-size='56' font-weight='bold' fill='white'%3EP%3C/text%3E%3C/svg%3E" alt="Logo">
    </div>
    <div id="sidebar-info">
      <h1 id="app-name"></h1>
      <p id="app-version"></p>
    </div>
    <div id="sidebar-publisher"><span id="publisher-name"></span></div>
  </aside>
  <main id="main-content">
    <section id="page-welcome" class="wizard-page active">
      <div class="page-header"><h2 id="welcome-title"></h2><p id="welcome-subtitle" class="page-description"></p></div>
      <div class="page-body"><p class="welcome-text">This wizard will guide you through the installation process. Click <strong>Next</strong> to continue.</p><div class="preview-badge">PREVIEW MODE — no actual installation</div></div>
    </section>
    <section id="page-eula" class="wizard-page">
      <div class="page-header"><h2 id="eula-title"></h2><p id="eula-scroll-hint" class="page-description"></p></div>
      <div class="page-body"><div id="eula-container" class="eula-box"><pre id="eula-text"></pre></div></div>
    </section>
    <section id="page-options" class="wizard-page">
      <div class="page-header"><h2 id="options-title"></h2></div>
      <div class="page-body">
        <div class="form-group"><label for="install-dir" id="label-install-path">Install Location</label><div class="input-row"><input type="text" id="install-dir" class="form-input"><button id="btn-browse" class="btn btn-outline">Browse...</button></div></div>
        <div class="form-group options-checks">
          <label class="checkbox-label" id="check-desktop-wrap"><input type="checkbox" id="check-desktop" checked><span id="label-desktop-shortcut"></span></label>
          <label class="checkbox-label" id="check-startmenu-wrap"><input type="checkbox" id="check-startmenu" checked><span id="label-startmenu-shortcut"></span></label>
          <label class="checkbox-label" id="check-autostart-wrap"><input type="checkbox" id="check-autostart"><span id="label-autostart"></span></label>
        </div>
      </div>
    </section>
    <section id="page-progress" class="wizard-page">
      <div class="page-header"><h2 id="progress-title"></h2><p id="progress-status" class="page-description">Preparing...</p></div>
      <div class="page-body">
        <div class="progress-bar-container"><div id="progress-bar" class="progress-bar" style="width:0%"></div></div>
        <div class="progress-info"><span id="progress-percent">0%</span><span id="progress-file" class="progress-file"></span></div>
        <details class="file-list-details"><summary>Installed Files</summary><ul id="file-list" class="file-list"></ul></details>
      </div>
    </section>
    <section id="page-finish" class="wizard-page">
      <div class="page-header"><h2 id="finish-title"></h2><p id="finish-subtitle" class="page-description"></p></div>
      <div class="page-body">
        <div class="finish-icon"><svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg></div>
        <label class="checkbox-label launch-check"><input type="checkbox" id="check-launch" checked><span id="label-launch"></span></label>
      </div>
    </section>
  </main>
  <footer id="nav-bar">
    <button id="btn-cancel" class="btn btn-text">Cancel</button>
    <div class="nav-spacer"></div>
    <button id="btn-back" class="btn btn-outline" disabled>Back</button>
    <button id="btn-next" class="btn btn-primary">Next</button>
  </footer>
</div>
<script>
const PREVIEW_CONFIG = {config_json};

// Inline the main.js logic for standalone preview.
const isTauri = false;
async function invoke(cmd, args) {{ return mockInvoke(cmd, args); }}

const PAGES = ["page-welcome","page-eula","page-options","page-progress","page-finish"];
let currentPage = 0, isInstalling = false, config = null;

document.addEventListener("DOMContentLoaded", async () => {{
  config = PREVIEW_CONFIG;
  applyConfig(config);
  document.getElementById("btn-next").addEventListener("click", onNext);
  document.getElementById("btn-back").addEventListener("click", onBack);
  document.getElementById("btn-cancel").addEventListener("click", onCancel);
  document.getElementById("btn-browse").addEventListener("click", () => {{}});
  const ec = document.getElementById("eula-container");
  if (ec) ec.addEventListener("scroll", onEulaScroll);
  updateNavButtons();
}});

function applyConfig(cfg) {{
  if (!cfg) return;
  const root = document.documentElement;
  const c = cfg.appearance?.colors;
  if (c) {{
    if (c.primary) root.style.setProperty("--accent", c.primary);
    if (c.accent) root.style.setProperty("--accent-hover", c.accent);
    if (c.background) root.style.setProperty("--surface-base", c.background);
    if (c.text) root.style.setProperty("--text-primary", c.text);
    if (c.text_secondary) root.style.setProperty("--text-secondary", c.text_secondary);
  }}
  if (cfg.appearance?.border_radius !== undefined)
    root.style.setProperty("--radius-sm", cfg.appearance.border_radius + "px");
  if (cfg.appearance?.font_family)
    root.style.setProperty("--font-family", cfg.appearance.font_family);
  setText("app-name", cfg.app_name);
  setText("app-version", "Version " + cfg.app_version);
  setText("publisher-name", cfg.publisher);
  document.title = cfg.app_name + " Setup — Preview";
  const s = cfg.strings || {{}};
  setText("welcome-title", rv(s.welcome_title, cfg));
  setText("welcome-subtitle", rv(s.welcome_subtitle, cfg));
  setText("eula-title", s.eula_title);
  setText("eula-scroll-hint", s.eula_scroll_hint);
  setText("options-title", s.options_title);
  setText("progress-title", s.progress_title);
  setText("finish-title", s.finish_title);
  setText("finish-subtitle", rv(s.finish_subtitle, cfg));
  setText("label-desktop-shortcut", s.options_desktop_shortcut);
  setText("label-startmenu-shortcut", s.options_start_menu_shortcut);
  setText("label-autostart", s.options_auto_start);
  setText("label-launch", rv(s.finish_launch, cfg));
  setText("btn-back", s.btn_back);
  document.getElementById("btn-next").textContent = s.btn_next;
  document.getElementById("btn-cancel").textContent = s.btn_cancel;
  if (cfg.eula_text) document.getElementById("eula-text").textContent = cfg.eula_text;
  const o = cfg.install_options;
  if (o) {{
    document.getElementById("install-dir").value = o.default_dir;
    document.getElementById("check-desktop").checked = o.desktop_shortcut_default;
    document.getElementById("check-startmenu").checked = o.start_menu_shortcut_default;
    document.getElementById("check-autostart").checked = o.auto_start_default;
    if (!o.allow_custom_dir) {{
      document.getElementById("install-dir").readOnly = true;
      document.getElementById("btn-browse").style.display = "none";
    }}
  }}
}}

function onNext() {{
  if (currentPage === PAGES.length - 1) return;
  if (PAGES[currentPage] === "page-options") {{ simulateInstall(); return; }}
  navigateTo(currentPage + 1);
}}
function onBack() {{ if (currentPage > 0 && !isInstalling) navigateTo(currentPage - 1); }}
function onCancel() {{ if(confirm("Cancel preview?")) window.close(); }}

function navigateTo(i) {{
  if (i < 0 || i >= PAGES.length) return;
  const ce = document.getElementById(PAGES[currentPage]);
  const ne = document.getElementById(PAGES[i]);
  if (ce) {{ ce.classList.remove("active"); ce.classList.add(i > currentPage ? "exit-left" : ""); }}
  setTimeout(() => {{
    if (ce) ce.classList.remove("exit-left");
    if (ne) ne.classList.add("active");
    currentPage = i;
    updateNavButtons();
  }}, 50);
}}

function updateNavButtons() {{
  const bb = document.getElementById("btn-back");
  const bn = document.getElementById("btn-next");
  const bc = document.getElementById("btn-cancel");
  bb.disabled = currentPage === 0 || isInstalling;
  const s = config?.strings || {{}};
  bn.textContent = PAGES[currentPage] === "page-options" ? (s.btn_install||"Install")
    : PAGES[currentPage] === "page-finish" ? (s.btn_finish||"Finish") : (s.btn_next||"Next");
  bn.disabled = false;
  if (PAGES[currentPage] === "page-eula") checkEulaScrolled();
  if (PAGES[currentPage] === "page-finish") {{ bb.style.display="none"; bc.style.display="none"; }}
  else {{ bb.style.display=""; bc.style.display=""; }}
  if (isInstalling) {{ bc.style.display="none"; bb.disabled=true; }}
}}

let eulaBottom = false;
function onEulaScroll() {{
  const c = document.getElementById("eula-container");
  if (!c) return;
  eulaBottom = c.scrollHeight - c.scrollTop - c.clientHeight < 20;
  checkEulaScrolled();
}}
function checkEulaScrolled() {{
  const bn = document.getElementById("btn-next");
  if (PAGES[currentPage] === "page-eula") {{
    const c = document.getElementById("eula-container");
    const need = c && c.scrollHeight > c.clientHeight + 10;
    bn.disabled = need && !eulaBottom;
  }}
}}

let mp = {{percent:0,current_file:"",status:"",installed_files:[],is_complete:false,error:null}};
let mf = ["app.exe","resources/config.json","resources/icon.png","resources/lang/en.json","resources/lang/zh-CN.json"];
let mi = 0;
function simulateInstall() {{
  isInstalling = true; mp = {{percent:0,current_file:"",status:"Preparing...",installed_files:[],is_complete:false,error:null}}; mi = 0;
  navigateTo(3);
  const step = () => {{
    if (mi < mf.length) {{
      mp.current_file = mf[mi]; mp.status = "Installing " + mf[mi] + "...";
      mp.percent = Math.round(((mi+1)/mf.length)*90); mp.installed_files.push(mf[mi]); mi++;
      updateProgress(mp); setTimeout(step, 400);
    }} else {{ mp.percent=100; mp.status="Done!"; mp.is_complete=true; updateProgress(mp); navigateTo(4); isInstalling=false; updateNavButtons(); }}
  }};
  setTimeout(step, 500);
}}
function updateProgress(p) {{
  const bar = document.getElementById("progress-bar");
  const pct = document.getElementById("progress-percent");
  const f = document.getElementById("progress-file");
  const st = document.getElementById("progress-status");
  const fl = document.getElementById("file-list");
  if (bar) bar.style.width = p.percent + "%";
  if (pct) pct.textContent = p.percent + "%";
  if (f) f.textContent = p.current_file;
  if (st) st.textContent = p.status;
  if (fl && p.installed_files) fl.innerHTML = p.installed_files.map(x=>"<li>"+x+"</li>").join("");
}}

function setText(id, t) {{ const e = document.getElementById(id); if (e && t) e.textContent = t; }}
function rv(s, c) {{ return (s||"").replace(/\{{name\}}/g, c.app_name||"").replace(/\{{version\}}/g, c.app_version||""); }}

function mockInvoke(cmd, args) {{
  if (cmd === "get_config") return Promise.resolve(PREVIEW_CONFIG);
  return Promise.resolve(null);
}}
</script>
</body>
</html>"##,
        name = config.app.name,
        css = include_str!("../../src/styles/main.css"),
        config_json = config_json,
    )
}

fn open_in_browser(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", "", &path.to_string_lossy()])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(path)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(path)
            .spawn();
    }
}

use std::path::Path;
