// On-demand KaTeX loader and renderer for chat messages.

var KATEX_VERSION = '0.16.21';
var KATEX_CSS_URL = 'https://cdn.jsdelivr.net/npm/katex@' + KATEX_VERSION + '/dist/katex.min.css';
var KATEX_JS_URL = 'https://cdn.jsdelivr.net/npm/katex@' + KATEX_VERSION + '/dist/katex.min.js';
var KATEX_AUTORENDER_URL =
  'https://cdn.jsdelivr.net/npm/katex@' + KATEX_VERSION + '/dist/contrib/auto-render.min.js';
var katexLoadPromise = null;

function hasLatexDelimiters(text) {
  if (!text) return false;
  return /\$\$|\\\[|\\\(|\$(?=\S)[^$\n]+\$/.test(text);
}

function loadScript(url) {
  return new Promise(function (resolve, reject) {
    var script = document.createElement('script');
    script.src = url;
    script.async = true;
    script.onload = function () {
      resolve();
    };
    script.onerror = function () {
      reject(new Error('Failed to load script: ' + url));
    };
    document.head.appendChild(script);
  });
}

function ensureKatexLoaded() {
  if (typeof renderMathInElement === 'function') return Promise.resolve(true);
  if (katexLoadPromise) return katexLoadPromise;

  katexLoadPromise = new Promise(function (resolve) {
    var cssId = 'openfang-katex-css';
    if (!document.getElementById(cssId)) {
      var link = document.createElement('link');
      link.id = cssId;
      link.rel = 'stylesheet';
      link.href = KATEX_CSS_URL;
      document.head.appendChild(link);
    }

    loadScript(KATEX_JS_URL)
      .then(function () {
        return loadScript(KATEX_AUTORENDER_URL);
      })
      .then(function () {
        resolve(typeof renderMathInElement === 'function');
      })
      .catch(function () {
        katexLoadPromise = null;
        resolve(false);
      });
  });

  return katexLoadPromise;
}

// Render LaTeX math in the chat message container using KaTeX auto-render.
// Call this after new messages are inserted into the DOM.
function renderLatex(el) {
  var target = el || document.getElementById('messages');
  if (!target) return;
  if (!hasLatexDelimiters(target.textContent || '')) return;

  ensureKatexLoaded().then(function (ok) {
    if (!ok || typeof renderMathInElement !== 'function') return;
    try {
      renderMathInElement(target, {
        delimiters: [
          { left: '$$', right: '$$', display: true },
          { left: '\\[', right: '\\]', display: true },
          { left: '$', right: '$', display: false },
          { left: '\\(', right: '\\)', display: false },
        ],
        throwOnError: false,
        trust: false,
      });
    } catch (e) {
      /* KaTeX render error — ignore gracefully */
    }
  });
}
