// index.js — メインエントリ（データ取得・UI構築・イベント登録・キーボードショートカット）
//
// 依存する外部スクリプト（読み込み順）:
//   1. ui_components.js    — showToast, showModal, showContextMenu
//   2. tag_filter.js       — TagFilterTooltip
//   3. site_index_polyfills.js — forEach / Set polyfill
//   4. site_index_settings.js  — グローバル変数, loadSettings, saveSettings, saveGlobalSettings
//   5. site_index_filters.js   — フィルター関連UI, getFilteredEntries
//   6. site_index_table.js     — テーブル描画, ページネーション, ソート, createCollapsibleHeader
//   7. index.js (このファイル)  — fetchData, buildUI, イベントハンドラ

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
   UI 構築（初期描画）
-------------------------------------------------- */
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
   初期イベント登録
-------------------------------------------------- */
document.addEventListener('DOMContentLoaded', () => {
  document.getElementById('copy-selected-button').addEventListener('click', copySelected);

  document.getElementById('reset-localstorage-button').addEventListener('click', () => {
    showModal({
      title: 'ローカルストレージリセット',
      message: 'ローカルストレージをリセットしますか？（サイト別設定のみ）',
      confirmText: 'リセット',
      confirmStyle: 'danger'
    }).then(function (ok) {
      if (!ok) return;
      localStorage.removeItem(siteSettingsKey);
      localStorage.removeItem('pageWidth');
      localStorage.removeItem('siteIndexWidth');
      location.reload();
    });
  });

  document.getElementById('reset-hidden-authors-button').addEventListener('click', () => {
    showModal({
      title: '非表示作者リセット',
      message: 'サイト別の非表示作者をリセットしますか？',
      confirmText: 'リセット',
      confirmStyle: 'danger'
    }).then(function (ok) {
      if (!ok) return;
      hiddenAuthors = [];
      saveSettings();
      renderHiddenAuthors();
      renderTable();
    });
  });

  document.getElementById('reset-include-tags-button').addEventListener('click', () => {
    showModal({
      title: '含むタグリセット',
      message: 'サイト別の含むタグをリセットしますか？',
      confirmText: 'リセット',
      confirmStyle: 'danger'
    }).then(function (ok) {
      if (!ok) return;
      includedTags = [];
      saveSettings();
      renderTagFilters();
      renderTable();
    });
  });

  document.getElementById('reset-exclude-tags-button').addEventListener('click', () => {
    showModal({
      title: '含まないタグリセット',
      message: 'サイト別の含まないタグをリセットしますか？',
      confirmText: 'リセット',
      confirmStyle: 'danger'
    }).then(function (ok) {
      if (!ok) return;
      excludedTags = [];
      saveSettings();
      renderTagFilters();
      renderTable();
    });
  });

  document.getElementById('reset-author-filter-button').addEventListener('click', () => {
    showModal({
      title: '作者絞り込みリセット',
      message: '作者絞り込みをリセットしますか？',
      confirmText: 'リセット',
      confirmStyle: 'danger'
    }).then(function (ok) {
      if (!ok) return;
      filteredAuthors = [];
      const dd = document.getElementById('author-filter-dropdown');
      if (dd) dd.value = '';
      saveSettings();
      renderTable();
    });
  });

  // グローバルフィルターリセットボタン
  var globalResetIncBtn = document.getElementById('reset-global-include-tags-button');
  if (globalResetIncBtn) {
    globalResetIncBtn.addEventListener('click', () => {
      showModal({
        title: '共通 含むタグリセット',
        message: '共通の含むタグをリセットしますか？（全サイトに影響します）',
        confirmText: 'リセット',
        confirmStyle: 'danger'
      }).then(function (ok) {
        if (!ok) return;
        globalIncludedTags = [];
        saveGlobalSettings();
        renderGlobalFilters();
        renderTable();
      });
    });
  }

  var globalResetExcBtn = document.getElementById('reset-global-exclude-tags-button');
  if (globalResetExcBtn) {
    globalResetExcBtn.addEventListener('click', () => {
      showModal({
        title: '共通 含まないタグリセット',
        message: '共通の含まないタグをリセットしますか？（全サイトに影響します）',
        confirmText: 'リセット',
        confirmStyle: 'danger'
      }).then(function (ok) {
        if (!ok) return;
        globalExcludedTags = [];
        saveGlobalSettings();
        renderGlobalFilters();
        renderTable();
      });
    });
  }

  var globalResetAuthorsBtn = document.getElementById('reset-global-hidden-authors-button');
  if (globalResetAuthorsBtn) {
    globalResetAuthorsBtn.addEventListener('click', () => {
      showModal({
        title: '共通 非表示作者リセット',
        message: '共通の非表示作者をリセットしますか？（全サイトに影響します）',
        confirmText: 'リセット',
        confirmStyle: 'danger'
      }).then(function (ok) {
        if (!ok) return;
        globalHiddenAuthors = [];
        saveGlobalSettings();
        renderGlobalHiddenAuthors();
        renderTable();
      });
    });
  }

  var globalResetAllBtn = document.getElementById('reset-global-all-button');
  if (globalResetAllBtn) {
    globalResetAllBtn.addEventListener('click', () => {
      showModal({
        title: '共通フィルター全リセット',
        message: '共通フィルター設定をすべてリセットしますか？（全サイトに影響します）',
        confirmText: '全リセット',
        confirmStyle: 'danger'
      }).then(function (ok) {
        if (!ok) return;
        localStorage.removeItem('globalFilterSettings');
        location.reload();
      });
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
    // フィルタ適用後のエントリを取得
    const filtered = getFilteredEntries();
    if (event.shiftKey && !event.ctrlKey) {
      // フィルタ後の全データを選択（全ページ）
      for (let i = 0; i < filtered.length; i++) {
        selectedRows.add(filtered[i][0]);
      }
    } else if (event.ctrlKey && !event.shiftKey) {
      // フィルタ後の現在ページの行だけ選択
      const start = (currentPage - 1) * rowsPerPage;
      const pageEntries = rowsPerPage
        ? filtered.slice(start, start + rowsPerPage)
        : filtered;
      for (let i = 0; i < pageEntries.length; i++) {
        selectedRows.add(pageEntries[i][0]);
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
