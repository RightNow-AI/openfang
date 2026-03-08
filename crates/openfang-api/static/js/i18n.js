'use strict';

(function() {
  var STORAGE_KEY = 'openfang-lang';
  var DEFAULT_LOCALE = 'en';
  var SUPPORTED = ['en', 'zh-CN'];

  var zhMap = {
    'OpenFang Dashboard': 'OpenFang 控制台',

    'API Key Required': '需要 API 密钥',
    'This instance requires an API key. Enter the key from your config.toml.': '该实例需要 API 密钥。请输入你在 config.toml 中配置的密钥。',
    'Enter API key...': '输入 API 密钥...',
    'Unlock Dashboard': '解锁控制台',

    'Light': '浅色',
    'System': '跟随系统',
    'Dark': '深色',

    'Language': '语言',

    'Connecting...': '正在连接...',
    'Reconnecting...': '正在重连...',
    'disconnected': '已断开连接',

    'Chat': '聊天',
    'Monitor': '监控',
    'Overview': '概览',
    'Analytics': '分析',
    'Logs': '日志',
    'Agents': '代理',
    'Sessions': '会话',
    'Approvals': '审批',
    'Automation': '自动化',
    'Workflows': '工作流',
    'Scheduler': '计划任务',
    'Extensions': '扩展',
    'Channels': '渠道',
    'Skills': '技能',
    'Hands': 'Hands',
    'Settings': '设置',

    'Copy': '复制',
    'Copied!': '已复制！',
    'Copied to clipboard': '已复制到剪贴板',
    'Copy failed': '复制失败',

    'Cancel': '取消',
    'Confirm': '确认',

    'Cannot reach daemon — is openfang running?': '无法连接到守护进程 — openfang 是否正在运行？',
    'Not authorized — check your API key': '未授权 — 请检查 API 密钥',
    'Permission denied': '权限不足',
    'Resource not found': '资源不存在',
    'Rate limited — slow down and try again': '触发限流 — 请稍后再试',
    'Request too large': '请求过大',
    'Server error — check daemon logs': '服务器错误 — 请查看守护进程日志',
    'Daemon unavailable — is it running?': '守护进程不可用 — 是否正在运行？',

    'Reconnected': '已重新连接',
    'Connection lost, reconnecting...': '连接已断开，正在重连...',
    'Connection lost — switched to HTTP mode': '连接已断开 — 已切换到 HTTP 模式',

    'Upload failed': '上传失败'
  };

  function normalizeLocale(loc) {
    if (!loc) return DEFAULT_LOCALE;
    var l = String(loc).trim();
    if (l === 'zh' || l.toLowerCase() === 'zh-cn' || l.toLowerCase() === 'zh_cn') return 'zh-CN';
    if (l.toLowerCase().startsWith('zh-')) return 'zh-CN';
    if (SUPPORTED.indexOf(l) >= 0) return l;
    return DEFAULT_LOCALE;
  }

  function getNavigatorLocale() {
    var lang = (navigator.languages && navigator.languages[0]) || navigator.language || navigator.userLanguage;
    return normalizeLocale(lang);
  }

  function getStoredLocale() {
    try { return normalizeLocale(localStorage.getItem(STORAGE_KEY)); } catch (e) { return DEFAULT_LOCALE; }
  }

  var _locale = getStoredLocale() || getNavigatorLocale();
  _locale = normalizeLocale(_locale);
  var _titleOriginal = '';
  var _textOriginal = (typeof WeakMap !== 'undefined') ? new WeakMap() : null;
  var _attrOriginal = (typeof WeakMap !== 'undefined') ? new WeakMap() : null;

  function setLocale(loc) {
    _locale = normalizeLocale(loc);
    try { localStorage.setItem(STORAGE_KEY, _locale); } catch (e) {}

    try {
      document.documentElement.lang = _locale === 'zh-CN' ? 'zh-CN' : 'en';
      if (document.title) {
        if (!_titleOriginal) _titleOriginal = document.title;
        document.title = translateTextForLocale(_titleOriginal, _locale);
      }
    } catch (e) {}
  }

  function getLocale() {
    return _locale;
  }

  function translateExactZh(enText) {
    return zhMap[enText] || enText;
  }

  function translatePatternsZh(text) {
    if (!text) return text;

    var m = String(text).match(/^\s*(\d+)\s+agent\(s\)\s+running\s*$/);
    if (m) return m[1] + ' 个代理运行中';

    if (String(text).startsWith('disconnected — ')) {
      return '已断开连接 — ' + String(text).slice('disconnected — '.length);
    }

    if (/^Error:\s*/.test(String(text))) {
      return String(text).replace(/^Error:\s*/, '错误：');
    }

    if (String(text) === 'never') return '从不';
    if (String(text) === 'just now') return '刚刚';
    if (String(text) === 'in <1m') return '不到 1 分钟后';
    m = String(text).match(/^in\s+(\d+)m$/);
    if (m) return m[1] + ' 分钟后';
    m = String(text).match(/^in\s+(\d+)h$/);
    if (m) return m[1] + ' 小时后';
    m = String(text).match(/^in\s+(\d+)d$/);
    if (m) return m[1] + ' 天后';
    m = String(text).match(/^(\d+)m\s+ago$/);
    if (m) return m[1] + ' 分钟前';
    m = String(text).match(/^(\d+)h\s+ago$/);
    if (m) return m[1] + ' 小时前';
    m = String(text).match(/^(\d+)d\s+ago$/);
    if (m) return m[1] + ' 天前';

    if (String(text).startsWith('Daily at ')) {
      return '每天 ' + String(text).slice('Daily at '.length);
    }
    m = String(text).match(/^(.*)\s+at\s+(\d{1,2}:\d{2}\s+[AP]M)$/);
    if (m) return m[1] + ' ' + m[2];

    m = String(text).match(/^Delete\s+"([^"]+)"\?\s+This cannot be undone\.$/);
    if (m) return '删除“' + m[1] + '”？此操作无法撤销。';

    m = String(text).match(/^Delete key\s+"([^"]+)"\?\s+This cannot be undone\.$/);
    if (m) return '删除密钥“' + m[1] + '”？此操作无法撤销。';

    if (String(text) === 'Cannot connect to daemon — is openfang running?') {
      return '无法连接到守护进程 — openfang 是否正在运行？';
    }
    if (String(text) === 'Cannot reach daemon — is openfang running?') {
      return '无法连接到守护进程 — openfang 是否正在运行？';
    }

    return text;
  }

  function translateTextForLocale(text, locale) {
    var s;
    var trimmed;
    var translated;
    var patterned;
    if (text === null || text === undefined) return text;
    s = String(text);
    trimmed = s.trim();
    if (!trimmed) return s;
    if (locale !== 'zh-CN') return s;
    translated = translateExactZh(trimmed);
    translated = translatePatternsZh(translated);
    if (translated !== trimmed) {
      return s.replace(trimmed, translated);
    }
    patterned = translatePatternsZh(trimmed);
    if (patterned !== trimmed) return s.replace(trimmed, patterned);
    return s;
  }

  function translateText(text) {
    return translateTextForLocale(text, _locale);
  }

  function hasOwn(obj, key) {
    return Object.prototype.hasOwnProperty.call(obj, key);
  }

  function hasAnyOwn(obj) {
    var k;
    if (!obj) return false;
    for (k in obj) {
      if (hasOwn(obj, k)) return true;
    }
    return false;
  }

  function shouldSkipNode(node) {
    if (!node) return true;
    var p = node.parentElement;
    if (!p) return false;
    if (p.closest && p.closest('[data-no-i18n]')) return true;
    var tag = (p.tagName || '').toUpperCase();
    if (tag === 'SCRIPT' || tag === 'STYLE') return true;
    if (tag === 'CODE' || tag === 'PRE' || tag === 'KBD' || tag === 'SAMP') return true;
    return false;
  }

  function translateAttributes(el) {
    var attrs;
    var i;
    var attr;
    var v;
    var tv;
    var store;
    var restored;
    if (!el || !el.getAttribute) return;
    attrs = ['title', 'placeholder', 'aria-label'];

    if (_locale === 'zh-CN') {
      for (i = 0; i < attrs.length; i++) {
        attr = attrs[i];
        v = el.getAttribute(attr);
        if (!v) continue;
        if (_attrOriginal) {
          store = _attrOriginal.get(el);
          if (!store) {
            store = {};
            _attrOriginal.set(el, store);
          }
          if (!hasOwn(store, attr)) {
            store[attr] = v;
          }
          tv = translateTextForLocale(store[attr], 'zh-CN');
        } else {
          tv = translateTextForLocale(v, 'zh-CN');
        }
        if (tv !== v) el.setAttribute(attr, tv);
      }
      return;
    }

    if (!_attrOriginal) return;
    store = _attrOriginal.get(el);
    if (!store) return;

    restored = false;
    for (i = 0; i < attrs.length; i++) {
      attr = attrs[i];
      if (!hasOwn(store, attr)) continue;
      if (store[attr] === null || store[attr] === undefined) {
        el.removeAttribute(attr);
      } else {
        el.setAttribute(attr, store[attr]);
      }
      delete store[attr];
      restored = true;
    }
    if (restored && !hasAnyOwn(store)) {
      _attrOriginal.delete(el);
    }
  }

  function translateTextNode(node) {
    var v;
    var entry;
    var source;
    var translated;
    if (!node) return;
    v = node.nodeValue;

    if (_locale === 'zh-CN') {
      if (!v || !v.trim()) return;
      source = v;
      if (_textOriginal) {
        entry = _textOriginal.get(node);
        if (entry && v === entry.translated) {
          source = entry.original;
        }
      }
      translated = translateTextForLocale(source, 'zh-CN');
      if (translated !== source) {
        if (_textOriginal) {
          _textOriginal.set(node, { original: source, translated: translated });
        }
        if (translated !== v) node.nodeValue = translated;
      } else if (_textOriginal) {
        _textOriginal.delete(node);
      }
      return;
    }

    if (!_textOriginal) return;
    entry = _textOriginal.get(node);
    if (!entry) return;
    if (v !== entry.original) node.nodeValue = entry.original;
    _textOriginal.delete(node);
  }

  function apply(root) {
    if (!root) return;

    if (root.nodeType === 1) translateAttributes(root);

    var n;
    var v;
    var tv;
    var walker = document.createTreeWalker(
      root,
      NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT,
      {
        acceptNode: function(node) {
          if (node.nodeType === 3) {
            if (shouldSkipNode(node)) return NodeFilter.FILTER_REJECT;
            if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
            return NodeFilter.FILTER_ACCEPT;
          }
          if (node.nodeType === 1) {
            return NodeFilter.FILTER_ACCEPT;
          }
          return NodeFilter.FILTER_REJECT;
        }
      }
    );

    n = walker.nextNode();
    while (n) {
      if (n.nodeType === 1) {
        translateAttributes(n);
      } else if (n.nodeType === 3) {
        translateTextNode(n);
      }
      n = walker.nextNode();
    }
  }

  var _observer = null;
  var _pending = false;
  function scheduleApply(target) {
    if (_pending) return;
    _pending = true;
    setTimeout(function() {
      _pending = false;
      try { apply(target || document.body); } catch (e) {}
    }, 0);
  }

  function installObserver() {
    if (_observer) return;
    if (typeof MutationObserver === 'undefined') return;
    _observer = new MutationObserver(function(mutations) {
      var i;
      var m;
      var j;
      var node;
      if (_locale === 'en') return;
      for (i = 0; i < mutations.length; i++) {
        m = mutations[i];
        if (m.type === 'childList') {
          for (j = 0; j < m.addedNodes.length; j++) {
            node = m.addedNodes[j];
            if (node && node.nodeType === 1) scheduleApply(node);
            if (node && node.nodeType === 3) scheduleApply(node.parentNode);
          }
        } else if (m.type === 'characterData') {
          scheduleApply(m.target && m.target.parentNode);
        }
      }
    });

    _observer.observe(document.body, { childList: true, subtree: true, characterData: true });
  }

  function init() {
    setLocale(_locale);
    try { apply(document.body); } catch (e) {}
    installObserver();
  }

  window.OpenFangI18n = {
    init: init,
    setLocale: function(loc) { setLocale(loc); scheduleApply(document.body); },
    getLocale: getLocale,
    intlLocale: function() { return _locale === 'zh-CN' ? 'zh-CN' : 'en-US'; },
    translateText: translateText,
    apply: apply,
    supported: function() { return SUPPORTED.slice(); }
  };

  try { init(); } catch (e) {}
})();
