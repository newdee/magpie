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

/** Stable anchor for a feature row, derived from its screenshot name. */
const anchorOf = (f) => "f-" + f.img.replace(/\.png$/, "");

function renderRows(lang) {
  for (const [section, items] of Object.entries(window.FEATURES)) {
    const host = document.getElementById(section);
    if (!host || !Array.isArray(items)) continue;
    host.innerHTML = items
      .map(
        (f) => `
        <article class="row reveal" id="${anchorOf(f)}">
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
  const rest = document.getElementById("rest");
  if (rest) {
    rest.innerHTML = window.FEATURES.rest[lang]
      .map(([term, desc]) => `<div class="rest-item reveal"><dt>${term}</dt><dd>${desc}</dd></div>`)
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

// Chapter rail. Listing all 23 entries at once would out-weigh the page, so
// only the chapter you're reading unfolds its features.
const CHAPTERS = [
  { id: "c1", key: "g1.title", features: "g1" },
  { id: "c2", key: "g2.title", features: "g2" },
  { id: "c3", key: "g3.title", features: "g3" },
  { id: "c4", key: "g4.title", features: "g4" },
  { id: "c5", key: "rest.title", features: null },
  { id: "c6", key: "g5.title", features: null },
];

function renderRail(lang) {
  const rail = document.getElementById("rail");
  if (!rail) return;
  const dict = window.CONTENT[lang];
  rail.innerHTML = CHAPTERS.map((ch, i) => {
    const subs = ch.features
      ? window.FEATURES[ch.features]
          .map((f) => `<a class="rail-sub" href="#${anchorOf(f)}">${f[lang].t}</a>`)
          .join("")
      : "";
    return `<div class="rail-chapter" data-target="${ch.id}">
      <a class="rail-link" href="#${ch.id}"><span class="rail-num">${String(i + 1).padStart(2, "0")}</span>${dict[ch.key]}</a>
      <div class="rail-subs"><div>${subs}</div></div>
    </div>`;
  }).join("");
}

// Scroll the rail's targets ourselves. Letting the browser follow the hash
// works when a person clicks, but the scroll and the hash update race each
// other, and this way the sticky header's offset is handled in one place.
document.addEventListener("click", (e) => {
  const link = e.target.closest?.(".rail a[href^='#']");
  if (!link) return;
  const el = document.getElementById(link.getAttribute("href").slice(1));
  if (!el) return;
  e.preventDefault();
  const top = el.getBoundingClientRect().top + window.scrollY - 84;
  window.scrollTo({ top, behavior: "smooth" });
  history.replaceState(null, "", link.getAttribute("href"));
});

/** Highlight the chapter whose section currently owns the upper viewport. */
function syncRail() {
  const rail = document.getElementById("rail");
  if (!rail) return;
  // the chapter that owns the probe line, rather than the last one to cross
  // it: a short final card would otherwise hand the highlight to the next
  // chapter's heading while you're still reading this one
  // near the top of the viewport: a jump lands its target just under the
  // header, and the rail should name the chapter that target belongs to
  const probe = innerHeight * 0.15;
  let active = CHAPTERS[0].id;
  for (const ch of CHAPTERS) {
    const el = document.getElementById(ch.id);
    if (!el) continue;
    const r = el.getBoundingClientRect();
    if (r.top <= probe && r.bottom > probe) {
      active = ch.id;
      break;
    }
    if (r.top <= probe) active = ch.id; // past it; keep as the running best
  }
  // above the first chapter (hero/stats): no chapter is being read yet
  const first = document.getElementById(CHAPTERS[0].id);
  const beforeTour = first && first.getBoundingClientRect().top > innerHeight * 0.35;
  for (const node of rail.querySelectorAll(".rail-chapter")) {
    node.classList.toggle("active", !beforeTour && node.dataset.target === active);
  }
  rail.classList.toggle("dim", !!beforeTour);
}

function apply(lang) {
  const dict = window.CONTENT[lang];
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  for (const el of document.querySelectorAll("[data-i18n]")) {
    const v = dict[el.dataset.i18n];
    if (v != null) el.innerHTML = v;
  }
  renderRows(lang);
  renderRail(lang);
  syncRail();
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
const onScroll = () => {
  header?.classList.toggle("scrolled", window.scrollY > 12);
  syncRail();
};
onScroll();
addEventListener("scroll", onScroll, { passive: true });
