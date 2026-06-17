/**
 * Parcel Setup — Main Application Script
 *
 * Manages the configuration wizard, communicates with the Tauri backend,
 * and handles the build pipeline.
 */

// ── Tauri IPC bridge ──────────────────────────────────────────────────

let tauriInvoke = null;
let tauriListen = null;

async function loadTauriApis() {
  try {
    const core = await import("https://esm.sh/@tauri-apps/api@2/core");
    tauriInvoke = core.invoke;
    const event = await import("https://esm.sh/@tauri-apps/api@2/event");
    tauriListen = event.listen;
  } catch (_) {
    // Running in browser — use mock
  }
}

async function invoke(cmd, args = {}) {
  if (tauriInvoke) {
    try { return await tauriInvoke(cmd, args); }
    catch (e) { console.warn(`invoke "${cmd}" failed:`, e); return mockInvoke(cmd, args); }
  }
  return mockInvoke(cmd, args);
}

// ── State ─────────────────────────────────────────────────────────────

const PAGES = ["page-project", "page-app", "page-files", "page-install", "page-appearance", "page-eula", "page-build"];
const STEP_LABELS = ["Project", "Application", "Files", "Install Options", "Appearance", "License", "Build"];

let currentPage = 0;
let projectPath = "";
let config = null;
let eulaText = "";
let isBuilding = false;
let visitedPages = new Set([0]);

// ── Default config ────────────────────────────────────────────────────

function defaultConfig() {
  return {
    app: { name: "MyApp", version: "0.1.0", identifier: "com.example.myapp", publisher: "My Company", publisher_url: "https://example.com" },
    paths: { target_exe: "src-tauri/target/release/app.exe", resources: [], icon: "icons/icon.png", output_dir: "dist" },
    install: {
      default_dir: "{localappdata}\\Programs\\{name}", allow_custom_dir: true, require_admin: false,
      shortcuts: { desktop_default: true, desktop_optional: true, start_menu_default: true, start_menu_optional: true },
      file_associations: [],
      auto_start: { enabled: true, default_value: false },
    },
    appearance: {
      theme: "dark", colors: { primary: "#60CDFF", accent: "#4DB8E8", background: "#202020", text: "#F5F5F5", text_secondary: "#A0A0A0" },
      logo: "parcel/assets/logo.png", font_family: "", border_radius: 6, page_animation: "fade",
      strings: {},
    },
    eula: { file: null },
    signing: { enabled: false },
  };
}

// ── Initialisation ────────────────────────────────────────────────────

document.addEventListener("DOMContentLoaded", async () => {
  await loadTauriApis();
  config = defaultConfig();

  // Navigation
  document.getElementById("btn-next").addEventListener("click", onNext);
  document.getElementById("btn-back").addEventListener("click", onBack);

  // Step nav clicks
  document.querySelectorAll(".step-item").forEach(el => {
    el.addEventListener("click", () => {
      const step = parseInt(el.dataset.step);
      if (!el.classList.contains("disabled")) navigateTo(step);
    });
  });

  // Project page
  document.getElementById("btn-browse-project").addEventListener("click", onBrowseProject);

  // Files page
  document.getElementById("btn-browse-exe").addEventListener("click", () => browseFile("target-exe", "Executables", ["exe"]));
  document.getElementById("btn-browse-icon").addEventListener("click", () => browseFile("icon-path", "Images", ["png", "ico", "svg"]));
  document.getElementById("btn-add-resource").addEventListener("click", onAddResource);
  document.getElementById("new-resource").addEventListener("keydown", e => { if (e.key === "Enter") onAddResource(); });

  // Install page
  document.getElementById("btn-add-assoc").addEventListener("click", onAddAssociation);

  // Appearance page — color pickers
  setupColorPickers();

  // EULA page
  document.getElementById("btn-browse-eula").addEventListener("click", () => browseFile("eula-file-path", "Text files", ["txt", "md"]));
  document.getElementById("btn-load-eula").addEventListener("click", onLoadEula);

  // Build page
  document.getElementById("btn-save-config").addEventListener("click", onSaveConfig);
  document.getElementById("btn-build").addEventListener("click", onBuild);
  document.getElementById("btn-clear-output").addEventListener("click", () => {
    document.getElementById("build-log").textContent = "";
  });

  // Live preview
  document.getElementById("live-preview-toggle").addEventListener("change", onTogglePreview);
  document.getElementById("btn-close-preview").addEventListener("click", () => {
    document.getElementById("preview-panel").style.display = "none";
    document.getElementById("live-preview-toggle").checked = false;
  });

  // Auto-bind form inputs
  bindFormInputs();
  updateNav();

  // Auto-load project if initial path was provided via CLI argument.
  try {
    const initialPath = await invoke("get_initial_path");
    if (initialPath) {
      projectPath = initialPath;
      document.getElementById("project-path").value = initialPath;
      await loadProjectConfig();
      populateForm();
      navigateTo(1);
    }
  } catch (_) {
    // No initial path — normal startup.
  }
});

// ── Navigation ────────────────────────────────────────────────────────

function onNext() {
  if (currentPage === PAGES.length - 1) { onBuild(); return; }
  if (currentPage === 0 && !projectPath) {
    showProjectError("Please select a project folder first.");
    return;
  }
  navigateTo(currentPage + 1);
}

function onBack() {
  if (currentPage > 0) navigateTo(currentPage - 1);
}

function navigateTo(index) {
  if (index < 0 || index >= PAGES.length || index === currentPage) return;
  if (index === 0 && currentPage !== 0) { /* always allow going back to project */ }

  const currentEl = document.getElementById(PAGES[currentPage]);
  const nextEl = document.getElementById(PAGES[index]);
  if (!currentEl || !nextEl) return;

  // Sync form data before leaving
  syncFormToConfig();

  // Remove stale classes
  PAGES.forEach(p => {
    const el = document.getElementById(p);
    if (el) { el.classList.remove("exit-up"); }
  });

  // Exit animation (forward only)
  if (index > currentPage) {
    currentEl.classList.add("exit-up");
  }
  currentEl.classList.remove("active");

  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      currentEl.classList.remove("exit-up");
      nextEl.classList.add("active");
      currentPage = index;
      visitedPages.add(index);

      // Page-specific setup
      if (PAGES[index] === "page-build") updateBuildSummary();
      if (PAGES[index] === "page-files") renderResources();
      if (PAGES[index] === "page-install") renderAssociations();
      if (PAGES[index] === "page-appearance") syncColorPickers();

      // Populate form from config
      populateForm();
      updateNav();
    });
  });
}

function updateNav() {
  const btnBack = document.getElementById("btn-back");
  const btnNext = document.getElementById("btn-next");

  btnBack.disabled = currentPage === 0;

  if (currentPage === PAGES.length - 1) {
    btnNext.textContent = "Build";
  } else {
    btnNext.textContent = "Next";
  }

  // Update step nav
  document.querySelectorAll(".step-item").forEach(el => {
    const step = parseInt(el.dataset.step);
    el.classList.remove("active", "disabled", "completed");
    if (step === currentPage) el.classList.add("active");
    else if (visitedPages.has(step)) el.classList.add("completed");
    else el.classList.add("disabled");
  });
}

// ── Project page ──────────────────────────────────────────────────────

async function onBrowseProject() {
  const result = await invoke("select_folder", { title: "Select Project Folder" });
  if (result) {
    projectPath = result;
    document.getElementById("project-path").value = result;
    await loadProjectConfig();
  }
}

async function loadProjectConfig() {
  try {
    const jsonStr = await invoke("load_config", { path: projectPath });
    config = JSON.parse(jsonStr);
    // Merge with defaults for any missing fields
    config = { ...defaultConfig(), ...config };
    config.app = { ...defaultConfig().app, ...config.app };
    config.paths = { ...defaultConfig().paths, ...config.paths };
    config.install = { ...defaultConfig().install, ...config.install };
    config.install.shortcuts = { ...defaultConfig().install.shortcuts, ...(config.install.shortcuts || {}) };
    config.install.auto_start = { ...defaultConfig().install.auto_start, ...(config.install.auto_start || {}) };
    config.appearance = { ...defaultConfig().appearance, ...config.appearance };
    config.appearance.colors = { ...defaultConfig().appearance.colors, ...(config.appearance.colors || {}) };
    config.eula = { ...defaultConfig().eula, ...(config.eula || {}) };

    showProjectStatus("success", `Loaded existing parcel.json from ${projectPath}`);

    // Enable all steps
    visitedPages.add(0);
    updateNav();
  } catch (e) {
    config = defaultConfig();
    showProjectStatus("info", `No parcel.json found — a new configuration will be created.`);
  }

  // Auto-extract project info from Tauri project files
  await autoExtractProjectInfo();
}

async function autoExtractProjectInfo() {
  try {
    const info = await invoke("read_project_info", { path: projectPath });
    if (!info) return;

    let extracted = [];

    // Only fill in fields that are currently empty or have default values
    const defaults = defaultConfig();

    if (info.name && (!config.app.name || config.app.name === defaults.app.name)) {
      config.app.name = info.name;
      extracted.push(`name: ${info.name}`);
    }

    if (info.version && (!config.app.version || config.app.version === defaults.app.version)) {
      config.app.version = info.version;
      extracted.push(`version: ${info.version}`);
    }

    if (info.identifier && (!config.app.identifier || config.app.identifier === defaults.app.identifier)) {
      config.app.identifier = info.identifier;
      extracted.push(`identifier: ${info.identifier}`);
    }

    if (info.publisher && (!config.app.publisher || config.app.publisher === defaults.app.publisher)) {
      config.app.publisher = info.publisher;
      extracted.push(`publisher: ${info.publisher}`);
    }

    // Auto-detect exe name from Cargo.toml
    if (info.exe_name) {
      const defaultExe = defaults.paths.target_exe;
      if (!config.paths.target_exe || config.paths.target_exe === defaultExe) {
        config.paths.target_exe = `src-tauri/target/release/${info.exe_name}.exe`;
        extracted.push(`executable: ${info.exe_name}.exe`);
      }
    }

    if (extracted.length > 0) {
      // Update the status message to show what was extracted
      const currentStatus = document.getElementById("project-status-text").textContent;
      showProjectStatus("success", `${currentStatus} Auto-detected: ${extracted.join(", ")}`);
    }

    // Repopulate form with extracted values
    populateForm();
  } catch (e) {
    console.warn("Failed to auto-extract project info:", e);
  }
}

function showProjectStatus(type, message) {
  const card = document.getElementById("project-status");
  const text = document.getElementById("project-status-text");
  card.style.display = "flex";
  card.className = `info-card ${type}`;
  text.textContent = message;
}

function showProjectError(message) {
  showProjectStatus("error", message);
}

// ── Form binding ──────────────────────────────────────────────────────

function bindFormInputs() {
  document.querySelectorAll("[data-bind]").forEach(el => {
    const event = el.type === "checkbox" ? "change" : "input";
    el.addEventListener(event, () => {
      const path = el.dataset.bind;
      const value = el.type === "checkbox" ? el.checked :
                    el.type === "number" ? parseInt(el.value) || 0 : el.value;
      setNestedValue(config, path, value);
    });
  });
}

function populateForm() {
  document.querySelectorAll("[data-bind]").forEach(el => {
    const path = el.dataset.bind;
    const value = getNestedValue(config, path);
    if (value === undefined || value === null) return;
    if (el.type === "checkbox") el.checked = !!value;
    else el.value = value;
  });

  // Special fields
  document.getElementById("project-path").value = projectPath;
}

function syncFormToConfig() {
  document.querySelectorAll("[data-bind]").forEach(el => {
    const path = el.dataset.bind;
    let value;
    if (el.type === "checkbox") {
      value = el.checked;
    } else if (el.type === "number") {
      value = parseInt(el.value) || 0;
    } else {
      value = el.value;
    }
    // Don't overwrite config with empty form values — keep defaults
    if (typeof value === "string" && value.trim() === "") return;
    setNestedValue(config, path, value);
  });

  // EULA text (stored separately, not in config.eula)
  const eulaEl = document.getElementById("eula-text");
  if (eulaEl) eulaText = eulaEl.value;
}

function getNestedValue(obj, path) {
  return path.split(".").reduce((o, k) => o?.[k], obj);
}

function setNestedValue(obj, path, value) {
  const keys = path.split(".");
  let current = obj;
  for (let i = 0; i < keys.length - 1; i++) {
    if (!current[keys[i]]) current[keys[i]] = {};
    current = current[keys[i]];
  }
  current[keys[keys.length - 1]] = value;
}

// ── File browser helpers ──────────────────────────────────────────────

async function browseFile(inputId, filterName, extensions) {
  const result = await invoke("select_file", {
    title: `Select ${filterName}`,
    filters: [[filterName, extensions]],
  });
  if (result) {
    const input = document.getElementById(inputId);
    if (input) {
      // Make path relative to project if possible
      let relPath = result;
      if (projectPath && result.startsWith(projectPath)) {
        relPath = result.slice(projectPath.length + 1).replace(/\\/g, "/");
      }
      input.value = relPath;
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
  }
}

// ── Resource files ────────────────────────────────────────────────────

function renderResources() {
  const list = document.getElementById("resources-list");
  if (!list || !config?.paths?.resources) return;

  list.innerHTML = config.paths.resources.map((r, i) => `
    <span class="tag">
      <span>${escapeHtml(r)}</span>
      <button class="tag-remove" data-index="${i}" title="Remove">&times;</button>
    </span>
  `).join("");

  list.querySelectorAll(".tag-remove").forEach(btn => {
    btn.addEventListener("click", () => {
      const idx = parseInt(btn.dataset.index);
      config.paths.resources.splice(idx, 1);
      renderResources();
    });
  });
}

function onAddResource() {
  const input = document.getElementById("new-resource");
  const val = input.value.trim();
  if (!val) return;
  if (!config.paths.resources) config.paths.resources = [];
  config.paths.resources.push(val);
  input.value = "";
  renderResources();
}

// ── File associations ─────────────────────────────────────────────────

function renderAssociations() {
  const list = document.getElementById("file-associations-list");
  if (!list || !config?.install?.file_associations) return;

  list.innerHTML = config.install.file_associations.map((a, i) => `
    <div class="association-row">
      <span class="ext">.${escapeHtml(a.extension)}</span>
      <span class="desc">${escapeHtml(a.description)}</span>
      <button class="btn btn-text btn-sm assoc-remove" data-index="${i}">&times;</button>
    </div>
  `).join("");

  list.querySelectorAll(".assoc-remove").forEach(btn => {
    btn.addEventListener("click", () => {
      config.install.file_associations.splice(parseInt(btn.dataset.index), 1);
      renderAssociations();
    });
  });
}

function onAddAssociation() {
  const ext = document.getElementById("new-assoc-ext").value.trim().replace(/^\./, "");
  const desc = document.getElementById("new-assoc-desc").value.trim();
  if (!ext) return;
  if (!config.install.file_associations) config.install.file_associations = [];
  config.install.file_associations.push({ extension: ext, description: desc, icon: null });
  document.getElementById("new-assoc-ext").value = "";
  document.getElementById("new-assoc-desc").value = "";
  renderAssociations();
}

// ── Color pickers ─────────────────────────────────────────────────────

function setupColorPickers() {
  const pairs = [
    ["color-primary-picker", "color-primary"],
    ["color-accent-picker", "color-accent"],
    ["color-background-picker", "color-background"],
    ["color-text-picker", "color-text"],
    ["color-text-secondary-picker", "color-text-secondary"],
  ];

  pairs.forEach(([pickerId, inputId]) => {
    const picker = document.getElementById(pickerId);
    const input = document.getElementById(inputId);
    if (!picker || !input) return;

    picker.addEventListener("input", () => {
      input.value = picker.value;
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });

    input.addEventListener("input", () => {
      if (/^#[0-9A-Fa-f]{6}$/.test(input.value)) {
        picker.value = input.value;
      }
    });
  });
}

function syncColorPickers() {
  const pairs = [
    ["color-primary-picker", "color-primary"],
    ["color-accent-picker", "color-accent"],
    ["color-background-picker", "color-background"],
    ["color-text-picker", "color-text"],
    ["color-text-secondary-picker", "color-text-secondary"],
  ];

  pairs.forEach(([pickerId, inputId]) => {
    const picker = document.getElementById(pickerId);
    const input = document.getElementById(inputId);
    if (picker && input && /^#[0-9A-Fa-f]{6}$/.test(input.value)) {
      picker.value = input.value;
    }
  });
}

// ── EULA ──────────────────────────────────────────────────────────────

async function onLoadEula() {
  const filePath = document.getElementById("eula-file-path").value.trim();
  if (!filePath) return;

  const fullPath = /^[A-Z]:\\/i.test(filePath) || filePath.startsWith("/")
    ? filePath
    : projectPath + "/" + filePath;

  try {
    const result = await invoke("read_file", { path: fullPath });
    if (result) {
      document.getElementById("eula-text").value = result;
      eulaText = result;
    }
  } catch (e) {
    alert("Could not load EULA file: " + e);
  }
}

// ── Build page ────────────────────────────────────────────────────────

function updateBuildSummary() {
  syncFormToConfig();

  const appSummary = document.getElementById("summary-app");
  appSummary.innerHTML = `
    <span class="label">Name</span><span class="value">${escapeHtml(config.app.name)}</span>
    <span class="label">Version</span><span class="value">${escapeHtml(config.app.version)}</span>
    <span class="label">Identifier</span><span class="value">${escapeHtml(config.app.identifier)}</span>
    <span class="label">Publisher</span><span class="value">${escapeHtml(config.app.publisher)}</span>
  `;

  const filesSummary = document.getElementById("summary-files");
  filesSummary.innerHTML = `
    <span class="label">Executable</span><span class="value">${escapeHtml(config.paths.target_exe)}</span>
    <span class="label">Resources</span><span class="value">${(config.paths.resources || []).length} pattern(s)</span>
    <span class="label">Output</span><span class="value">${escapeHtml(config.paths.output_dir)}</span>
  `;

  const installSummary = document.getElementById("summary-install");
  installSummary.innerHTML = `
    <span class="label">Default Dir</span><span class="value">${escapeHtml(config.install.default_dir)}</span>
    <span class="label">Custom Path</span><span class="value">${config.install.allow_custom_dir ? "Allowed" : "Not allowed"}</span>
    <span class="label">Admin Required</span><span class="value">${config.install.require_admin ? "Yes" : "No"}</span>
    <span class="label">Associations</span><span class="value">${(config.install.file_associations || []).length}</span>
  `;
}

async function onSaveConfig() {
  syncFormToConfig();

  // Save EULA text to a separate file if provided
  if (eulaText && eulaText.trim()) {
    const eulaFileName = config.eula?.file || "eula.txt";
    config.eula = config.eula || {};
    config.eula.file = eulaFileName;

    const eulaFullPath = projectPath.replace(/[\\/]+$/, "") + "/" + eulaFileName;
    try {
      await invoke("write_file", { path: eulaFullPath, content: eulaText });
    } catch (e) {
      console.warn("Failed to save EULA file:", e);
    }
  } else {
    config.eula = { file: null };
  }

  try {
    await invoke("save_config", {
      path: projectPath,
      configJson: JSON.stringify(config),
    });
    flashSaveIndicator();
  } catch (e) {
    alert("Failed to save configuration: " + e);
  }
}

async function onBuild() {
  if (isBuilding) return;
  if (!projectPath) { alert("Please select a project folder first."); return; }

  // Save config first
  await onSaveConfig();

  isBuilding = true;
  const statusEl = document.getElementById("build-status");
  const statusText = document.getElementById("build-status-text");
  const statusBar = document.getElementById("build-progress-bar");
  const outputEl = document.getElementById("build-output");
  const logEl = document.getElementById("build-log");
  const buildBtn = document.getElementById("btn-build");

  statusEl.style.display = "block";
  outputEl.style.display = "flex";
  statusText.textContent = "Building…";
  statusText.style.color = "var(--accent)";
  statusBar.className = "build-progress-bar indeterminate";
  buildBtn.disabled = true;
  logEl.textContent = "";

  // Listen for build progress events
  let unlisten = null;
  if (tauriListen) {
    unlisten = await tauriListen("build-progress", (event) => {
      const { line, stream } = event.payload;
      const span = document.createElement("span");
      if (stream === "stderr") span.className = "stderr";
      span.textContent = line + "\n";
      logEl.appendChild(span);
      logEl.scrollTop = logEl.scrollHeight;
    });
  }

  try {
    const result = await invoke("build_installer", { projectPath });

    statusBar.className = "build-progress-bar";
    statusBar.style.width = "100%";

    if (result.success) {
      statusText.textContent = "Build successful!";
      statusText.style.color = "var(--success)";
      appendLog(logEl, "\n✓ Build completed successfully.", false);
    } else {
      statusText.textContent = "Build failed";
      statusText.style.color = "var(--error)";
      if (result.stderr) appendLog(logEl, result.stderr, true);
    }
  } catch (e) {
    statusText.textContent = "Build error";
    statusText.style.color = "var(--error)";
    appendLog(logEl, "Error: " + e, true);
  } finally {
    isBuilding = false;
    buildBtn.disabled = false;
    if (unlisten) unlisten();
  }
}

function appendLog(el, text, isStderr) {
  const span = document.createElement("span");
  if (isStderr) span.className = "stderr";
  span.textContent = text + "\n";
  el.appendChild(span);
  el.scrollTop = el.scrollHeight;
}

function flashSaveIndicator() {
  const el = document.getElementById("save-indicator");
  el.style.display = "inline";
  el.style.animation = "none";
  el.offsetHeight; // force reflow
  el.style.animation = "fadeInOut 2s ease forwards";
  setTimeout(() => { el.style.display = "none"; }, 2100);
}

// ── Live preview ──────────────────────────────────────────────────────

function onTogglePreview() {
  const panel = document.getElementById("preview-panel");
  const checked = document.getElementById("live-preview-toggle").checked;
  panel.style.display = checked ? "flex" : "none";
  if (checked) refreshPreview();
}

function refreshPreview() {
  // In a full implementation, this would generate the installer HTML
  // from the config and load it into the iframe. For now, show a placeholder.
  const frame = document.getElementById("preview-frame");
  const doc = frame.contentDocument || frame.contentWindow.document;
  syncFormToConfig();

  const colors = config.appearance.colors;
  doc.open();
  doc.write(`<!DOCTYPE html><html><head><style>
    body { margin:0; padding:40px; font-family:Segoe UI,sans-serif; background:${colors.background}; color:${colors.text}; }
    h1 { font-size:20px; font-weight:600; }
    p { font-size:13px; color:${colors.text_secondary}; margin-top:8px; }
    .card { background:${adjustColor(colors.background, 15)}; border:1px solid rgba(255,255,255,0.08); border-radius:${config.appearance.border_radius}px; padding:16px; margin-top:16px; }
    .btn { display:inline-block; padding:8px 20px; border-radius:4px; background:${colors.primary}; color:#000; font-size:13px; font-weight:600; border:none; margin-top:16px; }
  </style></head><body>
    <h1>Welcome to ${escapeHtml(config.app.name)} Setup</h1>
    <p>Version ${escapeHtml(config.app.version)}</p>
    <div class="card"><p>This wizard will guide you through the installation process.</p></div>
    <button class="btn">Next</button>
  </body></html>`);
  doc.close();
}

function adjustColor(hex, amount) {
  const num = parseInt(hex.slice(1), 16);
  const r = Math.min(255, ((num >> 16) & 0xFF) + amount);
  const g = Math.min(255, ((num >> 8) & 0xFF) + amount);
  const b = Math.min(255, (num & 0xFF) + amount);
  return `rgb(${r},${g},${b})`;
}

// ── Utility ───────────────────────────────────────────────────────────

function escapeHtml(str) {
  if (!str) return "";
  const div = document.createElement("div");
  div.textContent = str;
  return div.innerHTML;
}

// ── Mock IPC (browser preview) ────────────────────────────────────────

function mockInvoke(cmd, args) {
  switch (cmd) {
    case "select_folder":
      return Promise.resolve("/mock/project/path");
    case "select_file":
      return Promise.resolve("/mock/project/src-tauri/target/release/app.exe");
    case "read_project_info":
      return Promise.resolve({
        name: "MyApp",
        version: "1.0.0",
        identifier: "com.example.myapp",
        publisher: "My Company",
        exe_name: "my-app",
      });
    case "load_config":
      return Promise.resolve(JSON.stringify(defaultConfig()));
    case "save_config":
      return Promise.resolve();
    case "list_directory":
      return Promise.resolve([
        { name: "src-tauri", path: "/mock/src-tauri", is_dir: true },
        { name: "package.json", path: "/mock/package.json", is_dir: false },
      ]);
    case "read_file":
      return Promise.resolve("Sample EULA text for preview mode.");
    case "build_installer":
      return new Promise(resolve => {
        setTimeout(() => {
          resolve({ success: true, stdout: "Build complete!\n  Installer: dist/MyApp_Setup.exe\n  Size: 12 MB", stderr: "" });
        }, 2000);
      });
    case "get_initial_path":
      return Promise.resolve(null);
    default:
      return Promise.reject(`Unknown command: ${cmd}`);
  }
}
