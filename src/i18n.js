// goglite — prosty mechanizm wielojęzyczności (PL / EN)
// Słowniki + tłumaczenie atrybutów [data-i18n] oraz tekstów generowanych w JS.

const DICT = {
  pl: {
    "app.title": "goglite — instalatory offline GOG",
    // Nawigacja
    "nav.library": "Biblioteka",
    "nav.downloads": "Pobierania",
    "nav.settings": "Ustawienia",
    // Akcje
    "action.sync": "Synchronizuj",
    "action.logout": "Wyloguj",
    "topbar.search": "Szukaj w bibliotece…",
    // Powitanie
    "welcome.title": "Witaj w goglite",
    "welcome.desc1": "Lekki menedżer pobierania instalatorów offline z GOG.com.",
    "welcome.desc2": "Zaloguj się przez okno GOG, a następnie zsynchronizuj bibliotekę.",
    "welcome.login": "Zaloguj przez GOG",
    // Synchronizacja / biblioteka
    "sync.syncing": "Synchronizowanie biblioteki…",
    "library.title": "Moja biblioteka",
    "library.emptyPrefix": "Brak gier. Kliknij ",
    "library.emptyAction": "Synchronizuj",
    "library.emptySuffix": ".",
    "library.loadError": "Błąd wczytywania biblioteki",
    "library.noResults": "Brak wyników.",
    // Pobierania
    "downloads.title": "Pobierania",
    "downloads.empty": "Brak pobierań.",
    "downloads.folderPrefix": "Folder zapisu:",
    "downloads.changeInSettings": "Zmień w ustawieniach",
    // Ustawienia
    "settings.title": "Ustawienia",
    "settings.language": "Język",
    "settings.language.desc": "Wybierz język interfejsu aplikacji.",
    "settings.theme": "Motyw",
    "settings.theme.desc": "Wybierz jasny lub ciemny wygląd aplikacji.",
    "settings.theme.light": "Jasny",
    "settings.theme.dark": "Ciemny",
    "settings.dir": "Folder zapisu pobrań",
    "settings.dir.desc": "Miejsce, do którego zapisywane są instalatory.",
    "settings.dir.change": "Zmień folder…",
    "settings.about": "O aplikacji",
    "settings.about.desc": "Lekki menedżer instalatorów offline GOG.",
    // Uwierzytelnianie
    "auth.online": "Online",
    "auth.offline": "Offline",
    "auth.opening": "Otwieranie okna logowania…",
    "auth.loginError": "Błąd logowania",
    "auth.unknown": "nieznany błąd",
    // Statusy pobierań
    "status.pending": "Oczekuje",
    "status.downloading": "Pobieranie",
    "status.paused": "Wstrzymane",
    "status.completed": "Ukończone",
    "status.failed": "Błąd",
    "status.cancelled": "Anulowane",
    // Modal
    "modal.loading": "Ładowanie…",
    "modal.noInstallers": "Brak instalatorów dla tej gry.",
    "modal.download": "Pobierz",
    "modal.genre": "Gatunek",
    "modal.developer": "Producent",
    "modal.publisher": "Wydawca",
    "modal.releaseDate": "Premiera",
    "modal.installers": "Instalatory",
    "modal.tabDescription": "Opis",
    "modal.tabInstallers": "Instalatory",
    "modal.screenshots": "Zrzuty ekranu",
    "modal.noDescription": "Brak opisu dla tej gry.",
    // Pobieranie (akcje)
    "dl.prepare": "Przygotowywanie…",
    "dl.stop": "Zatrzymaj",
    "dl.resume": "Wznów",
    "dl.cancel": "Anuluj",
    "error.prefix": "Błąd",
    // Filtr platformy (co można zainstalować na Windows/Linux/macOS)
    "filter.all": "Wszystkie",
    "filter.windows": "Windows",
    "filter.linux": "Linux",
    "filter.macos": "macOS",
    "modal.noInstallersForPlatform": "Brak instalatorów dla wybranej platformy.",
  },
  en: {
    "app.title": "goglite — GOG offline installers",
    "nav.library": "Library",
    "nav.downloads": "Downloads",
    "nav.settings": "Settings",
    "action.sync": "Sync",
    "action.logout": "Log out",
    "topbar.search": "Search library…",
    "welcome.title": "Welcome to goglite",
    "welcome.desc1": "A lightweight manager for downloading GOG.com offline installers.",
    "welcome.desc2": "Sign in through the GOG window, then sync your library.",
    "welcome.login": "Sign in with GOG",
    "sync.syncing": "Syncing library…",
    "library.title": "My library",
    "library.emptyPrefix": "No games. Click ",
    "library.emptyAction": "Sync",
    "library.emptySuffix": ".",
    "library.loadError": "Failed to load library",
    "library.noResults": "No results.",
    "downloads.title": "Downloads",
    "downloads.empty": "No downloads.",
    "downloads.folderPrefix": "Save folder:",
    "downloads.changeInSettings": "Change in settings",
    "settings.title": "Settings",
    "settings.language": "Language",
    "settings.language.desc": "Choose the application interface language.",
    "settings.theme": "Theme",
    "settings.theme.desc": "Choose a light or dark appearance.",
    "settings.theme.light": "Light",
    "settings.theme.dark": "Dark",
    "settings.dir": "Download folder",
    "settings.dir.desc": "Location where installers are saved.",
    "settings.dir.change": "Change folder…",
    "settings.about": "About",
    "settings.about.desc": "A lightweight manager for GOG offline installers.",
    "auth.online": "Online",
    "auth.offline": "Offline",
    "auth.opening": "Opening login window…",
    "auth.loginError": "Login error",
    "auth.unknown": "unknown error",
    "status.pending": "Pending",
    "status.downloading": "Downloading",
    "status.paused": "Paused",
    "status.completed": "Completed",
    "status.failed": "Failed",
    "status.cancelled": "Cancelled",
    "modal.loading": "Loading…",
    "modal.noInstallers": "No installers for this game.",
    "modal.download": "Download",
    "modal.genre": "Genre",
    "modal.developer": "Developer",
    "modal.publisher": "Publisher",
    "modal.releaseDate": "Release date",
    "modal.installers": "Installers",
    "modal.tabDescription": "Description",
    "modal.tabInstallers": "Installers",
    "modal.screenshots": "Screenshots",
    "modal.noDescription": "No description for this game.",
    "dl.prepare": "Preparing…",
    "dl.stop": "Stop",
    "dl.resume": "Resume",
    "dl.cancel": "Cancel",
    "error.prefix": "Error",
    "filter.all": "All",
    "filter.windows": "Windows",
    "filter.linux": "Linux",
    "filter.macos": "macOS",
    "modal.noInstallersForPlatform": "No installers for the selected platform.",
  },
};

export const LANGUAGES = ["pl", "en"];
let current = "pl";

/** Normalizuje dowolny kod języka (np. `pl-PL`, `en-US`) do `"pl"`/`"en"` lub `null`. */
export function normalizeLang(raw) {
  if (!raw) return null;
  const l = String(raw).toLowerCase();
  if (l.startsWith("pl")) return "pl";
  if (l.startsWith("en")) return "en";
  return null;
}

/** Zwraca bieżący kod języka. */
export function currentLang() {
  return current;
}

/** Tłumaczy klucz; gdy brak, zwraca `fallback` lub sam klucz. */
export function t(key, fallback) {
  const dict = DICT[current] || DICT.pl;
  return dict[key] != null ? dict[key] : (fallback != null ? fallback : key);
}

/** Podmienia teksty statyczne (`[data-i18n]`), placeholdery i title oraz tytuł okna i `lang`. */
export function applyTranslations(root = document) {
  root.querySelectorAll("[data-i18n]").forEach((el) => {
    el.textContent = t(el.getAttribute("data-i18n"));
  });
  root.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
    el.setAttribute("placeholder", t(el.getAttribute("data-i18n-placeholder")));
  });
  root.querySelectorAll("[data-i18n-title]").forEach((el) => {
    el.setAttribute("title", t(el.getAttribute("data-i18n-title")));
  });
  document.title = t("app.title");
  document.documentElement.lang = current;
  syncLangSwitch();
}

/** Ustawia język (waliduje kod) i natychmiast stosuje tłumaczenia do DOM. */
export function setLanguage(lang) {
  const norm = normalizeLang(lang);
  current = norm || "pl";
  applyTranslations();
  return current;
}

/** Podświetla aktywny przycisk w przełączniku języka (jeśli istnieje). */
function syncLangSwitch() {
  document.querySelectorAll("#lang-switch [data-lang]").forEach((btn) => {
    btn.classList.toggle("active", btn.getAttribute("data-lang") === current);
  });
}
