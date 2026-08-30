// Language: ?lang= wins, then a stored choice, then the OS language.
// Switching only swaps text nodes — no reload, no second page to drift.
const STORE_KEY = "magpie.site.lang";

function initialLang() {
  const q = new URLSearchParams(location.search).get("lang");
  if (q === "en" || q === "zh") return q;
  try {
    const saved = localStorage.getItem(STORE_KEY);
    if (saved === "en" || saved === "zh") return saved;
  } catch {
    /* fall through to the OS language */
  }
  return (navigator.language || "en").toLowerCase().startsWith("zh") ? "zh" : "en";
}

function renderCards(lang) {
  for (const [section, items] of Object.entries(window.FEATURES)) {
    const host = document.getElementById(section);
    if (!host || !Array.isArray(items)) continue;
    host.innerHTML = items
      .map(
        (f) => `
        <article class="card">
          <img src="img/${f.img}" alt="${f[lang].t}" loading="lazy" width="740" />
          <h3>${f[lang].t}</h3>
          <p>${f[lang].d}</p>
        </article>`,
      )
      .join("");
  }
  const claims = document.getElementById("g5");
  if (claims) {
    claims.innerHTML = window.FEATURES.g5[lang].map((c) => `<li>${c}</li>`).join("");
  }
}

function apply(lang) {
  const dict = window.CONTENT[lang];
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  for (const el of document.querySelectorAll("[data-i18n]")) {
    const v = dict[el.dataset.i18n];
    if (v != null) el.innerHTML = v;
  }
  renderCards(lang);
  const btn = document.getElementById("lang");
  if (btn) btn.textContent = lang === "zh" ? "English" : "中文";
  document.title =
    lang === "zh"
      ? "magpie — 你存过的一切，一个快捷键之外"
      : "magpie — everything you saved, one keystroke away";
}

let current = initialLang();
apply(current);

document.getElementById("lang")?.addEventListener("click", () => {
  current = current === "zh" ? "en" : "zh";
  try {
    localStorage.setItem(STORE_KEY, current);
  } catch {
    /* the choice just won't persist */
  }
  apply(current);
});
