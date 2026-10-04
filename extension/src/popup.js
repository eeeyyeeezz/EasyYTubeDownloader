const msg = (key) => chrome.i18n.getMessage(key) || key;
const YOUTUBE_VIDEO =
  /^https?:\/\/((www|m|music)\.)?youtube\.com\/(watch\?|shorts\/|playlist\?)|^https?:\/\/youtu\.be\/./i;

for (const el of document.querySelectorAll("[data-i18n]")) {
  el.textContent = msg(el.dataset.i18n);
}

const status = document.getElementById("status");

chrome.tabs.query({ active: true, currentWindow: true }, ([tab]) => {
  if (!tab?.url || !YOUTUBE_VIDEO.test(tab.url)) {
    status.textContent = msg("popupNotYouTube");
    return;
  }
  document.getElementById("title").textContent = tab.title?.replace(/ - YouTube$/, "") ?? "";
  const actions = document.getElementById("actions");
  actions.hidden = false;
  actions.addEventListener("click", (e) => {
    const preset = e.target.closest("button")?.dataset.preset;
    if (!preset) return;
    chrome.tabs.sendMessage(tab.id, { type: "eytd-launch", url: tab.url, preset });
    status.textContent = msg("popupSent");
    setTimeout(() => window.close(), 1500);
  });
});
