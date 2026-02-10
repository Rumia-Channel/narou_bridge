// site_index_table.js — テーブル描画・ソート・ページネーション・UI部品

/* --------------------------------------------------
   統一折りたたみヘッダー
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
            const authorUrl = it.author_url || '';
            const a = document.createElement('a');
            a.href = 'javascript:void(0)';
            a.textContent = authorName;
            a.addEventListener('click', (e) => {
              handleAuthorFiltering(authorName, authorId, e, authorUrl);
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

  if (links.length === 0) return;

  function onCopySuccess() {
    var msg = titles.length + '件のリンクをコピーしました';
    showToast(msg, { type: 'success' });
  }

  // WebKit compatibility: check if clipboard API is available
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(links.join('\n'))
      .then(function () { onCopySuccess(); })
      .catch((err) => showToast('コピーに失敗しました: ' + err, { type: 'error' }));
  } else {
    // Fallback for older browsers
    const textarea = document.createElement('textarea');
    textarea.value = links.join('\n');
    document.body.appendChild(textarea);
    textarea.select();
    try {
      document.execCommand('copy');
      onCopySuccess();
    } catch (err) {
      showToast('コピーに失敗しました: ' + err, { type: 'error' });
    }
    document.body.removeChild(textarea);
  }
}
