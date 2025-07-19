// index.js — 完全版
// HTML ファイルの場所を基準に index.json を読み込みます

// WebKit compatibility: Polyfills
if (!Array.prototype.forEach) {
  Array.prototype.forEach = function(callback, thisArg) {
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
  window.Set = function() {
    this.data = [];
  };
  window.Set.prototype.add = function(value) {
    if (this.data.indexOf(value) === -1) {
      this.data.push(value);
    }
    return this;
  };
  window.Set.prototype.has = function(value) {
    return this.data.indexOf(value) !== -1;
  };
  window.Set.prototype.delete = function(value) {
    var index = this.data.indexOf(value);
    if (index !== -1) {
      this.data.splice(index, 1);
      return true;
    }
    return false;
  };
  window.Set.prototype.clear = function() {
    this.data = [];
  };
  Object.defineProperty(window.Set.prototype, 'size', {
    get: function() {
      return this.data.length;
    }
  });
}

// WebKit compatibility: Promise polyfill check
if (typeof Promise === 'undefined') {
  console.warn('Promise is not supported in this browser. Some features may not work.');
}

// WebKit compatibility: fetch polyfill check
if (typeof fetch === 'undefined') {
  console.warn('fetch is not supported in this browser. XMLHttpRequest will be used instead.');
}

var basePath = window.location.pathname.replace(/\/[^/]*$/, '/');

/* --------------------------------------------------
   グローバル変数・初期設定
-------------------------------------------------- */
var columns = [
  'serialization',
  'title',
  'author',
  'type',
  'tags',
  'create_date',
  'update_date'
];
var tableData = {};
var currentPage = 1;
var rowsPerPage = 10;
var hiddenCols = [];
var filteredAuthors = [];
var hiddenAuthors = [];
var typeFilter = 'all';
var includedTags = [];
var excludedTags = [];
var includeOperator = 'AND';
var excludeOperator = 'AND';
var selectedRows = new Set();
var fixedWidthMapping = { serialization: 6, type: 3, create_date: 14, update_date: 14 };
var variableWeightMapping = { title: 50, author: 20, tags: 30 };
var sortInfo = { column: null, ascending: true };
var isIncludeTagsCollapsed = false;
var isExcludeTagsCollapsed = false;
var isHiddenAuthorsCollapsed = false;

/* --------------------------------------------------
   データ取得（ETagによるキャッシュ判定を導入）
-------------------------------------------------- */
async function fetchData() {
  var overlay = document.getElementById('loading-overlay');
  overlay.style.display = 'flex';

  // 以前に保存したETagを取得（初回はnullになります）
  var etagKey = 'indexJsonEtag_' + basePath;
  var savedEtag = localStorage.getItem(etagKey);

  try {
    var response;
    var newEtag = null;

    if (savedEtag) {
      // 1) サーバーにHEADリクエストを送り、最新のETagだけを取得する
      try {
        var headResp = await fetch(basePath + 'index.json', {
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
-------------------------------------------------- */
function loadSettings() {
  var s = JSON.parse(localStorage.getItem('tableSettings')) || {};
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
}

function saveSettings() {
  var s = {
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
    isHiddenAuthorsCollapsed: isHiddenAuthorsCollapsed
  };
  localStorage.setItem('tableSettings', JSON.stringify(s));
}

/* --------------------------------------------------
   UI 構築（初期描画）
-------------------------------------------------- */
function buildUI() {
  renderTagFilters();
  updateAuthorDropdownOptions();
  updateAuthorDropdownValue();
  renderHiddenAuthors();
  renderTableHeaders();
  renderTable();
  updatePagination();
  applySettingsToUI();
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
   作者関連
-------------------------------------------------- */
function updateAuthorDropdownOptions() {
  var dd = document.getElementById('author-filter-dropdown');
  if (!dd) return;
  while (dd.options.length > 1) dd.remove(1);
  var map = {};
  
  // WebKit compatibility: avoid Object.values
  var keys = Object.keys(tableData);
  for (var i = 0; i < keys.length; i++) {
    var it = tableData[keys[i]];
    var id = it.author_id || it.author;
    var time = new Date(it.update_date).getTime();
    if (!map[id] || time > map[id].time) map[id] = { name: it.author, time: time };
  }
  
  var sortedIds = Object.keys(map).sort(function(a, b) {
    return map[a].name.localeCompare(map[b].name);
  });
  
  for (var i = 0; i < sortedIds.length; i++) {
    var id = sortedIds[i];
    var o = document.createElement('option');
    o.value = id;
    o.textContent = map[id].name;
    dd.appendChild(o);
  }
}

function updateAuthorDropdownValue() {
  var dd = document.getElementById('author-filter-dropdown');
  if (dd) dd.value = filteredAuthors[0] || '';
}

function handleAuthorFiltering(author, authorId, ev) {
  ev.preventDefault();
  if (confirm('この作者で絞り込みますか？  キャンセルを押すと非表示リストに追加します。')) {
    filteredAuthors = [authorId];
  } else {
    if (!hiddenAuthors.includes(author)) hiddenAuthors.push(author);
  }
  currentPage = 1;
  saveSettings();
  renderHiddenAuthors();
  renderTable();
  updatePagination();
}

/* --------------------------------------------------
   非表示作者エリア
-------------------------------------------------- */
function renderHiddenAuthors() {
  const c = document.getElementById('hidden-author-container');
  if (!c) return;
  c.innerHTML = '';
  const head = document.createElement('div');
  head.style.cursor = 'pointer';
  head.style.fontWeight = 'bold';
  head.textContent = `非表示作者${isHiddenAuthorsCollapsed ? ' [+]' : ' [-]'}`;
  head.addEventListener('click', () => {
    isHiddenAuthorsCollapsed = !isHiddenAuthorsCollapsed;
    saveSettings();
    renderHiddenAuthors();
  });
  c.appendChild(head);
  if (isHiddenAuthorsCollapsed) return;
  hiddenAuthors.forEach(name => {
    const s = document.createElement('span');
    s.textContent = name;
    s.classList.add('hidden-author-tag');
    s.addEventListener('click', () => {
      hiddenAuthors = hiddenAuthors.filter(a => a !== name);
      saveSettings();
      renderHiddenAuthors();
      renderTable();
    });
    c.appendChild(s);
    c.appendChild(document.createTextNode(' '));
  });
}

/* --------------------------------------------------
   タグフィルター UI
-------------------------------------------------- */
function renderTagFilters() {
  buildTagSection(
    'include',
    includedTags,
    isIncludeTagsCollapsed,
    includeOperator,
    op => includeOperator = op,
    tags => includedTags = tags
  );
  buildTagSection(
    'exclude',
    excludedTags,
    isExcludeTagsCollapsed,
    excludeOperator,
    op => excludeOperator = op,
    tags => excludedTags = tags
  );
}

function getFilteredEntries() {
  return Object.entries(tableData).filter(([, it]) => {
    if (typeFilter !== 'all' && it.type !== typeFilter) return false;
    if (hiddenAuthors.includes(it.author)) return false;
    if (filteredAuthors.length && !filteredAuthors.includes(it.author_id || it.author)) return false;

    const tags = it.all_tags || [];

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

    // 両方の条件を満たした場合のみ表示
    return includeResult && excludeResult;
  });
}

function buildTagSection(kind, tagArr, collapsed, operator, setOp, setTags) {
  const root = document.getElementById(kind === 'include' ? 'include-tags' : 'exclude-tags');
  if (!root) return;
  root.innerHTML = '';
  const head = document.createElement('div');
  head.style.cursor = 'pointer';
  head.style.fontWeight = 'bold';
  head.textContent = `${kind === 'include' ? '含むタグ' : '含まないタグ'}${collapsed ? ' [+]' : ' [-]'}`;
  head.addEventListener('click', () => {
    if (kind === 'include') isIncludeTagsCollapsed = !isIncludeTagsCollapsed;
    if (kind === 'exclude') isExcludeTagsCollapsed = !isExcludeTagsCollapsed;
    saveSettings();
    renderTagFilters();
  });
  root.appendChild(head);
  if (collapsed) return;

  const sel = document.createElement('select');
  ['AND', 'OR'].forEach(op => {
    const o = document.createElement('option');
    o.value = o.textContent = op;
    sel.appendChild(o);
  });
  sel.value = operator;
  sel.addEventListener('change', () => {
    setOp(sel.value);
    saveSettings();
    renderTable();
    updatePagination();
  });
  root.appendChild(document.createTextNode(' 条件: '));
  root.appendChild(sel);
  root.appendChild(document.createElement('br'));

  tagArr.forEach(tag => {
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = true;
    cb.addEventListener('change', () => {
      setTags(tagArr.filter(t => t !== tag));
      saveSettings();
      renderTagFilters();
      renderTable();
      updatePagination();
    });
    root.appendChild(cb);
    root.appendChild(document.createTextNode(' ' + tag));
    root.appendChild(document.createElement('br'));
  });
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
  if (confirm(`「${tag}」を含むフィルターに追加しますか？`)) {
    if (!includedTags.includes(tag)) includedTags.push(tag);
    excludedTags = excludedTags.filter(t => t !== tag);
  } else {
    if (!excludedTags.includes(tag)) excludedTags.push(tag);
    includedTags = includedTags.filter(t => t !== tag);
  }
  currentPage = 1;
  saveSettings();
  renderTagFilters();
  renderTable();
  updatePagination();
}

function updateAuthorFilter(id) {
  filteredAuthors = id ? [id] : [];
  currentPage = 1;
  saveSettings();
  renderTable();
  updatePagination();
}

function renderTable() {
  var tbody = document.getElementById('user-table-body');
  tbody.innerHTML = '';

  // 1) フィルター適用
  var entries = getFilteredEntries();

  // 2) ソート
  if (sortInfo.column) {
    entries.sort(function(a, b) {
      var entryA = a[1];
      var entryB = b[1];
      var A = entryA[sortInfo.column] || '';
      var B = entryB[sortInfo.column] || '';
      if (!isNaN(A) && !isNaN(B)) {
        A = parseFloat(A);
        B = parseFloat(B);
      }
      return (A < B ? -1 : A > B ? 1 : 0) * (sortInfo.ascending ? 1 : -1);
    });
  }

  // 3) 総ページ数計算・currentPage を clamp
  var totalItems = entries.length;
  var totalPages = rowsPerPage
    ? Math.ceil(totalItems / rowsPerPage)
    : 1;
  currentPage = Math.min(Math.max(1, currentPage), totalPages);

  // 4) ページ情報表示
  document.getElementById('page-info').textContent = currentPage + ' / ' + totalPages;

  // 5) ページネーション（スライス）
  var start = (currentPage - 1) * rowsPerPage;
  var pageEntries = rowsPerPage
    ? entries.slice(start, start + rowsPerPage)
    : entries;

  // 6) 行レンダリング
  for (var i = 0; i < pageEntries.length; i++) {
    var key = pageEntries[i][0];
    var it = pageEntries[i][1];
    var tr = document.createElement('tr');

    // チェックボックス
    var tdChk = document.createElement('td');
    tdChk.style.width = '3ch';
    var cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.classList.add('row-checkbox');
    cb.dataset.key = key;
    cb.checked = selectedRows.has(key);
    cb.addEventListener('change', function () {
      this.checked ? selectedRows.add(key) : selectedRows.delete(key);
      updateSelectedCount();
    });
    tdChk.appendChild(cb);
    tr.appendChild(tdChk);

    // セル描画のための幅計算
    var fixedTotal = fixedWidthMapping.serialization
      + fixedWidthMapping.type
      + fixedWidthMapping.create_date
      + fixedWidthMapping.update_date;
    var varTotal = variableWeightMapping.title
      + variableWeightMapping.author
      + variableWeightMapping.tags;

    for (var j = 0; j < columns.length; j++) {
      var c = columns[j];
      var td = document.createElement('td');
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
            var a = document.createElement('a');
            a.href = './' + key + '/';
            a.textContent = it.title;
            td.appendChild(a);
            break;
          }
          case 'author': {
            var a = document.createElement('a');
            a.href = it.author_url;
            a.target = '_blank';
            a.textContent = it.author;
            a.addEventListener('click', function(e) {
              if (e.ctrlKey) handleAuthorFiltering(it.author, it.author_id || it.author, e);
            });
            td.appendChild(a);
            break;
          }
          case 'type':
            td.textContent = it.type === 'novel' ? '小説' : '漫画';
            break;
          case 'tags':
            if (Array.isArray(it.all_tags)) {
              for (var k = 0; k < it.all_tags.length; k++) {
                var t = it.all_tags[k];
                var s = document.createElement('span');
                s.textContent = t;
                s.classList.add('tag-item');
                s.style.cursor = 'pointer';
                // Use IIFE to capture the tag value properly
                s.addEventListener('click', (function(tagValue) {
                  return function() {
                    tagFilterClick(tagValue);
                  };
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
  console.log(
    '≪DEBUG≫ includeOp=', includeOperator,
    'includedTags=', includedTags,
    'filteredCount=', totalItems,
    'rowsPerPage=', rowsPerPage
  );
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
  var overlay = document.createElement('div');
  overlay.style.cssText = 'position:fixed;top:0;left:0;width:100%;height:100%;background:rgba(0,0,0,0.5);display:flex;align-items:center;justify-content:center;';
  var box = document.createElement('div');
  box.style.cssText = 'background:#fff;padding:1em;border-radius:5px;max-width:80%;max-height:80%;overflow:auto;';
  
  // WebKit compatibility: avoid template literals
  var content = '<strong>リンク先をコピーしました</strong><br><br>' + titles.join('<br>') + '<br><br><button id="close-copy-popup">閉じる</button>';
  box.innerHTML = content;
  
  overlay.appendChild(box);
  document.body.appendChild(overlay);
  document.getElementById('close-copy-popup').addEventListener('click', function() {
    document.body.removeChild(overlay);
  });
}

function copySelected() {
  var links = [];
  var titles = [];
  var checkboxes = document.querySelectorAll('.row-checkbox');
  
  // WebKit compatibility: use for loop instead of forEach
  for (var i = 0; i < checkboxes.length; i++) {
    var cb = checkboxes[i];
    if (cb.checked) {
      var row = cb.closest('tr');
      var a = row.querySelector('.td-title a');
      if (a) { 
        links.push(a.href); 
        titles.push(a.textContent); 
      }
    }
  }
  
  // WebKit compatibility: check if clipboard API is available
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(links.join('\n'))
      .then(function() { showCopyPopup(titles); })
      .catch(function(err) { alert('コピーに失敗しました: ' + err); });
  } else {
    // Fallback for older browsers
    var textarea = document.createElement('textarea');
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
    if (confirm('ローカルストレージをリセットしますか？')) {
      // このページで使っている設定 only
      localStorage.removeItem('tableSettings');
      location.reload();
    }
  });

  document.getElementById('reset-hidden-authors-button').addEventListener('click', () => {
    if (confirm('非表示作者をリセットしますか？')) {
      hiddenAuthors = [];
      saveSettings();
      renderHiddenAuthors();
      renderTable();
    }
  });

  document.getElementById('reset-include-tags-button').addEventListener('click', () => {
    if (confirm('含むタグをリセットしますか？')) {
      includedTags = [];
      saveSettings();
      renderTagFilters();
      renderTable();
    }
  });

  document.getElementById('reset-exclude-tags-button').addEventListener('click', () => {
    if (confirm('含まないタグをリセットしますか？')) {
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
});

/* --------------------------------------------------
   キーボードショートカット
-------------------------------------------------- */
document.addEventListener('keydown', function(event) {
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
      var keys = Object.keys(tableData);
      for (var i = 0; i < keys.length; i++) {
        selectedRows.add(keys[i]);
      }
    } else if (event.ctrlKey && !event.shiftKey) {
      // 表示中の行だけ選択
      var checkboxes = document.querySelectorAll('#user-table-body .row-checkbox');
      for (var i = 0; i < checkboxes.length; i++) {
        var cb = checkboxes[i];
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
