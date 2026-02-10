// index.js — 完全版
// HTML ファイルの場所を基準に index.json を読み込みます

// WebKit compatibility: Polyfills
if (!Array.prototype.forEach) {
  Array.prototype.forEach = function (callback, thisArg) {
    var T, k;
    if (this == null) {
      throw new TypeError('this is null or not defined');
    }
    var O = Object(this);
    var len = parseInt(O.length) || 0;
    if (typeof callback !== "function") {
      throw new TypeError(callback + ' is not a function');
    }
    if (arguments.length > 1) {
      T = thisArg;
    }
    k = 0;
    while (k < len) {
      var kValue;
      if (k in O) {
        kValue = O[k];
        callback.call(T, kValue, k, O);
      }
      k++;
    }
  };
}

// WebKit compatibility: Set polyfill
if (typeof Set === 'undefined') {
  window.Set = function () {
    this.data = [];
  };
  window.Set.prototype.add = function (value) {
    if (this.data.indexOf(value) === -1) {
      this.data.push(value);
    }
    return this;
  };
  window.Set.prototype.has = function (value) {
    return this.data.indexOf(value) !== -1;
  };
  window.Set.prototype.delete = function (value) {
    var index = this.data.indexOf(value);
    if (index !== -1) {
      this.data.splice(index, 1);
      return true;
    }
    return false;
  };
  window.Set.prototype.clear = function () {
    this.data = [];
  };
  Object.defineProperty(window.Set.prototype, 'size', {
    get: function () {
      return this.data.length;
    }
  });
}

const basePath = window.location.pathname.replace(/\/[^/]*$/, '/');

/* --------------------------------------------------
   グローバル変数・初期設定
-------------------------------------------------- */
const columns = [
  'serialization',
  'title',
  'author',
  'type',
  'tags',
  'create_date',
  'update_date'
];
let tableData = {};
let currentPage = 1;
let rowsPerPage = 10;
let hiddenCols = [];
let filteredAuthors = [];
let hiddenAuthors = [];
let typeFilter = 'all';
// サイト別フィルター
let includedTags = [];
let excludedTags = [];
let includeOperator = 'AND';
let excludeOperator = 'AND';
// グローバルフィルター（全サイト共通）
let globalIncludedTags = [];
let globalExcludedTags = [];
let globalIncludeOperator = 'AND';
let globalExcludeOperator = 'AND';
let globalHiddenAuthors = [];
// フィルターモード: 'both'（両方）, 'global'（グローバルのみ）, 'site'（サイト別のみ）
let filterMode = 'both';
const selectedRows = new Set();
const fixedWidthMapping = { serialization: 10, type: 8, create_date: 14, update_date: 14 };
const variableWeightMapping = { title: 50, author: 20, tags: 30 };
let sortInfo = { column: null, ascending: true };
let isIncludeTagsCollapsed = false;
let isExcludeTagsCollapsed = false;
let isHiddenAuthorsCollapsed = false;
let isColumnSelectorCollapsed = false;
let isGlobalIncludeTagsCollapsed = false;
let isGlobalExcludeTagsCollapsed = false;
let isGlobalHiddenAuthorsCollapsed = false;
let isGlobalSectionCollapsed = false;
let isSiteSectionCollapsed = false;

/* --------------------------------------------------
   データ取得（ETagによるキャッシュ判定を導入）
-------------------------------------------------- */
async function fetchData() {
  const overlay = document.getElementById('loading-overlay');
  overlay.style.display = 'flex';

  // 以前に保存したETagを取得（初回はnullになります）
  const etagKey = 'indexJsonEtag_' + basePath;
  const savedEtag = localStorage.getItem(etagKey);

  try {
    let response;
    let newEtag = null;

    if (savedEtag) {
      // 1) サーバーにHEADリクエストを送り、最新のETagだけを取得する
      try {
        const headResp = await fetch(basePath + 'index.json', {
          method: 'HEAD'
        });
        if (!headResp.ok) {
          throw new Error(`HEADリクエストが失敗しました (${headResp.status})`);
        }
        newEtag = headResp.headers.get('ETag');
      } catch (headErr) {
        // HEADが失敗した場合は通常のGETにフォールバック
        console.warn('HEADリクエスト失敗、GETで再取得します', headErr);
        response = await fetch(basePath + 'index.json');
      }

      if (newEtag && newEtag === savedEtag) {
        // 2) ETagが同じ → ブラウザのHTTPキャッシュ（Cache Storage）からのみ読み込む
        try {
          response = await fetch(basePath + 'index.json', {
            method: 'GET',
            cache: 'only-if-cached',
            mode: 'same-origin'
          });
          if (!response || !response.ok) {
            // キャッシュにない場合は通常のGETにフォールバック
            throw new Error('only-if-cachedでの取得に失敗、GETで再取得します');
          }
        } catch (cacheErr) {
          console.warn('キャッシュからの読み込み失敗、GETで再取得します', cacheErr);
          response = await fetch(basePath + 'index.json');
        }
      } else {
        // 3) ETagが異なる（またはHEADでETag取得に失敗）→ GETで再取得し、新しいETagを保存
        response = await fetch(basePath + 'index.json');
        newEtag = response.headers.get('ETag');
      }
    } else {
      // savedEtagがない（初回ロード）→ 通常のGETで取得
      response = await fetch(basePath + 'index.json');
      newEtag = response.headers.get('ETag');
    }

    if (!response.ok) {
      throw new Error(`${response.status} ${response.statusText}`);
    }

    // JSONデータを取得
    tableData = await response.json();

    // 新しいETagが取得できていたらlocalStorageに保存
    if (newEtag) {
      localStorage.setItem(etagKey, newEtag);
    }

    loadSettings();
    buildUI();
  } catch (err) {
    console.error('JSONデータの読み込みに失敗しました:', err);
  } finally {
    overlay.style.display = 'none';
  }
}

/* --------------------------------------------------
   ローカルストレージ（設定の保存・読込）
   - tableSettings: サイト別設定（従来互換）
   - globalFilterSettings: 全サイト共通フィルター
-------------------------------------------------- */
function loadSettings() {
  // サイト別設定の読み込み
  const s = JSON.parse(localStorage.getItem('tableSettings')) || {};
  rowsPerPage = typeof s.rowsPerPage === 'number' ? s.rowsPerPage : 10;
  hiddenCols = s.hiddenCols || [];
  currentPage = s.currentPage || 1;
  typeFilter = s.typeFilter || 'all';
  filteredAuthors = s.filteredAuthors || [];
  hiddenAuthors = s.hiddenAuthors || [];
  sortInfo = s.sortInfo || { column: null, ascending: true };
  includedTags = s.includedTags || [];
  excludedTags = s.excludedTags || [];
  includeOperator = s.includeOperator || 'AND';
  excludeOperator = s.excludeOperator || 'AND';
  isIncludeTagsCollapsed = s.isIncludeTagsCollapsed || false;
  isExcludeTagsCollapsed = s.isExcludeTagsCollapsed || false;
  isHiddenAuthorsCollapsed = s.isHiddenAuthorsCollapsed || false;
  isColumnSelectorCollapsed = s.isColumnSelectorCollapsed || false;
  filterMode = s.filterMode || 'both';
  isGlobalSectionCollapsed = s.isGlobalSectionCollapsed || false;
  isSiteSectionCollapsed = s.isSiteSectionCollapsed || false;

  // グローバル設定の読み込み
  const g = JSON.parse(localStorage.getItem('globalFilterSettings')) || {};
  globalIncludedTags = g.includedTags || [];
  globalExcludedTags = g.excludedTags || [];
  globalIncludeOperator = g.includeOperator || 'AND';
  globalExcludeOperator = g.excludeOperator || 'AND';
  globalHiddenAuthors = g.hiddenAuthors || [];
  isGlobalIncludeTagsCollapsed = g.isIncludeTagsCollapsed || false;
  isGlobalExcludeTagsCollapsed = g.isExcludeTagsCollapsed || false;
  isGlobalHiddenAuthorsCollapsed = g.isHiddenAuthorsCollapsed || false;
}

function saveSettings() {
  // サイト別設定の保存
  const s = {
    rowsPerPage: rowsPerPage,
    hiddenCols: hiddenCols,
    currentPage: currentPage,
    typeFilter: typeFilter,
    filteredAuthors: filteredAuthors,
    hiddenAuthors: hiddenAuthors,
    sortInfo: sortInfo,
    includedTags: includedTags,
    excludedTags: excludedTags,
    includeOperator: includeOperator,
    excludeOperator: excludeOperator,
    isIncludeTagsCollapsed: isIncludeTagsCollapsed,
    isExcludeTagsCollapsed: isExcludeTagsCollapsed,
    isHiddenAuthorsCollapsed: isHiddenAuthorsCollapsed,
    isColumnSelectorCollapsed: isColumnSelectorCollapsed,
    filterMode: filterMode,
    isGlobalSectionCollapsed: isGlobalSectionCollapsed,
    isSiteSectionCollapsed: isSiteSectionCollapsed
  };
  localStorage.setItem('tableSettings', JSON.stringify(s));
}

function saveGlobalSettings() {
  const g = {
    includedTags: globalIncludedTags,
    excludedTags: globalExcludedTags,
    includeOperator: globalIncludeOperator,
    excludeOperator: globalExcludeOperator,
    hiddenAuthors: globalHiddenAuthors,
    isIncludeTagsCollapsed: isGlobalIncludeTagsCollapsed,
    isExcludeTagsCollapsed: isGlobalExcludeTagsCollapsed,
    isHiddenAuthorsCollapsed: isGlobalHiddenAuthorsCollapsed
  };
  localStorage.setItem('globalFilterSettings', JSON.stringify(g));
}

/* --------------------------------------------------
   UI 構築（初期描画）
-------------------------------------------------- */
/**
 * 統一折りたたみヘッダーを生成する
 * @param {string} label - 見出しテキスト
 * @param {boolean} collapsed - 現在折りたたみ中か
 * @param {Function} onToggle - クリック時のコールバック
 * @param {Object} [options] - 追加オプション
 * @param {boolean} [options.isGlobal] - グローバル変種（紫色左ボーダー）
 * @returns {HTMLElement} ヘッダー要素
 */
function createCollapsibleHeader(label, collapsed, onToggle, options) {
  options = options || {};
  var head = document.createElement('div');
  head.className = 'collapsible-header' + (collapsed ? ' collapsed' : '') + (options.isGlobal ? ' global-variant' : '');

  var labelSpan = document.createElement('span');
  labelSpan.className = 'collapsible-header-label';
  labelSpan.textContent = label;
  head.appendChild(labelSpan);

  var icon = document.createElement('span');
  icon.className = 'collapsible-header-icon';
  icon.textContent = '▼';
  head.appendChild(icon);

  head.addEventListener('click', onToggle);
  return head;
}

function buildUI() {
  renderFilterModeSelector();
  renderGlobalFilters();
  renderTagFilters();
  updateAuthorDropdownOptions();
  updateAuthorDropdownValue();
  renderHiddenAuthors();
  renderColumnSelector();
  renderTableHeaders();
  renderTable();
  updatePagination();
  applySettingsToUI();
  updateFilterPanelVisibility();
  applySectionCollapseState();
}

function applySettingsToUI() {
  document.getElementById('rowsPerPageSelect').value = rowsPerPage;
  document.getElementById('typeSelect').value = typeFilter;
  columns.forEach(col => {
    const cb = document.getElementById('show-' + col);
    if (cb) cb.checked = !hiddenCols.includes(col);
  });
}

/* --------------------------------------------------
   表示項目の選択（折りたたみ対応）
-------------------------------------------------- */
function renderColumnSelector() {
  var container = document.getElementById('column-selector-container');
  if (!container) return;
  container.innerHTML = '';

  var head = createCollapsibleHeader(
    '表示項目の選択',
    isColumnSelectorCollapsed,
    function () {
      isColumnSelectorCollapsed = !isColumnSelectorCollapsed;
      saveSettings();
      renderColumnSelector();
    }
  );
  container.appendChild(head);

  if (isColumnSelectorCollapsed) return;

  var body = document.createElement('div');
  body.className = 'collapsible-body column-selector';

  var columnDefs = [
    { key: 'serialization', label: '連載状況' },
    { key: 'title', label: 'タイトル' },
    { key: 'author', label: '作者名' },
    { key: 'type', label: '形式' },
    { key: 'tags', label: 'タグ' },
    { key: 'create_date', label: '掲載日時' },
    { key: 'update_date', label: '更新日時' }
  ];

  columnDefs.forEach(function (def) {
    var label = document.createElement('label');
    var cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.id = 'show-' + def.key;
    cb.checked = !hiddenCols.includes(def.key);
    cb.addEventListener('click', (function (key) {
      return function () { toggleColumn(key); };
    })(def.key));
    label.appendChild(cb);
    label.appendChild(document.createTextNode(' ' + def.label));
    body.appendChild(label);
  });

  container.appendChild(body);
}

/* --------------------------------------------------
   作者関連
-------------------------------------------------- */
function updateAuthorDropdownOptions() {
  const dd = document.getElementById('author-filter-dropdown');
  if (!dd) return;
  while (dd.options.length > 1) dd.remove(1);
  const map = {};

  // WebKit compatibility: avoid Object.values
  const keys = Object.keys(tableData);
  for (let i = 0; i < keys.length; i++) {
    const it = tableData[keys[i]];
    const id = it.author_id || it.author;
    const time = new Date(it.update_date).getTime();
    if (!map[id] || time > map[id].time) map[id] = { name: it.author, time: time };
  }

  const sortedIds = Object.keys(map).sort((a, b) =>
    map[a].name.localeCompare(map[b].name)
  );

  for (let i = 0; i < sortedIds.length; i++) {
    const id = sortedIds[i];
    const o = document.createElement('option');
    o.value = id;
    o.textContent = map[id].name;
    dd.appendChild(o);
  }
}

function updateAuthorDropdownValue() {
  const dd = document.getElementById('author-filter-dropdown');
  if (dd) dd.value = filteredAuthors[0] || '';
}

function handleAuthorFiltering(author, authorId, ev) {
  ev.preventDefault();
  const action = prompt(
    'この作者の処理を選択してください:\n' +
    '1 = この作者で絞り込み\n' +
    '2 = サイト別の非表示リストに追加\n' +
    '3 = 共通の非表示リストに追加（全サイト適用）\n' +
    'それ以外 = キャンセル',
    '1'
  );
  if (action === '1') {
    filteredAuthors = [authorId];
    saveSettings();
  } else if (action === '2') {
    if (!hiddenAuthors.includes(author)) hiddenAuthors.push(author);
    saveSettings();
    renderHiddenAuthors();
  } else if (action === '3') {
    if (!globalHiddenAuthors.includes(author)) globalHiddenAuthors.push(author);
    saveGlobalSettings();
    renderGlobalHiddenAuthors();
  } else {
    return;
  }
  currentPage = 1;
  renderTable();
  updatePagination();
}

/* --------------------------------------------------
   非表示作者エリア（サイト別）
-------------------------------------------------- */
function renderHiddenAuthors() {
  var c = document.getElementById('hidden-author-container');
  if (!c) return;
  c.innerHTML = '';

  var head = createCollapsibleHeader(
    '非表示作者（サイト別）',
    isHiddenAuthorsCollapsed,
    function () {
      isHiddenAuthorsCollapsed = !isHiddenAuthorsCollapsed;
      saveSettings();
      renderHiddenAuthors();
    }
  );
  c.appendChild(head);

  if (isHiddenAuthorsCollapsed) return;

  var body = document.createElement('div');
  body.className = 'collapsible-body';
  hiddenAuthors.forEach(function (name) {
    var s = document.createElement('span');
    s.textContent = name;
    s.classList.add('hidden-author-tag');
    s.addEventListener('click', function () {
      hiddenAuthors = hiddenAuthors.filter(function (a) { return a !== name; });
      saveSettings();
      renderHiddenAuthors();
      renderTable();
    });
    body.appendChild(s);
    body.appendChild(document.createTextNode(' '));
  });
  c.appendChild(body);
}

/* --------------------------------------------------
   タグフィルター UI（サイト別）
-------------------------------------------------- */
function renderTagFilters() {
  buildTagSection(
    'include',
    includedTags,
    isIncludeTagsCollapsed,
    includeOperator,
    op => includeOperator = op,
    tags => includedTags = tags,
    false
  );
  buildTagSection(
    'exclude',
    excludedTags,
    isExcludeTagsCollapsed,
    excludeOperator,
    op => excludeOperator = op,
    tags => excludedTags = tags,
    false
  );
}

function getFilteredEntries() {
  const useGlobal = filterMode === 'both' || filterMode === 'global';
  const useSite = filterMode === 'both' || filterMode === 'site';

  return Object.entries(tableData).filter(([, it]) => {
    if (typeFilter !== 'all' && it.type !== typeFilter) return false;

    // 作者フィルター（サイト別）
    if (useSite) {
      if (hiddenAuthors.includes(it.author)) return false;
    }
    // 作者フィルター（グローバル）— グローバル非表示作者は常に適用
    if (useGlobal) {
      if (globalHiddenAuthors.includes(it.author)) return false;
    }
    if (filteredAuthors.length && !filteredAuthors.includes(it.author_id || it.author)) return false;

    const tags = it.all_tags || [];

    // --- グローバルフィルター ---
    if (useGlobal) {
      // グローバル含むタグ判定
      if (globalIncludedTags.length) {
        if (globalIncludeOperator === 'AND') {
          if (!globalIncludedTags.every(tag => tags.includes(tag))) return false;
        } else {
          if (!globalIncludedTags.some(tag => tags.includes(tag))) return false;
        }
      }
      // グローバル含まないタグ判定
      if (globalExcludedTags.length) {
        if (globalExcludeOperator === 'AND') {
          if (globalExcludedTags.every(tag => tags.includes(tag))) return false;
        } else {
          if (globalExcludedTags.some(tag => tags.includes(tag))) return false;
        }
      }
    }

    // --- サイト別フィルター ---
    if (useSite) {
      // 含むタグ判定（includeOperatorに応じて）
      let includeResult = true;
      if (includedTags.length) {
        if (includeOperator === 'AND') {
          includeResult = includedTags.every(tag => tags.includes(tag));
        } else {
          includeResult = includedTags.some(tag => tags.includes(tag));
        }
      }

      // 含まないタグ判定（excludeOperatorに応じて）
      let excludeResult = true;
      if (excludedTags.length) {
        if (excludeOperator === 'AND') {
          excludeResult = !excludedTags.every(tag => tags.includes(tag));
        } else {
          excludeResult = !excludedTags.some(tag => tags.includes(tag));
        }
      }

      if (!includeResult || !excludeResult) return false;
    }

    return true;
  });
}

function buildTagSection(kind, tagArr, collapsed, operator, setOp, setTags, isGlobal) {
  var prefix = isGlobal ? 'global-' : '';
  var root = document.getElementById(prefix + (kind === 'include' ? 'include-tags' : 'exclude-tags'));
  if (!root) return;
  root.innerHTML = '';
  var labelPrefix = isGlobal ? '【共通】' : '';
  var labelText = labelPrefix + (kind === 'include' ? '含むタグ' : '含まないタグ');

  var head = createCollapsibleHeader(
    labelText,
    collapsed,
    function () {
      if (isGlobal) {
        if (kind === 'include') isGlobalIncludeTagsCollapsed = !isGlobalIncludeTagsCollapsed;
        if (kind === 'exclude') isGlobalExcludeTagsCollapsed = !isGlobalExcludeTagsCollapsed;
        saveGlobalSettings();
      } else {
        if (kind === 'include') isIncludeTagsCollapsed = !isIncludeTagsCollapsed;
        if (kind === 'exclude') isExcludeTagsCollapsed = !isExcludeTagsCollapsed;
        saveSettings();
      }
      if (isGlobal) renderGlobalFilters();
      else renderTagFilters();
    },
    { isGlobal: isGlobal }
  );
  root.appendChild(head);

  if (collapsed) return;

  var body = document.createElement('div');
  body.className = 'collapsible-body';

  var sel = document.createElement('select');
  sel.className = 'tag-filter-op-select';
  ['AND', 'OR'].forEach(function (op) {
    var o = document.createElement('option');
    o.value = o.textContent = op;
    sel.appendChild(o);
  });
  sel.value = operator;
  sel.addEventListener('change', function () {
    setOp(sel.value);
    if (isGlobal) saveGlobalSettings();
    else saveSettings();
    renderTable();
    updatePagination();
  });
  body.appendChild(document.createTextNode(' 条件: '));
  body.appendChild(sel);
  body.appendChild(document.createElement('br'));

  tagArr.forEach(function (tag) {
    var cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = true;
    cb.addEventListener('change', function () {
      setTags(tagArr.filter(function (t) { return t !== tag; }));
      if (isGlobal) {
        saveGlobalSettings();
        renderGlobalFilters();
      } else {
        saveSettings();
        renderTagFilters();
      }
      renderTable();
      updatePagination();
    });
    body.appendChild(cb);
    body.appendChild(document.createTextNode(' ' + tag));
    body.appendChild(document.createElement('br'));
  });

  root.appendChild(body);
}

/* --------------------------------------------------
   フィルターモード切り替えUI
-------------------------------------------------- */
function renderFilterModeSelector() {
  const container = document.getElementById('filter-mode-container');
  if (!container) return;
  container.innerHTML = '';

  const label = document.createElement('span');
  label.className = 'filter-label';
  label.textContent = 'フィルターモード';
  label.style.marginBottom = '0';
  container.appendChild(label);

  const sel = document.createElement('select');
  sel.className = 'filter-select';
  sel.id = 'filterModeSelect';
  [
    { value: 'both', text: '共通 + サイト別' },
    { value: 'global', text: '共通のみ' },
    { value: 'site', text: 'サイト別のみ' }
  ].forEach(opt => {
    const o = document.createElement('option');
    o.value = opt.value;
    o.textContent = opt.text;
    sel.appendChild(o);
  });
  sel.value = filterMode;
  sel.addEventListener('change', () => {
    filterMode = sel.value;
    currentPage = 1;
    saveSettings();
    updateFilterPanelVisibility();
    renderTable();
    updatePagination();
  });
  container.appendChild(sel);
}

/**
 * フィルターモードに応じて、グローバル/サイト別フィルターパネルの表示を切り替え
 */
function updateFilterPanelVisibility() {
  const globalPanel = document.getElementById('global-filter-panel');
  const sitePanel = document.getElementById('site-filter-panel');
  if (globalPanel) {
    globalPanel.style.display = (filterMode === 'both' || filterMode === 'global') ? 'block' : 'none';
  }
  if (sitePanel) {
    sitePanel.style.display = (filterMode === 'both' || filterMode === 'site') ? 'block' : 'none';
  }
}

/**
 * セクションヘッダーの折りたたみ状態をDOMに反映
 */
function applySectionCollapseState() {
  var globalToggle = document.getElementById('global-section-toggle');
  var globalBody = document.getElementById('global-section-body');
  if (globalToggle && globalBody) {
    if (isGlobalSectionCollapsed) {
      globalToggle.classList.add('collapsed');
      globalBody.style.display = 'none';
    } else {
      globalToggle.classList.remove('collapsed');
      globalBody.style.display = '';
    }
  }

  var siteToggle = document.getElementById('site-section-toggle');
  var siteBody = document.getElementById('site-section-body');
  if (siteToggle && siteBody) {
    if (isSiteSectionCollapsed) {
      siteToggle.classList.add('collapsed');
      siteBody.style.display = 'none';
    } else {
      siteToggle.classList.remove('collapsed');
      siteBody.style.display = '';
    }
  }
}

/* --------------------------------------------------
   グローバルフィルターUI
-------------------------------------------------- */
function renderGlobalFilters() {
  // グローバルタグフィルター
  buildTagSection(
    'include',
    globalIncludedTags,
    isGlobalIncludeTagsCollapsed,
    globalIncludeOperator,
    op => globalIncludeOperator = op,
    tags => globalIncludedTags = tags,
    true
  );
  buildTagSection(
    'exclude',
    globalExcludedTags,
    isGlobalExcludeTagsCollapsed,
    globalExcludeOperator,
    op => globalExcludeOperator = op,
    tags => globalExcludedTags = tags,
    true
  );

  // グローバル非表示作者
  renderGlobalHiddenAuthors();
}

function renderGlobalHiddenAuthors() {
  var c = document.getElementById('global-hidden-author-container');
  if (!c) return;
  c.innerHTML = '';

  var head = createCollapsibleHeader(
    '【共通】非表示作者',
    isGlobalHiddenAuthorsCollapsed,
    function () {
      isGlobalHiddenAuthorsCollapsed = !isGlobalHiddenAuthorsCollapsed;
      saveGlobalSettings();
      renderGlobalHiddenAuthors();
    },
    { isGlobal: true }
  );
  c.appendChild(head);

  if (isGlobalHiddenAuthorsCollapsed) return;

  var body = document.createElement('div');
  body.className = 'collapsible-body';
  globalHiddenAuthors.forEach(function (name) {
    var s = document.createElement('span');
    s.textContent = name;
    s.classList.add('hidden-author-tag', 'global-hidden-author-tag');
    s.addEventListener('click', function () {
      globalHiddenAuthors = globalHiddenAuthors.filter(function (a) { return a !== name; });
      saveGlobalSettings();
      renderGlobalHiddenAuthors();
      renderTable();
    });
    body.appendChild(s);
    body.appendChild(document.createTextNode(' '));
  });
  c.appendChild(body);
}

/* --------------------------------------------------
   テーブルヘッダー
-------------------------------------------------- */
function renderTableHeaders() {
  const thead = document.getElementById('table-head');
  thead.innerHTML = '';
  const tr = document.createElement('tr');
  const thSel = document.createElement('th');
  thSel.style.width = '3ch';
  tr.appendChild(thSel);

  const visible = columns.filter(c => !hiddenCols.includes(c));
  const fixedTotal = visible.reduce((sum, c) => sum + (fixedWidthMapping[c] || 0), 0);
  const varTotal = visible.filter(c => !fixedWidthMapping[c])
    .reduce((sum, c) => sum + (variableWeightMapping[c] || 0), 0);

  columns.forEach(c => {
    const th = document.createElement('th');
    th.textContent = columnLabel(c);
    th.addEventListener('click', () => sortByColumn(c));
    th.classList.add(`th-${c}`);
    if (hiddenCols.includes(c)) th.classList.add('hidden-column');
    else if (fixedWidthMapping[c]) th.style.width = fixedWidthMapping[c] + 'ch';
    else th.style.width = `calc((100% - ${fixedTotal}ch) * ${(variableWeightMapping[c] || 0) / varTotal})`;
    tr.appendChild(th);
  });
  thead.appendChild(tr);
}

function columnLabel(c) {
  switch (c) {
    case 'serialization': return '連載状況';
    case 'title': return 'タイトル';
    case 'author': return '作者名';
    case 'type': return '形式';
    case 'tags': return 'タグ';
    case 'create_date': return '掲載日時';
    case 'update_date': return '更新日時';
    default: return c;
  }
}

/* --------------------------------------------------
   ソート
-------------------------------------------------- */
function sortByColumn(c) {
  if (sortInfo.column === c) sortInfo.ascending = !sortInfo.ascending;
  else { sortInfo.column = c; sortInfo.ascending = true; }
  currentPage = 1;
  renderTable();
  updatePagination();
  saveSettings();
}

/* --------------------------------------------------
   テーブル本体
-------------------------------------------------- */
function formatDateTime(str) {
  const d = new Date(str);
  if (isNaN(d)) return str;
  const z = v => ('0' + v).slice(-2);
  return `${d.getFullYear()}/${z(d.getMonth() + 1)}/${z(d.getDate())} ${z(d.getHours())}:${z(d.getMinutes())}`;
}

function tagFilterClick(tag) {
  showTagFilterPopup(tag);
}

/**
 * タグクリック時のポップアップ: 共通/サイト別 × 含む/含まない を選択
 */
function showTagFilterPopup(tag) {
  // 既存のポップアップがあれば閉じる
  const existing = document.getElementById('tag-filter-popup-overlay');
  if (existing) existing.remove();

  const overlay = document.createElement('div');
  overlay.id = 'tag-filter-popup-overlay';
  overlay.className = 'tag-popup-overlay';

  const box = document.createElement('div');
  box.className = 'tag-popup-box';

  const title = document.createElement('div');
  title.className = 'tag-popup-title';
  title.textContent = `「${tag}」をフィルターに追加`;
  box.appendChild(title);

  const desc = document.createElement('div');
  desc.className = 'tag-popup-desc';
  desc.textContent = '追加先とフィルター種別を選択してください';
  box.appendChild(desc);

  const btnGroup = document.createElement('div');
  btnGroup.className = 'tag-popup-btn-group';

  // 共通 含む
  const globalIncBtn = document.createElement('button');
  globalIncBtn.className = 'btn btn-sm tag-popup-btn tag-popup-global-include';
  globalIncBtn.textContent = '共通: 含む';
  globalIncBtn.addEventListener('click', () => {
    if (!globalIncludedTags.includes(tag)) globalIncludedTags.push(tag);
    globalExcludedTags = globalExcludedTags.filter(t => t !== tag);
    saveGlobalSettings();
    renderGlobalFilters();
    currentPage = 1;
    renderTable();
    updatePagination();
    overlay.remove();
  });
  btnGroup.appendChild(globalIncBtn);

  // 共通 含まない
  const globalExcBtn = document.createElement('button');
  globalExcBtn.className = 'btn btn-sm tag-popup-btn tag-popup-global-exclude';
  globalExcBtn.textContent = '共通: 含まない';
  globalExcBtn.addEventListener('click', () => {
    if (!globalExcludedTags.includes(tag)) globalExcludedTags.push(tag);
    globalIncludedTags = globalIncludedTags.filter(t => t !== tag);
    saveGlobalSettings();
    renderGlobalFilters();
    currentPage = 1;
    renderTable();
    updatePagination();
    overlay.remove();
  });
  btnGroup.appendChild(globalExcBtn);

  // サイト別 含む
  const siteIncBtn = document.createElement('button');
  siteIncBtn.className = 'btn btn-sm tag-popup-btn tag-popup-site-include';
  siteIncBtn.textContent = 'サイト別: 含む';
  siteIncBtn.addEventListener('click', () => {
    if (!includedTags.includes(tag)) includedTags.push(tag);
    excludedTags = excludedTags.filter(t => t !== tag);
    saveSettings();
    renderTagFilters();
    currentPage = 1;
    renderTable();
    updatePagination();
    overlay.remove();
  });
  btnGroup.appendChild(siteIncBtn);

  // サイト別 含まない
  const siteExcBtn = document.createElement('button');
  siteExcBtn.className = 'btn btn-sm tag-popup-btn tag-popup-site-exclude';
  siteExcBtn.textContent = 'サイト別: 含まない';
  siteExcBtn.addEventListener('click', () => {
    if (!excludedTags.includes(tag)) excludedTags.push(tag);
    includedTags = includedTags.filter(t => t !== tag);
    saveSettings();
    renderTagFilters();
    currentPage = 1;
    renderTable();
    updatePagination();
    overlay.remove();
  });
  btnGroup.appendChild(siteExcBtn);

  box.appendChild(btnGroup);

  // キャンセルボタン
  const cancelBtn = document.createElement('button');
  cancelBtn.className = 'btn btn-sm btn-outline tag-popup-cancel';
  cancelBtn.textContent = 'キャンセル';
  cancelBtn.addEventListener('click', () => overlay.remove());
  box.appendChild(cancelBtn);

  overlay.appendChild(box);

  // オーバーレイクリックで閉じる
  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) overlay.remove();
  });

  document.body.appendChild(overlay);
}

function updateAuthorFilter(id) {
  filteredAuthors = id ? [id] : [];
  currentPage = 1;
  saveSettings();
  renderTable();
  updatePagination();
}

function renderTable() {
  const tbody = document.getElementById('user-table-body');
  tbody.innerHTML = '';

  // 1) フィルター適用
  const entries = getFilteredEntries();

  // 2) ソート
  if (sortInfo.column) {
    entries.sort((a, b) => {
      const entryA = a[1];
      const entryB = b[1];
      let A = entryA[sortInfo.column] || '';
      let B = entryB[sortInfo.column] || '';
      if (!isNaN(A) && !isNaN(B)) {
        A = parseFloat(A);
        B = parseFloat(B);
      }
      return (A < B ? -1 : A > B ? 1 : 0) * (sortInfo.ascending ? 1 : -1);
    });
  }

  // 3) 総ページ数計算・currentPage を clamp
  const totalItems = entries.length;
  const totalPages = rowsPerPage
    ? Math.ceil(totalItems / rowsPerPage)
    : 1;
  currentPage = Math.min(Math.max(1, currentPage), totalPages);

  // 4) ページ情報表示
  document.getElementById('page-info').textContent = currentPage + ' / ' + totalPages;

  // 5) ページネーション（スライス）
  const start = (currentPage - 1) * rowsPerPage;
  const pageEntries = rowsPerPage
    ? entries.slice(start, start + rowsPerPage)
    : entries;

  // 6) 行レンダリング
  for (let i = 0; i < pageEntries.length; i++) {
    const key = pageEntries[i][0];
    const it = pageEntries[i][1];
    const tr = document.createElement('tr');

    // チェックボックス
    const tdChk = document.createElement('td');
    tdChk.style.width = '3ch';
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.classList.add('row-checkbox');
    cb.dataset.key = key;
    cb.checked = selectedRows.has(key);
    cb.addEventListener('change', ((k) => {
      return function () {
        this.checked ? selectedRows.add(k) : selectedRows.delete(k);
        updateSelectedCount();
      };
    })(key));
    tdChk.appendChild(cb);
    tr.appendChild(tdChk);

    // セル描画のための幅計算
    const fixedTotal = fixedWidthMapping.serialization
      + fixedWidthMapping.type
      + fixedWidthMapping.create_date
      + fixedWidthMapping.update_date;
    const varTotal = variableWeightMapping.title
      + variableWeightMapping.author
      + variableWeightMapping.tags;

    for (let j = 0; j < columns.length; j++) {
      const c = columns[j];
      const td = document.createElement('td');
      if (hiddenCols.includes(c)) {
        td.classList.add('hidden-column');
      } else {
        td.classList.add('td-' + c);
        if (fixedWidthMapping[c]) {
          td.style.width = fixedWidthMapping[c] + 'ch';
        } else {
          td.style.width = 'calc((100% - ' + fixedTotal + 'ch) * ' + ((variableWeightMapping[c] || 0) / varTotal) + ')';
        }

        switch (c) {
          case 'serialization':
            td.textContent = it.serialization || '';
            break;
          case 'title': {
            const a = document.createElement('a');
            a.href = './' + key + '/';
            a.textContent = it.title;
            td.appendChild(a);
            break;
          }
          case 'author': {
            const authorName = it.author;
            const authorId = it.author_id || it.author;
            const a = document.createElement('a');
            a.href = it.author_url;
            a.target = '_blank';
            a.textContent = authorName;
            a.addEventListener('click', (e) => {
              if (e.ctrlKey) handleAuthorFiltering(authorName, authorId, e);
            });
            td.appendChild(a);
            break;
          }
          case 'type':
            td.textContent = it.type === 'novel' ? '小説' : '漫画';
            break;
          case 'tags':
            if (Array.isArray(it.all_tags)) {
              for (let k = 0; k < it.all_tags.length; k++) {
                const t = it.all_tags[k];
                const s = document.createElement('span');
                s.textContent = t;
                s.classList.add('tag-item');
                s.style.cursor = 'pointer';
                s.addEventListener('click', ((tagValue) => {
                  return () => tagFilterClick(tagValue);
                })(t));
                td.appendChild(s);
              }
            }
            break;
          case 'create_date':
          case 'update_date':
            td.textContent = formatDateTime(it[c] || '');
            break;
          default:
            td.textContent = it[c] || '';
        }
      }
      tr.appendChild(td);
    }

    tbody.appendChild(tr);
  }

  // 選択件数更新
  updateSelectedCount();
}

/* --------------------------------------------------
   ページネーション
-------------------------------------------------- */
function updatePagination() {
  const pageInfo = document.getElementById('page-info');
  const totalItems = getFilteredEntries().length;
  const totalPages = rowsPerPage
    ? Math.ceil(totalItems / rowsPerPage)
    : 1;
  currentPage = Math.min(Math.max(1, currentPage), totalPages);
  pageInfo.textContent = `${currentPage} / ${totalPages}`;
}


function nextPage() {
  currentPage++;
  renderTable();
  saveSettings();
}

function prevPage() {
  currentPage--;
  renderTable();
  saveSettings();
}

/* --------------------------------------------------
   各種 UI 操作
-------------------------------------------------- */
function updateRowsPerPage() {
  rowsPerPage = parseInt(document.getElementById('rowsPerPageSelect').value, 10);
  currentPage = 1;
  saveSettings();
  renderTable();
  updatePagination();
}

function toggleColumn(c) {
  const cb = document.getElementById('show-' + c);
  if (cb.checked) hiddenCols = hiddenCols.filter(x => x !== c);
  else if (!hiddenCols.includes(c)) hiddenCols.push(c);
  saveSettings();
  renderTableHeaders();
  renderTable();
  updatePagination();
}

function filterByType() {
  typeFilter = document.getElementById('typeSelect').value;
  currentPage = 1;
  saveSettings();
  renderTable();
  updatePagination();
}

function updateSelectedCount() {
  document.getElementById('selected-count').textContent = '選択された件数: ' + selectedRows.size;
}

function showCopyPopup(titles) {
  const overlay = document.createElement('div');
  overlay.className = 'copy-popup-overlay';

  const box = document.createElement('div');
  box.className = 'copy-popup-box';

  const content = '<strong>リンク先をコピーしました</strong><br><br>' +
    titles.join('<br>') +
    '<br><br><button id="close-copy-popup" class="copy-popup-close">閉じる</button>';
  box.innerHTML = content;

  overlay.appendChild(box);
  document.body.appendChild(overlay);

  document.getElementById('close-copy-popup').addEventListener('click', () => {
    document.body.removeChild(overlay);
  });
}

function copySelected() {
  const links = [];
  const titles = [];

  selectedRows.forEach((key) => {
    const item = tableData[key];
    if (item) {
      links.push(window.location.origin + basePath + key + '/');
      titles.push(item.title);
    }
  });

  // WebKit compatibility: check if clipboard API is available
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(links.join('\n'))
      .then(() => showCopyPopup(titles))
      .catch((err) => alert('コピーに失敗しました: ' + err));
  } else {
    // Fallback for older browsers
    const textarea = document.createElement('textarea');
    textarea.value = links.join('\n');
    document.body.appendChild(textarea);
    textarea.select();
    try {
      document.execCommand('copy');
      showCopyPopup(titles);
    } catch (err) {
      alert('コピーに失敗しました: ' + err);
    }
    document.body.removeChild(textarea);
  }
}

/* --------------------------------------------------
   初期イベント登録
-------------------------------------------------- */
document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('copy-selected-button').addEventListener('click', copySelected);

  document.getElementById('reset-localstorage-button').addEventListener('click', () => {
    if (confirm('ローカルストレージをリセットしますか？（サイト別設定のみ）')) {
      localStorage.removeItem('tableSettings');
      localStorage.removeItem('pageWidth');
      localStorage.removeItem('siteIndexWidth');
      location.reload();
    }
  });

  document.getElementById('reset-hidden-authors-button').addEventListener('click', () => {
    if (confirm('サイト別の非表示作者をリセットしますか？')) {
      hiddenAuthors = [];
      saveSettings();
      renderHiddenAuthors();
      renderTable();
    }
  });

  document.getElementById('reset-include-tags-button').addEventListener('click', () => {
    if (confirm('サイト別の含むタグをリセットしますか？')) {
      includedTags = [];
      saveSettings();
      renderTagFilters();
      renderTable();
    }
  });

  document.getElementById('reset-exclude-tags-button').addEventListener('click', () => {
    if (confirm('サイト別の含まないタグをリセットしますか？')) {
      excludedTags = [];
      saveSettings();
      renderTagFilters();
      renderTable();
    }
  });

  document.getElementById('reset-author-filter-button').addEventListener('click', () => {
    if (confirm('作者絞り込みをリセットしますか？')) {
      filteredAuthors = [];
      const dd = document.getElementById('author-filter-dropdown');
      if (dd) dd.value = '';
      saveSettings();
      renderTable();
    }
  });

  // グローバルフィルターリセットボタン
  var globalResetIncBtn = document.getElementById('reset-global-include-tags-button');
  if (globalResetIncBtn) {
    globalResetIncBtn.addEventListener('click', () => {
      if (confirm('共通の含むタグをリセットしますか？（全サイトに影響します）')) {
        globalIncludedTags = [];
        saveGlobalSettings();
        renderGlobalFilters();
        renderTable();
      }
    });
  }

  var globalResetExcBtn = document.getElementById('reset-global-exclude-tags-button');
  if (globalResetExcBtn) {
    globalResetExcBtn.addEventListener('click', () => {
      if (confirm('共通の含まないタグをリセットしますか？（全サイトに影響します）')) {
        globalExcludedTags = [];
        saveGlobalSettings();
        renderGlobalFilters();
        renderTable();
      }
    });
  }

  var globalResetAuthorsBtn = document.getElementById('reset-global-hidden-authors-button');
  if (globalResetAuthorsBtn) {
    globalResetAuthorsBtn.addEventListener('click', () => {
      if (confirm('共通の非表示作者をリセットしますか？（全サイトに影響します）')) {
        globalHiddenAuthors = [];
        saveGlobalSettings();
        renderGlobalHiddenAuthors();
        renderTable();
      }
    });
  }

  var globalResetAllBtn = document.getElementById('reset-global-all-button');
  if (globalResetAllBtn) {
    globalResetAllBtn.addEventListener('click', () => {
      if (confirm('共通フィルター設定をすべてリセットしますか？（全サイトに影響します）')) {
        localStorage.removeItem('globalFilterSettings');
        location.reload();
      }
    });
  }

  // セクションヘッダーの折りたたみ切り替え
  var globalSectionToggle = document.getElementById('global-section-toggle');
  if (globalSectionToggle) {
    globalSectionToggle.addEventListener('click', function () {
      isGlobalSectionCollapsed = !isGlobalSectionCollapsed;
      saveSettings();
      applySectionCollapseState();
    });
  }

  var siteSectionToggle = document.getElementById('site-section-toggle');
  if (siteSectionToggle) {
    siteSectionToggle.addEventListener('click', function () {
      isSiteSectionCollapsed = !isSiteSectionCollapsed;
      saveSettings();
      applySectionCollapseState();
    });
  }

  // 初期表示時のフィルターパネル表示切り替え
  updateFilterPanelVisibility();
});

/* --------------------------------------------------
   キーボードショートカット
-------------------------------------------------- */
document.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') {
    event.preventDefault();
    selectedRows.clear();
    renderTable();
    updateSelectedCount();
  }
  if ((event.ctrlKey || event.shiftKey) && event.key.toLowerCase() === 'a') {
    event.preventDefault();
    if (event.shiftKey && !event.ctrlKey) {
      // すべてのフィルタ後データを選択
      const keys = Object.keys(tableData);
      for (let i = 0; i < keys.length; i++) {
        selectedRows.add(keys[i]);
      }
    } else if (event.ctrlKey && !event.shiftKey) {
      // 表示中の行だけ選択
      const checkboxes = document.querySelectorAll('#user-table-body .row-checkbox');
      for (let i = 0; i < checkboxes.length; i++) {
        const cb = checkboxes[i];
        cb.checked = true;
        selectedRows.add(cb.dataset.key);
      }
    }
    updateSelectedCount();
    renderTable();
  }
});

/* --------------------------------------------------
   起動
-------------------------------------------------- */
fetchData().then(() => applySettingsToUI());
