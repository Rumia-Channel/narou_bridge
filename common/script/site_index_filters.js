// site_index_filters.js — フィルター・タグ・作者関連UI

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

  // 既存のポップアップがあれば閉じる
  var existing = document.getElementById('author-filter-popup-overlay');
  if (existing) existing.remove();

  var overlay = document.createElement('div');
  overlay.id = 'author-filter-popup-overlay';
  overlay.className = 'tag-popup-overlay';

  var box = document.createElement('div');
  box.className = 'tag-popup-box';

  var title = document.createElement('div');
  title.className = 'tag-popup-title';
  title.textContent = '「' + author + '」';
  box.appendChild(title);

  var desc = document.createElement('div');
  desc.className = 'tag-popup-desc';
  desc.textContent = '作者フィルターの操作を選択してください';
  box.appendChild(desc);

  var btnGroup = document.createElement('div');
  btnGroup.className = 'tag-popup-btn-group';
  btnGroup.style.gridTemplateColumns = '1fr';

  function doAction(action) {
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
    }
    currentPage = 1;
    renderTable();
    updatePagination();
    overlay.remove();
  }

  var actions = [
    { label: 'この作者で絞り込み', value: '1', cls: 'tag-popup-site-include' },
    { label: 'サイト別の非表示リストに追加', value: '2', cls: 'tag-popup-site-exclude' },
    { label: '共通の非表示リストに追加（全サイト適用）', value: '3', cls: 'tag-popup-global-exclude' }
  ];

  actions.forEach(function (item) {
    var btn = document.createElement('button');
    btn.className = 'btn btn-sm tag-popup-btn ' + item.cls;
    btn.textContent = item.label;
    btn.addEventListener('click', function () {
      doAction(item.value);
    });
    btnGroup.appendChild(btn);
  });

  box.appendChild(btnGroup);

  // キャンセルボタン
  var cancelBtn = document.createElement('button');
  cancelBtn.className = 'btn btn-sm btn-outline tag-popup-cancel';
  cancelBtn.textContent = 'キャンセル';
  cancelBtn.addEventListener('click', function () { overlay.remove(); });
  box.appendChild(cancelBtn);

  overlay.appendChild(box);

  // オーバーレイクリックで閉じる
  overlay.addEventListener('click', function (e) {
    if (e.target === overlay) overlay.remove();
  });

  document.body.appendChild(overlay);
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
   タグクリック時のポップアップ
-------------------------------------------------- */
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
