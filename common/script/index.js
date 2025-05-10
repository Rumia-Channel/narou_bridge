// index.js — 完全版
// HTML ファイルの場所を基準に index.json を読み込みます
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
let includedTags = [];
let excludedTags = [];
let includeOperator = 'AND';
let excludeOperator = 'AND';
let selectedRows = new Set();
const fixedWidthMapping = { serialization: 6, type: 3, create_date: 14, update_date: 14 };
const variableWeightMapping = { title: 50, author: 20, tags: 30 };
let sortInfo = { column: null, ascending: true };
let isIncludeTagsCollapsed = false;
let isExcludeTagsCollapsed = false;
let isHiddenAuthorsCollapsed = false;

/* --------------------------------------------------
   データ取得
-------------------------------------------------- */
async function fetchData() {
  const overlay = document.getElementById('loading-overlay');
  overlay.style.display = 'flex';
  try {
    const response = await fetch(basePath + 'index.json');
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    tableData = await response.json();
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
}

function saveSettings() {
  const s = {
    rowsPerPage,
    hiddenCols,
    currentPage,
    typeFilter,
    filteredAuthors,
    hiddenAuthors,
    sortInfo,
    includedTags,
    excludedTags,
    includeOperator,
    excludeOperator,
    isIncludeTagsCollapsed,
    isExcludeTagsCollapsed,
    isHiddenAuthorsCollapsed
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
  const dd = document.getElementById('author-filter-dropdown');
  if (!dd) return;
  while (dd.options.length > 1) dd.remove(1);
  const map = {};
  Object.values(tableData).forEach(it => {
    const id = it.author_id || it.author;
    const time = new Date(it.update_date).getTime();
    if (!map[id] || time > map[id].time) map[id] = { name: it.author, time };
  });
  Object.keys(map)
    .sort((a, b) => map[a].name.localeCompare(map[b].name))
    .forEach(id => {
      const o = document.createElement('option');
      o.value = id; o.textContent = map[id].name; dd.appendChild(o);
    });
}

function updateAuthorDropdownValue() {
  const dd = document.getElementById('author-filter-dropdown');
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

  // 条件セレクタ
  const sel = document.createElement('select');
  ['AND', 'OR'].forEach(op => {
    const o = document.createElement('option');
    o.value = o.textContent = op;
    sel.appendChild(o);
  });
  sel.value = operator;
  sel.addEventListener('change', () => { setOp(sel.value); saveSettings(); renderTable(); });
  root.appendChild(document.createTextNode(' 条件: '));
  root.appendChild(sel);
  root.appendChild(document.createElement('br'));

  // チェックボックス
  tagArr.forEach(tag => {
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = true;
    cb.addEventListener('change', () => {
      setTags(tagArr.filter(t => t !== tag));
      saveSettings();
      renderTagFilters();
      renderTable();
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
  const tbody = document.getElementById('user-table-body');
  tbody.innerHTML = '';

  let entries = Object.entries(tableData)
    .filter(([, it]) => typeFilter === 'all' || it.type === typeFilter)
    .filter(([, it]) => !hiddenAuthors.includes(it.author) &&
      (filteredAuthors.length === 0 || filteredAuthors.includes(it.author_id || it.author)))
    .filter(([, it]) => {
      if (!it.all_tags) return true;
      const incMatch = includedTags.length === 0 ||
        (includeOperator === 'AND'
          ? includedTags.every(t => it.all_tags.includes(t))
          : includedTags.some(t => it.all_tags.includes(t)));
      const excMatch = excludedTags.length === 0 ||
        (excludeOperator === 'OR'
          ? !excludedTags.some(t => it.all_tags.includes(t))
          : !excludedTags.every(t => it.all_tags.includes(t)));
      return incMatch && excMatch;
    });

  if (sortInfo.column) {
    entries.sort(([, a], [, b]) => {
      let A = a[sortInfo.column] ?? '';
      let B = b[sortInfo.column] ?? '';
      if (!isNaN(A) && !isNaN(B)) { A = parseFloat(A); B = parseFloat(B); }
      return A < B ? -1 : A > B ? 1 : 0;
    });
    if (!sortInfo.ascending) entries.reverse();
  }

  const start = (currentPage - 1) * rowsPerPage;
  const page = rowsPerPage ? entries.slice(start, start + rowsPerPage) : entries;

  page.forEach(([key, it]) => {
    const tr = document.createElement('tr');

    // 先頭チェックボックス
    const tdChk = document.createElement('td');
    tdChk.style.width = '3ch';
    const cb = document.createElement('input');
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

    // データセル
    const fixedTotal = fixedWidthMapping.serialization + fixedWidthMapping.type +
      fixedWidthMapping.create_date + fixedWidthMapping.update_date;
    const varTotal = variableWeightMapping.title + variableWeightMapping.author + variableWeightMapping.tags;

    columns.forEach(c => {
      const td = document.createElement('td');
      if (hiddenCols.includes(c)) {
        td.classList.add('hidden-column');
      } else {
        td.classList.add(`td-${c}`);
        if (fixedWidthMapping[c]) td.style.width = fixedWidthMapping[c] + 'ch';
        else td.style.width = `calc((100% - ${fixedTotal}ch) * ${(variableWeightMapping[c] || 0) / varTotal})`;

        switch (c) {
          case 'serialization':
            td.textContent = it.serialization ?? '';
            break;
          case 'title': {
            const a = document.createElement('a');
            a.href = `./${key}/`;
            a.textContent = it.title;
            td.appendChild(a);
            break;
          }
          case 'author': {
            const a = document.createElement('a');
            a.href = it.author_url;
            a.target = '_blank';
            a.textContent = it.author;
            a.addEventListener('click', e => {
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
              it.all_tags.forEach(t => {
                const s = document.createElement('span');
                s.textContent = t;
                s.classList.add('tag-item');
                s.style.cursor = 'pointer';
                s.addEventListener('click', () => tagFilterClick(t));
                td.appendChild(s);
              });
            }
            break;
          case 'create_date':
          case 'update_date':
            td.textContent = formatDateTime(it[c] ?? '');
            break;
          default:
            td.textContent = it[c] ?? '';
        }
      }
      tr.appendChild(td);
    });

    tbody.appendChild(tr);
  });
}

/* --------------------------------------------------
   ページネーション
-------------------------------------------------- */
function updatePagination() {
  const pageInfo = document.getElementById('page-info');
  const totalItems = Object.entries(tableData).filter(([, it]) => {
    if (typeFilter !== 'all' && it.type !== typeFilter) return false;
    if (hiddenAuthors.includes(it.author)) return false;
    if (filteredAuthors.length && !filteredAuthors.includes(it.author_id || it.author)) return false;
    if (it.all_tags) {
      const inc = includedTags.every(t => it.all_tags.includes(t));
      const exc = excludedTags.every(t => !it.all_tags.includes(t));
      if (!(inc && exc)) return false;
    }
    return true;
  }).length;
  const totalPages = rowsPerPage ? Math.ceil(totalItems / rowsPerPage) : 1;
  pageInfo.textContent = `${currentPage} / ${totalPages || 1}`;
}

function nextPage() {
  updatePagination();
  const totalPages = rowsPerPage ? Math.ceil(Object.entries(tableData).length / rowsPerPage) : 1;
  if (currentPage < totalPages) {
    currentPage++;
    saveSettings();
    renderTable();
    updatePagination();
  }
}

function prevPage() {
  if (currentPage > 1) {
    currentPage--;
    saveSettings();
    renderTable();
    updatePagination();
  }
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
  document.getElementById('selected-count').textContent = `選択された件数: ${selectedRows.size}`;
}

function showCopyPopup(titles) {
  const overlay = document.createElement('div');
  overlay.style.cssText = 'position:fixed;top:0;left:0;width:100%;height:100%;background:rgba(0,0,0,0.5);display:flex;align-items:center;justify-content:center;';
  const box = document.createElement('div');
  box.style.cssText = 'background:#fff;padding:1em;border-radius:5px;max-width:80%;max-height:80%;overflow:auto;';
  box.innerHTML = `<strong>リンク先をコピーしました</strong><br><br>${titles.join('<br>')}<br><br><button id="close-copy-popup">閉じる</button>`;
  overlay.appendChild(box);
  document.body.appendChild(overlay);
  document.getElementById('close-copy-popup').addEventListener('click', () => document.body.removeChild(overlay));
}

function copySelected() {
  const links = [];
  const titles = [];
  document.querySelectorAll('.row-checkbox').forEach(cb => {
    if (cb.checked) {
      const row = cb.closest('tr');
      const a = row.querySelector('.td-title a');
      if (a) { links.push(a.href); titles.push(a.textContent); }
    }
  });
  navigator.clipboard.writeText(links.join('\n'))
    .then(() => showCopyPopup(titles))
    .catch(err => alert('コピーに失敗しました: ' + err));
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
document.addEventListener('keydown', event => {
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
      Object.keys(tableData).forEach(k => selectedRows.add(k));
    } else if (event.ctrlKey && !event.shiftKey) {
      // 表示中の行だけ選択
      document.querySelectorAll('#user-table-body .row-checkbox').forEach(cb => {
        cb.checked = true;
        selectedRows.add(cb.dataset.key);
      });
    }
    updateSelectedCount();
    renderTable();
  }
});

/* --------------------------------------------------
   起動
-------------------------------------------------- */
fetchData().then(() => applySettingsToUI());
