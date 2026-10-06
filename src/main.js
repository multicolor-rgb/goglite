// goglite — frontend (invoke, listen, render) + wielojęzyczność (PL/EN)
// Aplikacja ogranicza się do przeglądania biblioteki GOG i pobierania instalatorów offline.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t, setLanguage, currentLang } from "./i18n.js";

/** Wykrywa system operacyjny (Windows/Linux/macOS) — do domyślnego filtra platformy. */
const detectOs = () => invoke("detect_os");

const $ = (s) => document.querySelector(s);
const show = (e) => e.classList.remove("hidden");
const hide = (e) => e.classList.add("hidden");

const d = {
  // sidebar / nawigacja
  navGames: $("#nav-games"), navDls: $("#nav-downloads"), navSettings: $("#nav-settings"),
  // topbar
  searchInput: $("#search-input"), topSettingsBtn: $("#btn-settings"),
  // auth
  loginBtn: $("#btn-login"), loginError: $("#login-error"), welcome: $("#welcome"),
  logoutBtn: $("#btn-logout"), syncBtn: $("#btn-sync"), statusBadge: $("#status-badge"),
  // biblioteka
  gameSection: $("#game-section"), gameGrid: $("#game-grid"),
  gameEmpty: $("#game-empty"), gameCount: $("#game-count"),
  platformFilter: $("#platform-filter"),
  syncBar: $("#sync-progress"), syncProg: $("#sync-progress-bar"),
  // pobierania
  dlList: $("#download-list"), dlBadge: $("#dl-badge"),
  dlDirLabel: $("#dl-dir-label"), dlSettingsBtn: $("#btn-dl-settings"),
  // ustawienia
  setDirLabel: $("#set-dir-label"), chooseDirBtn: $("#btn-choose-dir"), langSwitch: $("#lang-switch"),
  themeSwitch: $("#theme-switch"),
  // widoki
  viewGames: $("#view-games"), viewDls: $("#view-downloads"), viewSettings: $("#view-settings"),
  // modal
  modal: $("#game-modal"), modalTitle: $("#modal-title"), modalHeroImg: $("#modal-hero-img"),
  modalBody: $("#modal-body"), closeModalBtn: $("#btn-close-modal"), modalTabs: $("#modal-tabs"),
  screenshotViewer: $("#screenshot-viewer"), screenshotViewerImg: $("#screenshot-viewer-img"),
  screenshotViewerClose: $("#screenshot-viewer-close"),
};

let loggedIn = false;
/** id pobierania -> pełny wiersz (scalamy z cząstkowymi zdarzeniami postępu). */
const dlRows = new Map();
/** Aktualnie ustawiony folder zapisu (pusty = trzeba go wybrać przed pobraniem). */
let currentDir = "";
/** Pełna lista gier z bazy (do filtrowania wyszukiwarką i platformą). */
let gamesCache = [];
/** Zapytanie wyszukiwarki (małe litery). */
let searchQuery = "";
/** Bieżący motyw interfejsu: "dark" lub "light". */
let currentTheme = "dark";
/** Aktywna zakładka modala: "info" (opis + zrzuty) lub "installers" (instalatory). */
let activeModalTab = "info";
/** System operacyjny z backendu: "linux" | "windows" | "macos". */
let currentOs = "linux";
/** Filtr platformy w bibliotece: "all" | "windows" | "linux" | "macos". */
let platformFilter = "all";
/** Filtr platformy na liście instalatorów w modalu: "all" | "windows" | "linux" | "macos". */
let installerFilter = "all";
/** Instalatory aktualnie otwartej gry (do filtrowania bez ponownego zapytania). */
let modalInstallers = [];
/** Id gry aktualnie otwartej w modalu. */
let currentGameId = null;

const VIEWS = {
  games: { view: d.viewGames, nav: d.navGames },
  downloads: { view: d.viewDls, nav: d.navDls },
  settings: { view: d.viewSettings, nav: d.navSettings },
};

/** Przełącza aktywny widok i podświetla odpowiednią pozycję nawigacji. */
function switchView(v) {
  Object.values(VIEWS).forEach((x) => { x.view.classList.remove("active"); x.nav.classList.remove("active"); });
  const target = VIEWS[v] || VIEWS.games;
  target.view.classList.add("active");
  target.nav.classList.add("active");
}

/** Etykieta statusu pobierania w bieżącym języku. */
function statusLabel(s) { return t("status." + s, s); }

async function init() {
  // Nawigacja
  d.navGames.addEventListener("click", () => switchView("games"));
  d.navDls.addEventListener("click", () => switchView("downloads"));
  d.navSettings.addEventListener("click", () => switchView("settings"));
  d.topSettingsBtn.addEventListener("click", () => switchView("settings"));
  d.dlSettingsBtn.addEventListener("click", () => switchView("settings"));
  // Akcje
  d.loginBtn.addEventListener("click", doSetToken);
  d.logoutBtn.addEventListener("click", doLogout);
  d.syncBtn.addEventListener("click", sync);
  d.closeModalBtn.addEventListener("click", closeModal);
  d.modal.addEventListener("click", (e) => { if (e.target === d.modal) closeModal(); });
  // Zakładki modala (Opis / Instalatory)
  d.modalTabs.addEventListener("click", (e) => {
    const btn = e.target.closest(".modal-tab");
    if (btn) switchModalTab(btn.getAttribute("data-tab"));
  });
  // Podgląd zrzutów ekranu (lightbox wewnątrz modala)
  d.screenshotViewerClose.addEventListener("click", closeScreenshotViewer);
  d.screenshotViewer.addEventListener("click", closeScreenshotViewer);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !d.screenshotViewer.classList.contains("hidden")) {
      e.preventDefault();
      closeScreenshotViewer();
    }
  });
  d.chooseDirBtn.addEventListener("click", () => chooseDownloadDir());
  // Wyszukiwarka
  d.searchInput.addEventListener("input", () => {
    searchQuery = d.searchInput.value.trim().toLowerCase();
    renderGames();
  });
  // Przełącznik języka
  d.langSwitch.addEventListener("click", (e) => {
    const btn = e.target.closest("[data-lang]");
    if (btn) applyLanguage(btn.getAttribute("data-lang"), true);
  });
  // Przełącznik motywu (jasny / ciemny)
  d.themeSwitch.addEventListener("click", (e) => {
    const btn = e.target.closest("[data-theme-opt]");
    if (btn) applyTheme(btn.getAttribute("data-theme-opt"), true);
  });
  // Filtr platformy w bibliotece (Wszystkie / Windows / Linux / macOS)
  if (d.platformFilter) {
    d.platformFilter.addEventListener("click", (e) => {
      const btn = e.target.closest("[data-platform]");
      if (btn) selectPlatformFilter(btn.getAttribute("data-platform"));
    });
  }

  // Język: zapisane ustawienie > język systemu (fallback pl).
  let savedLang = null, savedTheme = null;
  try { const st = await invoke("get_settings"); savedLang = st && st.language; savedTheme = st && st.theme; } catch (e) {}
  applyLanguage(savedLang || navigator.language, false);
  // Motyw: zapisane ustawienie > domyślny ciemny.
  applyTheme(savedTheme || "dark", false);

  // Wykrycie systemu — domyślnie zaznaczamy platformę bieżącego systemu w filtrze.
  try { const info = await detectOs(); if (info && info.os) currentOs = info.os; } catch (e) {}
  selectPlatformFilter(currentOs);

  try { const st = await invoke("get_auth_status"); loggedIn = st.loggedIn; } catch (e) {}
  renderAuth();
  await refreshDownloadDir();
  await loadDownloads();
  if (loggedIn) { await loadGames(); show(d.syncBtn); }

  listen("auth:changed", (e) => {
    loggedIn = e.payload.loggedIn; renderAuth();
    if (loggedIn) { hide(d.welcome); show(d.syncBtn); sync(); }
    else { gamesCache = []; show(d.welcome); hide(d.gameSection); hide(d.gameEmpty); hide(d.syncBtn); }
  });
  listen("auth:error", (e) => {
    const msg = (e.payload && e.payload.message) || t("auth.unknown");
    d.loginError.textContent = t("auth.loginError") + ": " + msg;
    show(d.loginError);
    d.loginBtn.disabled = false; d.loginBtn.textContent = t("welcome.login");
  });
  listen("library:page", (e) => { show(d.syncBar); d.syncProg.value = e.payload.progress || 0; });
  listen("download:progress", (e) => {
    const row = dlRows.get(e.payload.id);
    if (!row) return;
    Object.assign(row, e.payload);
    const el = document.querySelector(`[data-dl-id="${e.payload.id}"]`);
    if (!el) return;
    // Postęp zmienia tylko pasek/liczby — nie przebudowujemy przycisków (stabilne klikanie).
    if (el.dataset.status === row.status) patchDlProgress(el, row);
    else paintDl(el, row);
  });
  listen("download:state", () => loadDownloads());
}

/** Ustawia język, tłumaczy cały interfejs i (opcjonalnie) zapisuje wybór. */
function applyLanguage(lang, persist) {
  const norm = setLanguage(lang);
  renderAuth();
  if (loggedIn) renderGames();
  renderDownloads();
  setDirLabel(currentDir);
  if (d.modal.open) {
    const loadingEl = d.modalBody.querySelector(".loading");
    if (loadingEl) loadingEl.textContent = t("modal.loading");
  }
  if (persist) { invoke("set_language", { language: norm }).catch(() => {}); }
}

/** Ustawia motyw (`"light"`/`"dark"`), podświetla przełącznik i (opcjonalnie) zapisuje wybór. */
function applyTheme(theme, persist) {
  const norm = theme === "light" ? "light" : "dark";
  currentTheme = norm;
  document.documentElement.setAttribute("data-theme", norm);
  document.querySelectorAll("#theme-switch [data-theme-opt]").forEach((btn) => {
    btn.classList.toggle("active", btn.getAttribute("data-theme-opt") === norm);
  });
  if (persist) { invoke("set_theme", { theme: norm }).catch(() => {}); }
}

function renderAuth() {
  if (loggedIn) {
    show(d.logoutBtn); show(d.syncBtn); hide(d.welcome);
    d.statusBadge.textContent = t("auth.online"); d.statusBadge.classList.add("badge-online"); show(d.statusBadge);
  } else {
    hide(d.logoutBtn); show(d.welcome);
    d.statusBadge.textContent = t("auth.offline"); d.statusBadge.classList.remove("badge-online"); show(d.statusBadge);
  }
}

async function doSetToken() {
  hide(d.loginError);
  d.loginBtn.disabled = true; d.loginBtn.textContent = t("auth.opening");
  try {
    await invoke("login_via_www");
  } catch (e) {
    d.loginError.textContent = t("error.prefix") + ": " + String(e); show(d.loginError);
  } finally {
    d.loginBtn.disabled = false; d.loginBtn.textContent = t("welcome.login");
  }
}

async function doLogout() {
  try {
    await invoke("logout"); loggedIn = false; gamesCache = [];
    renderAuth(); d.gameGrid.innerHTML = "";
  } catch (e) { console.error(e); }
}

async function sync() {
  if (!loggedIn) return;
  show(d.syncBar);
  try { await invoke("sync_library"); await loadGames(); }
  catch (e) {
    console.error("sync:", e);
    if (String(e).toLowerCase().includes("auth")) { loggedIn = false; renderAuth(); }
    else {
      d.gameEmpty.innerHTML = `<p class="error">${t("library.loadError")}: ${esc(String(e))}</p>`;
      show(d.gameEmpty); show(d.gameSection);
    }
  }
  finally { hide(d.syncBar); }
}

async function loadGames() {
  let games;
  try { games = await invoke("get_games"); } catch (e) { games = []; }
  gamesCache = Array.isArray(games) ? games : [];
  renderGames();
}

/** Rysuje siatkę gier (filtr wyszukiwarki + filtr platformy). */
function renderGames() {
  const q = searchQuery;
  // Filtr platformy stosujemy tylko wtedy, gdy API dostarczyło dane o platformach.
  const usePlatform = platformFilter !== "all" && platformDataAvailable();
  const list = gamesCache.filter((g) => {
    if (q && !(g.title || "").toLowerCase().includes(q)) return false;
    if (usePlatform && !(Array.isArray(g.platforms) && g.platforms.includes(platformFilter))) return false;
    return true;
  });
  d.gameGrid.innerHTML = "";
  if (gamesCache.length === 0) {
    d.gameCount.textContent = "";
    d.gameEmpty.innerHTML = `<p>${t("library.emptyPrefix")}<strong>${t("library.emptyAction")}</strong>${t("library.emptySuffix")}</p>`;
    show(d.gameEmpty); hide(d.gameSection);
    return;
  }
  show(d.gameSection);
  d.gameCount.textContent = String(list.length);
  if (list.length === 0) {
    d.gameEmpty.innerHTML = `<p>${t("library.noResults")}</p>`;
    show(d.gameEmpty);
    return;
  }
  hide(d.gameEmpty);
  for (const g of list) {
    const card = document.createElement("div");
    card.className = "game-card"; card.dataset.gameId = g.id;
    const src = g.imageUrl || g.iconUrl;
    const img = src
      ? `<img class="game-card-cover" src="${esc(src)}" alt="${esc(g.title)}" loading="lazy">`
      : `<div class="game-card-fallback">\uD83C\uDFAE</div>`;
    const genre = g.genre ? `<div class="game-card-genre">${esc(g.genre)}</div>` : "";
    card.innerHTML = `${img}${genre}${platformBadgesHtml(g.platforms)}<div class="game-card-title">${esc(g.title)}</div>`;
    card.addEventListener("click", () => openGame(g));
    d.gameGrid.appendChild(card);
  }
}

/** Czy którykolwiek wiersz biblioteki zawiera informację o platformach. */
function platformDataAvailable() {
  return gamesCache.some((g) => Array.isArray(g.platforms) && g.platforms.length);
}

/** Znaczniki platform (Win/Lin/Mac) w rogu kafelka gry. */
function platformBadgesHtml(platforms) {
  if (!Array.isArray(platforms) || !platforms.length) return "";
  const parts = [];
  if (platforms.includes("windows")) parts.push(`<span class="plat-tag plat-windows">Win</span>`);
  if (platforms.includes("linux")) parts.push(`<span class="plat-tag plat-linux">Lin</span>`);
  if (platforms.includes("macos")) parts.push(`<span class="plat-tag plat-macos">Mac</span>`);
  return parts.length ? `<div class="game-card-platforms">${parts.join("")}</div>` : "";
}

/** Ustawia aktywny filtr platformy w bibliotece i odświeża siatkę. */
function selectPlatformFilter(platform) {
  platformFilter = platform || "all";
  if (d.platformFilter) {
    d.platformFilter.querySelectorAll("[data-platform]").forEach((b) =>
      b.classList.toggle("active", b.getAttribute("data-platform") === platformFilter));
  }
  renderGames();
}

async function openGame(game) {
  currentGameId = String(game.id);
  const image = game.imageUrl || game.iconUrl;
  d.modalTitle.textContent = game.title;
  if (image) { d.modalHeroImg.src = image; d.modalHeroImg.classList.remove("hidden"); }
  else { d.modalHeroImg.removeAttribute("src"); d.modalHeroImg.classList.add("hidden"); }
  d.modalBody.innerHTML = `<p class="loading">${t("modal.loading")}</p>`;
  closeScreenshotViewer();
  switchModalTab("info");
  d.modal.showModal();
  // Metadane (opis, producenci, wydawcy, gatunki) + instalatory — równolegle.
  const [metaRes, instRes] = await Promise.allSettled([
    invoke("get_game_details", { gameId: game.id, language: currentLang() }),
    invoke("get_installers", { gameId: game.id }),
  ]);
  const meta = metaRes.status === "fulfilled" ? metaRes.value : null;
  const installers = instRes.status === "fulfilled" ? instRes.value : null;
  const instError = instRes.status === "rejected" ? instRes.reason : null;
  modalInstallers = Array.isArray(installers) ? installers : [];
  installerFilter = "all";
  renderModal(game, meta, installers, instError);
}

/**
 * Buduje treść modala w dwóch zakładkach:
 *  - „info” — metadane, opis oraz zrzuty ekranu (jeśli API je zwraca),
 *  - „installers” — lista instalatorów do pobrania (filtr i sortowanie wg platformy).
 */
function renderModal(game, meta, installers, installersError) {
  const devs = (meta && meta.developers && meta.developers.length) ? meta.developers : (game.developers || []);
  const pubs = (meta && meta.publishers && meta.publishers.length) ? meta.publishers : (game.publishers || []);
  const genres = (meta && meta.genres && meta.genres.length) ? meta.genres : (game.genre ? [game.genre] : []);
  const description = (meta && meta.description) || "";
  const screenshots = (meta && Array.isArray(meta.screenshots)) ? meta.screenshots : [];

  const metaRows = [];
  if (genres.length) metaRows.push(metaRow("modal.genre", genres.join(", ")));
  if (devs.length) metaRows.push(metaRow("modal.developer", devs.join(", ")));
  if (pubs.length) metaRows.push(metaRow("modal.publisher", pubs.join(", ")));
  if (game.releaseDate) metaRows.push(metaRow("modal.releaseDate", dateOnly(game.releaseDate)));

  // ---- zakładka „Opis” ----
  let info = "";
  if (metaRows.length) info += `<div class="modal-meta">${metaRows.join("")}</div>`;
  if (description) info += `<p class="modal-description">${esc(description)}</p>`;
  if (screenshots.length) {
    info += `<h3 class="modal-section-title">${t("modal.screenshots")}</h3>`;
    info += `<div class="screenshot-grid">${screenshots.map(screenshotHtml).join("")}</div>`;
  }
  if (!info) info = `<p class="empty-state">${t("modal.noDescription")}</p>`;

  // ---- zakładka „Instalatory” ----
  const hasInstallers = !installersError && installers && installers.length;
  let inst = "";
  if (installersError) {
    inst = `<p class="error">${t("error.prefix")}: ${esc(String(installersError))}</p>`;
  } else if (!installers || !installers.length) {
    inst = `<p class="empty-state">${t("modal.noInstallers")}</p>`;
  } else {
    inst = `<div class="platform-filter" id="inst-filter">` +
      platformChip("all", t("filter.all")) +
      platformChip("windows", t("filter.windows")) +
      platformChip("linux", t("filter.linux")) +
      platformChip("macos", t("filter.macos")) +
      `</div><div id="inst-list" class="installer-list"></div>`;
  }

  d.modalBody.innerHTML =
    `<div class="modal-panel" data-panel="info">${info}</div>` +
    `<div class="modal-panel hidden" data-panel="installers">${inst}</div>`;

  if (hasInstallers) {
    renderInstallerList(game.id);
    d.modalBody.querySelector("#inst-filter").addEventListener("click", (e) => {
      const btn = e.target.closest("[data-platform]");
      if (!btn) return;
      installerFilter = btn.getAttribute("data-platform") || "all";
      d.modalBody.querySelector("#inst-filter").querySelectorAll("[data-platform]").forEach((b) =>
        b.classList.toggle("active", b === btn));
      renderInstallerList(game.id);
    });
  }

  // Podgląd zrzutów ekranu
  d.modalBody.querySelectorAll(".screenshot-thumb").forEach((img) =>
    img.addEventListener("click", () => openScreenshotViewer(img.dataset.full)));

  closeScreenshotViewer();
  switchModalTab(activeModalTab);
}

/** Przycisk filtra platformy (z podświetleniem aktywnego wyboru instalatorów). */
function platformChip(value, label) {
  const active = installerFilter === value ? " active" : "";
  return `<button class="chip${active}" data-platform="${value}">${esc(label)}</button>`;
}

/** Rysuje listę instalatorów w modalu (filtr + sortowanie wg platformy). */
function renderInstallerList(gameId) {
  const list = d.modalBody.querySelector("#inst-list");
  if (!list) return;
  const items = sortInstallers(modalInstallers)
    .filter((i) => installerFilter === "all" || normOs(i.os) === installerFilter);
  if (!items.length) {
    list.innerHTML = `<p class="empty-state">${t("modal.noInstallersForPlatform")}</p>`;
    return;
  }
  list.innerHTML = items.map((i) => installerItemHtml(i, gameId)).join("");
  list.querySelectorAll(".dl-t").forEach((b) =>
    b.addEventListener("click", () => startDownload(parseInt(b.dataset.game), b.dataset.inst, b)));
}

/** Sortuje instalatory: Windows → Linux → macOS → inne, a następnie po nazwie. */
function sortInstallers(items) {
  const rank = (os) => ({ windows: 0, linux: 1, macos: 2 }[normOs(os)] ?? 3);
  return [...items].sort((a, b) =>
    rank(a.os) - rank(b.os) || String(a.name || "").localeCompare(String(b.name || "")));
}

/** Normalizuje oznaczenie systemu instalatora do `"windows"` / `"linux"` / `"macos"`. */
function normOs(os) {
  const s = String(os || "").toLowerCase();
  if (s.startsWith("win")) return "windows";
  if (s.startsWith("lin")) return "linux";
  if (s.startsWith("mac") || s.startsWith("osx") || s.startsWith("darwin")) return "macos";
  return s;
}

/** HTML pojedynczego zrzutu ekranu (miniatura; klik otwiera podgląd). */
function screenshotHtml(s) {
  const thumb = s.thumb || s.full || "";
  const full = s.full || s.thumb || "";
  return `<img class="screenshot-thumb" src="${esc(thumb)}" data-full="${esc(full)}" alt="" loading="lazy">`;
}

/** Przełącza aktywną zakładkę modala i pokazuje odpowiadający jej panel. */
function switchModalTab(name) {
  activeModalTab = name === "installers" ? "installers" : "info";
  d.modalTabs.querySelectorAll(".modal-tab").forEach((b) =>
    b.classList.toggle("active", b.getAttribute("data-tab") === activeModalTab));
  d.modalBody.querySelectorAll(".modal-panel").forEach((p) =>
    p.classList.toggle("hidden", p.getAttribute("data-panel") !== activeModalTab));
}

/** Otwiera podgląd zrzutu ekranu w pełnym rozmiarze. */
function openScreenshotViewer(url) {
  if (!url) return;
  d.screenshotViewerImg.src = url;
  show(d.screenshotViewer);
}

/** Zamyka podgląd zrzutu ekranu. */
function closeScreenshotViewer() {
  hide(d.screenshotViewer);
  d.screenshotViewerImg.removeAttribute("src");
}

/** Pojedyncza metadana (etykieta + wartość) w nagłówku modala. */
function metaRow(labelKey, value) {
  return `<span class="meta-item"><span class="meta-label">${t(labelKey)}</span>${esc(value)}</span>`;
}

/** Zwraca samą datę (RRRR-MM-DD), obcinając ewentualną godzinę/strefę. */
function dateOnly(s) {
  const str = String(s == null ? "" : s);
  const m = str.match(/^\d{4}-\d{2}-\d{2}/);
  return m ? m[0] : str;
}

/** HTML pojedynczego instalatora na liście w modalu. */
function installerItemHtml(i, gameId) {
  return `
    <div class="installer-item">
      <div class="installer-main">
        <h4>${esc(i.name)}</h4>
        <div class="installer-meta">
          <span class="tag">${esc((i.os || "").toUpperCase())}</span>
          <span>${fmtSize(i.size)}</span>
          <span>v${esc(i.version || "?")}</span>
          <span>${esc(i.language || "en")}</span>
        </div>
      </div>
      <button class="btn btn-primary dl-t" data-game="${gameId}" data-inst="${esc(i.id)}">${t("modal.download")}</button>
    </div>`;
}

function closeModal() { closeScreenshotViewer(); d.modal.close(); }

/* ---------------------------- folder zapisu ---------------------------- */

async function refreshDownloadDir() {
  try { const dir = await invoke("get_download_dir"); setDirLabel(dir); } catch (e) {}
}

/** Aktualizuje znaczniki ścieżki (w widoku Pobierań i w Ustawieniach). */
function setDirLabel(dir) {
  currentDir = dir || "";
  const value = dir || "—";
  if (d.dlDirLabel) { d.dlDirLabel.textContent = t("downloads.folderPrefix") + " " + value; d.dlDirLabel.title = dir || ""; }
  if (d.setDirLabel) { d.setDirLabel.textContent = value; d.setDirLabel.title = dir || ""; }
}

/** Otwiera okno wyboru folderu; zwraca wybraną ścieżkę lub `null` (anulowano). */
async function chooseDownloadDir() {
  try {
    const dir = await invoke("pick_download_dir");
    if (dir) { setDirLabel(dir); return dir; }
    return null;
  } catch (e) { console.error("pick dir:", e); return null; }
}

/** Nowe pobranie: jeśli folder nie jest jeszcze ustawiony, najpierw o niego pytamy. */
async function startDownload(gid, iid, btn) {
  if (!currentDir) {
    const dir = await chooseDownloadDir();
    if (dir === null) return; // anulowano wybór folderu
  }
  if (btn) { btn.disabled = true; btn.textContent = t("dl.prepare"); }
  try {
    await invoke("start_download", { gameId: gid, installerId: iid });
    if (d.modal.open) closeModal();
    await loadDownloads();
    switchView("downloads");
  } catch (e) {
    console.error("download:", e);
    if (String(e).toLowerCase().includes("auth")) { loggedIn = false; renderAuth(); }
    else if (d.modal.open) { d.modalBody.innerHTML = `<p class="error">${t("error.prefix")}: ${esc(String(e))}</p>`; }
  } finally {
    if (btn) { btn.disabled = false; btn.textContent = t("modal.download"); }
  }
}

/** Wznowienie istniejącego pobrania (bez pytania o folder). */
async function resumeDl(gid, iid) {
  try {
    await invoke("start_download", { gameId: gid, installerId: iid });
    await loadDownloads();
    switchView("downloads");
  } catch (e) {
    console.error(e);
    if (String(e).toLowerCase().includes("auth")) { loggedIn = false; renderAuth(); }
  }
}

async function pauseDl(id) { try { await invoke("pause_download", { downloadId: id }); } catch (e) {} await loadDownloads(); }
async function cancelDl(id) { try { await invoke("cancel_download", { downloadId: id }); } catch (e) {} await loadDownloads(); }

async function loadDownloads() {
  let rows;
  try { rows = await invoke("get_downloads"); } catch (e) { rows = []; }
  dlRows.clear();
  for (const r of rows) dlRows.set(r.id, r);
  renderDownloads();
}

function renderDownloads() {
  d.dlList.innerHTML = "";
  const rows = [...dlRows.values()].sort((a, b) => b.id - a.id);
  if (!rows.length) { d.dlList.innerHTML = `<p class="empty-state">${t("downloads.empty")}</p>`; hide(d.dlBadge); return; }
  const active = rows.filter((x) => x.status === "downloading").length;
  d.dlBadge.textContent = active > 0 ? String(active) : "\u2713";
  show(d.dlBadge);
  for (const x of rows) {
    const el = document.createElement("div");
    el.className = "download-item"; el.dataset.dlId = x.id;
    paintDl(el, x);
    d.dlList.appendChild(el);
  }
}

/** Procent postępu (0–100) dla wiersza pobierania. */
function dlPct(row) {
  let pct = row.progress != null ? row.progress : (row.totalBytes > 0 ? (row.downloadedBytes / row.totalBytes * 100) : 0);
  if (!isFinite(pct) || pct < 0) pct = row.status === "completed" ? 100 : 0;
  return pct;
}

/** Klasa CSS opisująca stan wiersza. */
function dlClass(status) {
  return ["completed", "failed", "paused", "cancelled"].includes(status) ? status : "active";
}

/**
 * Pełny render kafelka pobierania (pasek + liczby + przyciski akcji).
 * Wywoływany przy budowie listy oraz gdy zmieni się status.
 */
function paintDl(el, row) {
  const pct = Math.min(dlPct(row), 100);
  const cls = dlClass(row.status);
  const busy = row.status === "downloading" || row.status === "pending";
  el.dataset.status = row.status;
  el.innerHTML = `
    <div class="dl-head">
      <div class="dl-title">${esc(row.gameTitle)}</div>
      <div class="dl-status ${cls}">${statusLabel(row.status)}</div>
    </div>
    <div class="dl-filename">${esc(row.filename)}</div>
    <div class="dl-progress-track"><div class="dl-progress-fill ${cls}" style="width:${pct}%"></div></div>
    <div class="dl-info">
      <span>${fmtSize(row.downloadedBytes)} / ${fmtSize(row.totalBytes)}</span>
      <span class="dl-pct">${pct.toFixed(1)}%</span>
    </div>
    ${row.error ? `<div class="dl-error">${esc(row.error)}</div>` : ""}
    <div class="dl-actions">
      ${busy ? `<button class="btn btn-small" onclick="pauseDl(${row.id})">${t("dl.stop")}</button>` : ""}
      ${(row.status === "paused" || row.status === "failed") ? `<button class="btn btn-small btn-primary" onclick="resumeDl(${row.gameId},'${esc(row.installerId)}')">${t("dl.resume")}</button>` : ""}
      ${(busy || row.status === "paused" || row.status === "failed") ? `<button class="btn btn-small" onclick="cancelDl(${row.id})">${t("dl.cancel")}</button>` : ""}
    </div>`;
}

/**
 * Lekka aktualizacja postępu — zmienia tylko szerokość paska i liczby,
 * nie dotykając przycisków (dzięki temu klikanie jest stabilne i pasek nie miga).
 */
function patchDlProgress(el, row) {
  const pct = Math.min(dlPct(row), 100);
  const fill = el.querySelector(".dl-progress-fill");
  if (fill) fill.style.width = pct + "%";
  const info = el.querySelector(".dl-info");
  if (info) {
    info.innerHTML = `<span>${fmtSize(row.downloadedBytes)} / ${fmtSize(row.totalBytes)}</span>` +
      `<span class="dl-pct">${pct.toFixed(1)}%</span>`;
  }
}

function esc(s) { return !s ? "" : String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#039;" })[c]); }
function fmtSize(b) { b = parseInt(b) || 0; if (b < 1024) return b + " B"; if (b < 1048576) return (b/1024).toFixed(1) + " KiB"; if (b < 1073741824) return (b/1048576).toFixed(1) + " MiB"; return (b/1073741824).toFixed(2) + " GiB"; }

window.pauseDl = pauseDl;
window.cancelDl = cancelDl;
window.resumeDl = resumeDl;
window.closeModal = closeModal;
document.addEventListener("DOMContentLoaded", init);