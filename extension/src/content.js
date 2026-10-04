// Adds "Download" / "MP3" buttons to YouTube video pages and hands the
// current URL to the EasyYTubeDownloader app through its easyytd:// link.
(() => {
  const msg = (key) => chrome.i18n.getMessage(key) || key;

  function appLink(url, preset) {
    const query = new URLSearchParams({ url });
    if (preset) query.set("preset", preset);
    return `easyytd://download?${query}`;
  }

  function launch(url, preset) {
    if (url) window.location.href = appLink(url, preset);
  }

  function videoUrl(href = location.href) {
    try {
      const u = new URL(href);
      if (!/(^|\.)youtube\.com$/.test(u.hostname) && u.hostname !== "youtu.be") return null;
      if (u.hostname === "youtu.be" && u.pathname.length > 1) return u.href;
      if (u.pathname === "/watch" && u.searchParams.get("v")) return u.href;
      if (u.pathname.startsWith("/shorts/") || u.pathname === "/playlist") return u.href;
    } catch {
      // not a URL
    }
    return null;
  }

  function button(className, label, title, onClick) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = `eytd-btn ${className}`;
    b.textContent = label;
    b.title = title;
    b.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      onClick();
    });
    return b;
  }

  function makeButtons() {
    const wrap = document.createElement("div");
    wrap.id = "eytd-buttons";
    wrap.className = "eytd-wrap";
    wrap.append(
      button("eytd-main", `⬇ ${msg("download")}`, msg("downloadTitle"), () => launch(videoUrl())),
      button("eytd-mp3", `♪ ${msg("downloadMp3")}`, msg("mp3Title"), () => launch(videoUrl(), "mp3")),
    );
    return wrap;
  }

  function place() {
    const existing = document.getElementById("eytd-buttons");
    const url = videoUrl();
    if (!url || location.pathname === "/playlist") {
      existing?.remove();
      return;
    }
    const isShorts = location.pathname.startsWith("/shorts/");
    const anchor = isShorts
      ? null
      : document.querySelector("ytd-watch-metadata #actions-inner, ytd-watch-metadata #actions");
    if (existing) {
      const placed = anchor
        ? existing.parentElement === anchor
        : existing.classList.contains("eytd-floating");
      if (placed) return;
      existing.remove();
    }
    const el = makeButtons();
    if (anchor) {
      anchor.prepend(el);
    } else {
      el.classList.add("eytd-floating");
      document.body.append(el);
    }
  }

  let timer = 0;
  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(place, 250);
  }

  new MutationObserver(schedule).observe(document.body, { childList: true, subtree: true });
  document.addEventListener("yt-navigate-finish", schedule);
  schedule();

  chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (message?.type === "eytd-launch") {
      launch(videoUrl(message.url) ?? videoUrl(), message.preset);
      sendResponse({ ok: true });
    }
  });
})();
