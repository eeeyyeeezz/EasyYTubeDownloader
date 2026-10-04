// Context-menu entries on YouTube pages. The actual hand-off to the app
// happens in the content script, which opens the easyytd:// link.
const MENUS = [
  { id: "eytd-video", preset: null, title: "menuVideo" },
  { id: "eytd-mp3", preset: "mp3", title: "menuMp3" },
];

const YOUTUBE = /^https?:\/\/((www|m|music)\.)?(youtube\.com|youtu\.be)\//i;

chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.removeAll(() => {
    for (const menu of MENUS) {
      chrome.contextMenus.create({
        id: menu.id,
        title: chrome.i18n.getMessage(menu.title),
        contexts: ["page", "link", "video"],
        documentUrlPatterns: ["*://*.youtube.com/*"],
      });
    }
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  const menu = MENUS.find((m) => m.id === info.menuItemId);
  if (!menu || tab?.id == null) return;
  const url = info.linkUrl && YOUTUBE.test(info.linkUrl) ? info.linkUrl : info.pageUrl;
  chrome.tabs.sendMessage(tab.id, { type: "eytd-launch", url, preset: menu.preset });
});
