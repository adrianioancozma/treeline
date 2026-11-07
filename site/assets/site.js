// Theme toggle (remembered, with safe storage access) and the mobile docs navigation.
(function () {
  var root = document.documentElement;
  var KEY = "apmgit-site-theme";
  try {
    var saved = localStorage.getItem(KEY);
    if (saved === "light" || saved === "dark") root.setAttribute("data-theme", saved);
  } catch (e) { /* storage blocked: follow the system theme */ }

  function currentIsDark() {
    var t = root.getAttribute("data-theme");
    if (t) return t === "dark";
    return !(window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches);
  }

  document.addEventListener("click", function (e) {
    var toggle = e.target.closest && e.target.closest(".theme-toggle");
    if (toggle) {
      var next = currentIsDark() ? "light" : "dark";
      root.setAttribute("data-theme", next);
      try { localStorage.setItem(KEY, next); } catch (err) { /* ignore */ }
      return;
    }
    var nav = e.target.closest && e.target.closest(".nav-toggle");
    if (nav) {
      var box = nav.parentElement;
      var open = box.classList.toggle("open");
      nav.setAttribute("aria-expanded", String(open));
    }
  });
})();
