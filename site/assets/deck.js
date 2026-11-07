// Slide deck: arrow keys / space / swipe / buttons, URL hash (#3) for deep links, full screen with F.
(function () {
  var slides = Array.prototype.slice.call(document.querySelectorAll(".slide"));
  if (!slides.length) return;
  var counter = document.getElementById("counter");
  var bar = document.getElementById("progress");
  var current = 0;

  function show(n, updateHash) {
    current = Math.max(0, Math.min(slides.length - 1, n));
    slides.forEach(function (s, i) {
      s.classList.toggle("active", i === current);
      s.classList.toggle("past", i < current);
      s.setAttribute("aria-hidden", i === current ? "false" : "true");
      if (i === current) s.scrollTop = 0;
    });
    counter.textContent = current + 1 + " / " + slides.length;
    bar.style.width = ((current + 1) / slides.length) * 100 + "%";
    if (updateHash !== false) {
      try { history.replaceState(null, "", "#" + (current + 1)); } catch (e) { /* file:// or blocked */ }
    }
  }

  function fromHash() {
    var n = parseInt(location.hash.slice(1), 10);
    return isNaN(n) ? 0 : n - 1;
  }

  function fullscreen() {
    if (document.fullscreenElement) { document.exitFullscreen(); }
    else if (document.documentElement.requestFullscreen) { document.documentElement.requestFullscreen(); }
  }

  document.addEventListener("keydown", function (e) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    var k = e.key;
    if (k === "ArrowRight" || k === "PageDown" || k === " " || k === "Enter") { e.preventDefault(); show(current + 1); }
    else if (k === "ArrowLeft" || k === "PageUp" || k === "Backspace") { e.preventDefault(); show(current - 1); }
    else if (k === "Home") { e.preventDefault(); show(0); }
    else if (k === "End") { e.preventDefault(); show(slides.length - 1); }
    else if (k === "f" || k === "F") { fullscreen(); }
  });

  document.getElementById("next").addEventListener("click", function () { show(current + 1); });
  document.getElementById("prev").addEventListener("click", function () { show(current - 1); });
  document.getElementById("full").addEventListener("click", fullscreen);
  window.addEventListener("hashchange", function () { show(fromHash(), false); });

  // swipe on touch screens
  var startX = null;
  document.addEventListener("touchstart", function (e) { startX = e.changedTouches[0].clientX; }, { passive: true });
  document.addEventListener("touchend", function (e) {
    if (startX === null) return;
    var dx = e.changedTouches[0].clientX - startX;
    if (Math.abs(dx) > 50) show(current + (dx < 0 ? 1 : -1));
    startX = null;
  }, { passive: true });

  show(fromHash(), false);
})();
