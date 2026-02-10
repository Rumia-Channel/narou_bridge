// site_index_settings.js — グローバル変数・ローカルストレージ設定

// var を使用: Cloudflare等のスクリプト注入やキャッシュにより
// 同一スコープで再評価された場合の重複宣言エラーを防止する
var basePath = window.location.pathname.replace(/\/[^/]*$/, '/');
var siteSettingsKey = 'tableSettings_' + basePath;

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
   ローカルストレージ（設定の保存・読込）
   - tableSettings_<path>: サイト別設定（パスごとに独立）
   - globalFilterSettings: 全サイト共通フィルター
-------------------------------------------------- */
function loadSettings() {
  // 旧キー 'tableSettings' からのマイグレーション
  // (以前は全サイト共通だったため、新キーが未設定なら旧データを引き継ぐ)
  if (!localStorage.getItem(siteSettingsKey) && localStorage.getItem('tableSettings')) {
    localStorage.setItem(siteSettingsKey, localStorage.getItem('tableSettings'));
    localStorage.removeItem('tableSettings');
  }

  // サイト別設定の読み込み
  const s = JSON.parse(localStorage.getItem(siteSettingsKey)) || {};
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
  localStorage.setItem(siteSettingsKey, JSON.stringify(s));
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
