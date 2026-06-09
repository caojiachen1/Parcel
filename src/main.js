/**
 * Parcel Installer — Main Application Script (ES Module)
 *
 * Manages the wizard page flow, communicates with the Tauri backend
 * via @tauri-apps/api, and applies runtime configuration to the UI.
 */

// ── Tauri IPC bridge ──────────────────────────────────────────────────

let tauriInvoke = null;
let tauriOpen = null;
let tauriGetCurrentWindow = null;

/**
 * Lazily load Tauri APIs at runtime.
 * Called inside DOMContentLoaded so we avoid top-level await (not supported
 * by older build targets like safari13).
 */
async function loadTauriApis() {
  try {
    // @tauri-apps/api — core IPC
    const core = await import("https://esm.sh/@tauri-apps/api@2/core");
    tauriInvoke = core.invoke;

    // @tauri-apps/plugin-dialog — native dialogs
    const dialog = await import("https://esm.sh/@tauri-apps/plugin-dialog@2");
    tauriOpen = dialog.open;

    // @tauri-apps/api — window management
    const window = await import("https://esm.sh/@tauri-apps/api@2/window");
    tauriGetCurrentWindow = window.getCurrentWindow;
  } catch (_) {
    // Running outside Tauri (browser preview) — fall back to mock.
  }
}

/**
 * Invoke a Tauri backend command.
 * Falls back to mock data when running in a browser (preview mode).
 */
async function invoke(cmd, args = {}) {
  if (tauriInvoke) {
    try {
      return await tauriInvoke(cmd, args);
    } catch (e) {
      console.warn(`Tauri invoke "${cmd}" failed:`, e);
      return mockInvoke(cmd, args);
    }
  }
  return mockInvoke(cmd, args);
}

// ── Page definitions ────────────────────────────────────────────────────

const PAGES = ["page-welcome", "page-eula", "page-options", "page-progress", "page-finish"];
let currentPage = 0;
let isInstalling = false;
let config = null;

// ── Initialisation ──────────────────────────────────────────────────────

document.addEventListener("DOMContentLoaded", async () => {
  // Load Tauri APIs first (no-op in browser preview).
  await loadTauriApis();

  // Load configuration from the backend.
  try {
    config = await invoke("get_config");
    applyConfig(config);
  } catch (e) {
    console.warn("Failed to load config, using defaults:", e);
  }

  // Bind navigation buttons.
  document.getElementById("btn-next").addEventListener("click", onNext);
  document.getElementById("btn-back").addEventListener("click", onBack);
  document.getElementById("btn-cancel").addEventListener("click", onCancel);

  // Bind browse button.
  document.getElementById("btn-browse").addEventListener("click", onBrowse);

  // Bind EULA scroll detection.
  const eulaContainer = document.getElementById("eula-container");
  if (eulaContainer) {
    eulaContainer.addEventListener("scroll", onEulaScroll);
  }

  // Show preview badge if in preview mode.
  if (config && config.is_preview) {
    const badge = document.getElementById("preview-badge");
    if (badge) badge.style.display = "inline-block";
  }

  updateNavButtons();
});

// ── Configuration application ───────────────────────────────────────────

function applyConfig(cfg) {
  if (!cfg) return;

  // Apply CSS custom properties from appearance config.
  const root = document.documentElement;
  const colors = cfg.appearance?.colors;
  if (colors) {
    if (colors.primary) root.style.setProperty("--color-primary", colors.primary);
    if (colors.accent) root.style.setProperty("--color-accent", colors.accent);
    if (colors.background) root.style.setProperty("--color-bg", colors.background);
    if (colors.text) root.style.setProperty("--color-text", colors.text);
    if (colors.text_secondary) root.style.setProperty("--color-text-secondary", colors.text_secondary);
  }

  if (cfg.appearance?.border_radius !== undefined) {
    root.style.setProperty("--border-radius", cfg.appearance.border_radius + "px");
  }

  if (cfg.appearance?.font_family) {
    root.style.setProperty("--font-family", cfg.appearance.font_family);
  }

  // Apply theme.
  if (cfg.appearance?.theme === "dark") {
    root.classList.add("theme-dark");
  }

  // Sidebar info.
  setText("app-name", cfg.app_name);
  setText("app-version", `Version ${cfg.app_version}`);
  setText("publisher-name", cfg.publisher);

  // Window title.
  document.title = `${cfg.app_name} Setup`;

  // Welcome page strings.
  const s = cfg.strings;
  if (s) {
    setText("welcome-title", replaceVars(s.welcome_title, cfg));
    setText("welcome-subtitle", replaceVars(s.welcome_subtitle, cfg));
    setText("eula-title", s.eula_title);
    setText("eula-scroll-hint", s.eula_scroll_hint);
    setText("options-title", s.options_title);
    setText("progress-title", s.progress_title);
    setText("finish-title", s.finish_title);
    setText("finish-subtitle", replaceVars(s.finish_subtitle, cfg));
    setText("label-desktop-shortcut", s.options_desktop_shortcut);
    setText("label-startmenu-shortcut", s.options_start_menu_shortcut);
    setText("label-autostart", s.options_auto_start);
    setText("label-launch", replaceVars(s.finish_launch, cfg));

    // Button text.
    setText("btn-back", s.btn_back);
    document.getElementById("btn-next").textContent = s.btn_next;
    document.getElementById("btn-cancel").textContent = s.btn_cancel;
  }

  // EULA text — always update to replace the "Loading..." placeholder,
  // even when eula_text is an empty string (which is falsy in JS).
  const eulaEl = document.getElementById("eula-text");
  if (eulaEl) {
    eulaEl.textContent = cfg.eula_text || "No license agreement configured.";
  }

  // Install options.
  const opts = cfg.install_options;
  if (opts) {
    const dirInput = document.getElementById("install-dir");
    if (dirInput) dirInput.value = opts.default_dir;

    const checkDesktop = document.getElementById("check-desktop");
    if (checkDesktop) checkDesktop.checked = opts.desktop_shortcut_default;
    if (!opts.desktop_shortcut_optional && checkDesktop) {
      checkDesktop.parentElement.style.display = "none";
    }

    const checkStartMenu = document.getElementById("check-startmenu");
    if (checkStartMenu) checkStartMenu.checked = opts.start_menu_shortcut_default;
    if (!opts.start_menu_shortcut_optional && checkStartMenu) {
      checkStartMenu.parentElement.style.display = "none";
    }

    const checkAutoStart = document.getElementById("check-autostart");
    if (checkAutoStart) checkAutoStart.checked = opts.auto_start_default;
    if (!opts.auto_start_enabled && checkAutoStart) {
      checkAutoStart.parentElement.style.display = "none";
    }

    if (!opts.allow_custom_dir) {
      dirInput.readOnly = true;
      document.getElementById("btn-browse").style.display = "none";
    }
  }
}

// ── Navigation ──────────────────────────────────────────────────────────

function onNext() {
  if (currentPage === PAGES.length - 1) {
    // Finish page — close.
    onFinish();
    return;
  }

  // Special handling for "Next" on options page → start install.
  if (PAGES[currentPage] === "page-options") {
    onStartInstall();
    return;
  }

  navigateTo(currentPage + 1);
}

function onBack() {
  if (currentPage > 0 && !isInstalling) {
    navigateTo(currentPage - 1);
  }
}

async function onCancel() {
  if (isInstalling) {
    const confirmed = confirm(
      config?.strings?.cancel_confirm_message || "Are you sure you want to cancel?"
    );
    if (!confirmed) return;
    invoke("cancel_install").catch(() => {});
  }
  // Close the window.
  try {
    if (tauriGetCurrentWindow) {
      const win = tauriGetCurrentWindow();
      await win.close();
    } else {
      window.close();
    }
  } catch (_) {
    window.close();
  }
}

async function onFinish() {
  const launch = document.getElementById("check-launch")?.checked ?? true;
  try {
    // Backend handles both launch and window close.
    await invoke("finish_install", { launchApp: launch });
  } catch (e) {
    console.warn("finish_install failed:", e);
  }
}

function navigateTo(index) {
  if (index < 0 || index >= PAGES.length) return;
  if (index === currentPage) return;

  const currentEl = document.getElementById(PAGES[currentPage]);
  const nextEl = document.getElementById(PAGES[index]);
  if (!currentEl || !nextEl) return;

  // Clean up any leftover transition classes from a previous navigation
  // (e.g. if the user clicks Back rapidly).
  PAGES.forEach(p => {
    const el = document.getElementById(p);
    if (el) el.classList.remove("exit-left");
  });

  // Step 1: Start exit transition on the current page (forward only).
  if (index > currentPage) {
    currentEl.classList.add("exit-left");
  }
  currentEl.classList.remove("active");

  // Step 2: Wait for the exit to render, then activate the new page.
  // Double-rAF guarantees the browser paints between DOM mutations —
  // much more reliable than setTimeout in WebView2.
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      // Clean up old page state.
      currentEl.classList.remove("exit-left");

      // Activate new page.
      nextEl.classList.add("active");
      currentPage = index;
      updateNavButtons();
    });
  });
}

function updateNavButtons() {
  const btnBack = document.getElementById("btn-back");
  const btnNext = document.getElementById("btn-next");
  const btnCancel = document.getElementById("btn-cancel");

  // Back button.
  btnBack.disabled = currentPage === 0 || isInstalling;

  // Next button text and state.
  const s = config?.strings || {};
  if (PAGES[currentPage] === "page-options") {
    btnNext.textContent = s.btn_install || "Install";
  } else if (PAGES[currentPage] === "page-finish") {
    btnNext.textContent = s.btn_finish || "Finish";
  } else {
    btnNext.textContent = s.btn_next || "Next";
  }

  btnNext.disabled = false;

  // On EULA page, disable Next until scrolled to bottom.
  if (PAGES[currentPage] === "page-eula") {
    checkEulaScrolled();
  }

  // Hide back/cancel on finish page.
  if (PAGES[currentPage] === "page-finish") {
    btnBack.style.display = "none";
    btnCancel.style.display = "none";
  } else {
    btnBack.style.display = "";
    btnCancel.style.display = "";
  }

  // Hide cancel during installation.
  if (isInstalling) {
    btnCancel.style.display = "none";
    btnBack.disabled = true;
  }
}

// ── EULA scroll detection ───────────────────────────────────────────────

let eulaScrolledToBottom = false;

function onEulaScroll() {
  const container = document.getElementById("eula-container");
  if (!container) return;

  const threshold = 20;
  const atBottom =
    container.scrollHeight - container.scrollTop - container.clientHeight < threshold;

  eulaScrolledToBottom = atBottom;
  checkEulaScrolled();
}

function checkEulaScrolled() {
  const btnNext = document.getElementById("btn-next");
  if (PAGES[currentPage] === "page-eula") {
    // If EULA text is short enough to not need scrolling, allow Next immediately.
    const container = document.getElementById("eula-container");
    const needsScroll = container && container.scrollHeight > container.clientHeight + 10;
    btnNext.disabled = needsScroll && !eulaScrolledToBottom;
  }
}

// ── Installation ────────────────────────────────────────────────────────

async function onStartInstall() {
  // Use snake_case to match Rust struct field names for Tauri IPC.
  const options = {
    install_dir: document.getElementById("install-dir").value,
    desktop_shortcut: document.getElementById("check-desktop").checked,
    start_menu_shortcut: document.getElementById("check-startmenu").checked,
    auto_start: document.getElementById("check-autostart").checked,
  };

  isInstalling = true;
  navigateTo(3); // Jump to progress page.

  try {
    await invoke("start_install", { options });
    pollProgress();
  } catch (e) {
    console.error("Failed to start installation:", e);
    setStatus("Error: " + e);
    isInstalling = false;
    updateNavButtons();
  }
}

async function pollProgress() {
  const interval = setInterval(async () => {
    try {
      const progress = await invoke("get_install_progress");
      updateProgressUI(progress);

      if (progress.is_complete || progress.error) {
        clearInterval(interval);
        isInstalling = false;

        if (progress.error) {
          showError(progress.error);
        } else {
          // Move to finish page.
          navigateTo(4);
        }
      }
    } catch (e) {
      clearInterval(interval);
      isInstalling = false;
      console.error("Progress poll error:", e);
    }
  }, 200);
}

function updateProgressUI(progress) {
  const bar = document.getElementById("progress-bar");
  const percent = document.getElementById("progress-percent");
  const file = document.getElementById("progress-file");
  const status = document.getElementById("progress-status");
  const list = document.getElementById("file-list");

  if (bar) bar.style.width = progress.percent + "%";
  if (percent) percent.textContent = progress.percent + "%";
  if (file) file.textContent = progress.current_file;
  if (status) status.textContent = progress.status;

  if (list && progress.installed_files) {
    list.innerHTML = progress.installed_files
      .map((f) => `<li>${escapeHtml(f)}</li>`)
      .join("");
  }
}

function setStatus(text) {
  const status = document.getElementById("progress-status");
  if (status) status.textContent = text;
}

function showError(message) {
  setStatus("Error: " + message);
  const btnNext = document.getElementById("btn-next");
  btnNext.textContent = config?.strings?.btn_finish || "Close";
  btnNext.disabled = false;
  btnNext.onclick = async () => {
    try {
      if (tauriGetCurrentWindow) {
        const win = tauriGetCurrentWindow();
        await win.close();
      } else {
        window.close();
      }
    } catch (_) {
      window.close();
    }
  };
}

// ── Browse directory ────────────────────────────────────────────────────

async function onBrowse() {
  const currentDir = document.getElementById("install-dir").value;
  try {
    // Try using the Tauri dialog plugin first (frontend API).
    if (tauriOpen) {
      const selected = await tauriOpen({
        directory: true,
        multiple: false,
        defaultPath: currentDir,
      });
      if (selected) {
        document.getElementById("install-dir").value = selected;
      }
      return;
    }
    // Fall back to backend command.
    const selected = await invoke("browse_directory", { defaultPath: currentDir });
    if (selected) {
      document.getElementById("install-dir").value = selected;
    }
  } catch (e) {
    // User cancelled or error — ignore.
  }
}

// ── Utility ─────────────────────────────────────────────────────────────

function setText(id, text) {
  const el = document.getElementById(id);
  if (el && text) el.textContent = text;
}

function replaceVars(str, cfg) {
  if (!str) return "";
  return str
    .replace(/\{name\}/g, cfg.app_name || "")
    .replace(/\{version\}/g, cfg.app_version || "")
    .replace(/\{path\}/g, "");
}

function escapeHtml(str) {
  const div = document.createElement("div");
  div.textContent = str;
  return div.innerHTML;
}

// ── Mock IPC for browser preview ────────────────────────────────────────

function mockInvoke(cmd, args) {
  switch (cmd) {
    case "get_config":
      return Promise.resolve({
        app_name: "MyApp",
        app_version: "1.0.0",
        publisher: "My Company",
        publisher_url: "https://example.com",
        eula_text:
          "END USER LICENSE AGREEMENT\n\n" +
          "This is a preview of the EULA text.\n\n" +
          "1. GRANT OF LICENSE\n" +
          "This software is licensed, not sold.\n\n" +
          "2. RESTRICTIONS\n" +
          "You may not reverse engineer this software.\n\n" +
          "3. DISCLAIMER\n" +
          'THE SOFTWARE IS PROVIDED "AS IS".\n\n' +
          "4. LIMITATION OF LIABILITY\n" +
          "In no event shall the authors be liable for any damages.\n\n" +
          "Please scroll down to enable the Next button.",
        appearance: {
          theme: "dark",
          colors: {
            primary: "#60CDFF",
            accent: "#4DB8E8",
            background: "#202020",
            text: "#F5F5F5",
            text_secondary: "#A0A0A0",
          },
          border_radius: 6,
          page_animation: "fade",
          font_family: "",
        },
        strings: {
          welcome_title: "Welcome to MyApp Setup",
          welcome_subtitle: "Version 1.0.0",
          eula_title: "License Agreement",
          eula_scroll_hint: "Please scroll down to read the entire agreement.",
          options_title: "Installation Options",
          options_install_path: "Install Location",
          options_browse: "Browse...",
          options_desktop_shortcut: "Create desktop shortcut",
          options_start_menu_shortcut: "Create Start Menu shortcut",
          options_auto_start: "Launch at system startup",
          progress_title: "Installing...",
          progress_installing: "Copying files...",
          progress_complete: "Installation complete!",
          finish_title: "Installation Successful",
          finish_subtitle: "MyApp has been installed on your computer.",
          finish_launch: "Launch MyApp",
          btn_next: "Next",
          btn_back: "Back",
          btn_install: "Install",
          btn_finish: "Finish",
          btn_cancel: "Cancel",
          btn_agree: "I Agree",
          btn_disagree: "I Disagree",
          cancel_confirm_title: "Cancel Installation",
          cancel_confirm_message: "Are you sure you want to cancel?",
          overwrite_title: "Existing Installation Detected",
          overwrite_message: "A previous installation was found. Overwrite?",
          error_title: "Installation Error",
        },
        install_options: {
          default_dir: "C:\\Users\\User\\AppData\\Local\\Programs\\MyApp",
          allow_custom_dir: true,
          desktop_shortcut_default: true,
          desktop_shortcut_optional: true,
          start_menu_shortcut_default: true,
          start_menu_shortcut_optional: true,
          auto_start_enabled: true,
          auto_start_default: false,
          file_associations: [],
        },
        parcel_version: "0.1.0",
        is_preview: true,
      });

    case "start_install":
      // Simulate installation progress.
      simulateInstall();
      return Promise.resolve();

    case "get_install_progress":
      return Promise.resolve(getMockProgress());

    case "browse_directory":
      return Promise.resolve("C:\\Users\\User\\AppData\\Local\\Programs\\CustomDir");

    case "cancel_install":
    case "finish_install":
      return Promise.resolve();

    default:
      return Promise.reject(`Unknown command: ${cmd}`);
  }
}

// Mock installation simulation.
let mockProgress = { percent: 0, current_file: "", status: "", installed_files: [], is_complete: false, error: null };
let mockFiles = [
  "app.exe",
  "resources/config.json",
  "resources/icon.png",
  "resources/lang/en.json",
  "resources/lang/zh-CN.json",
  "resources/templates/default.html",
  "README.txt",
];
let mockFileIndex = 0;

function simulateInstall() {
  mockProgress = { percent: 0, current_file: "", status: "Preparing...", installed_files: [], is_complete: false, error: null };
  mockFileIndex = 0;

  const step = () => {
    if (mockFileIndex < mockFiles.length) {
      const file = mockFiles[mockFileIndex];
      mockProgress.current_file = file;
      mockProgress.status = `Installing ${file}...`;
      mockProgress.percent = Math.round(((mockFileIndex + 1) / mockFiles.length) * 90);
      mockProgress.installed_files.push(file);
      mockFileIndex++;
      setTimeout(step, 400);
    } else {
      mockProgress.percent = 100;
      mockProgress.status = "Installation complete!";
      mockProgress.current_file = "";
      mockProgress.is_complete = true;
    }
  };
  setTimeout(step, 500);
}

function getMockProgress() {
  return { ...mockProgress };
}
