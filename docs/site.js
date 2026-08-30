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

function renderRows(lang) {
  for (const [section, items] of Object.entries(window.FEATURES)) {
    const host = document.getElementById(section);
    if (!host || !Array.isArray(items)) continue;
    host.innerHTML = items
      .map(
        (f) => `
        <article class="row reveal">
          <div class="row-copy">
            <span class="tag">${f.tag ?? ""}</span>
            <h3>${f[lang].t}</h3>
            <p>${f[lang].d}</p>
          </div>
          <div class="shot">
            <img src="shots/${f.img}" alt="${f[lang].t}" loading="lazy" />
          </div>
        </article>`,
      )
      .join("");
  }
  const claims = document.getElementById("g5");
  if (claims) {
    claims.innerHTML = window.FEATURES.g5[lang]
      .map((c) => `<li class="reveal">${c}</li>`)
      .join("");
  }
  observeReveals();
}

function apply(lang) {
  const dict = window.CONTENT[lang];
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  for (const el of document.querySelectorAll("[data-i18n]")) {
    const v = dict[el.dataset.i18n];
    if (v != null) el.innerHTML = v;
  }
  renderRows(lang);
  const btn = document.getElementById("lang");
  if (btn) btn.textContent = lang === "zh" ? "English" : "中文";
  document.title =
    lang === "zh"
      ? "magpie · 你存过的一切，一个快捷键找回来"
      : "magpie · everything you saved, one keystroke away";
}

// Reveal-on-scroll. The observer handles the common case; a scroll-driven
// sweep is the safety net, because a programmatic jump (or a browser
// restoring a scroll position) can land past elements before the observer's
// first callback ever runs — those would sit invisible forever.
let io;
function sweepReveals() {
  for (const el of document.querySelectorAll(".reveal:not(.in)")) {
    const b = el.getBoundingClientRect();
    if (b.top < innerHeight * 0.94 && b.bottom > 0) el.classList.add("in");
  }
}
function observeReveals() {
  io?.disconnect();
  io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) {
          e.target.classList.add("in");
          io.unobserve(e.target);
        }
      }
    },
    { rootMargin: "0px 0px -6% 0px", threshold: 0.05 },
  );
  for (const el of document.querySelectorAll(".reveal:not(.in)")) io.observe(el);
}
addEventListener("scroll", sweepReveals, { passive: true });
addEventListener("resize", sweepReveals, { passive: true });
// a browser restoring a scroll position lands mid-page without firing a
// scroll event; one late sweep covers that without polling
addEventListener("load", () => setTimeout(sweepReveals, 120));

let current = initialLang();
apply(current);

document.getElementById("lang")?.addEventListener("click", () => {
  current = current === "zh" ? "en" : "zh";
  try {
    localStorage.setItem(STORE_KEY, current);
  } catch {
    /* the choice just won't persist */
  }
  // keep the reader where they were: re-render swaps every row node, so
  // without this the page would jump as the new content settles
  const y = window.scrollY;
  apply(current);
  window.scrollTo({ top: y, behavior: "instant" });
  // rows already on screen must not sit at opacity 0 waiting for a scroll.
  // Wait a frame first: adding the class in the same frame as the insert
  // merges both styles into one paint and the transition never runs.
  requestAnimationFrame(() => requestAnimationFrame(sweepReveals));
});

// the header grows a hairline once the hero scrolls past
const header = document.querySelector(".top");
const onScroll = () => header?.classList.toggle("scrolled", window.scrollY > 12);
onScroll();
addEventListener("scroll", onScroll, { passive: true });
