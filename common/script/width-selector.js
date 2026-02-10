// width-selector.js — 共通横幅セレクター
// 全ページ共通で使用。.app-header または header 要素にセレクトを挿入し、
// CSS変数 --page-width と localStorage 'pageWidth' で横幅を管理する。

(function () {
  'use strict';

  var STORAGE_KEY = 'pageWidth';
  var OLD_STORAGE_KEY = 'siteIndexWidth';
  var CSS_VAR = '--page-width';
  var OPTIONS = ['55%', '65%', '75%', '85%', '95%', '100%'];
  var DEFAULT_VALUE = '100%';

  /** localStorage migration: siteIndexWidth → pageWidth */
  function migrateStorage() {
    var oldVal = localStorage.getItem(OLD_STORAGE_KEY);
    if (oldVal && !localStorage.getItem(STORAGE_KEY)) {
      localStorage.setItem(STORAGE_KEY, oldVal);
    }
    // 旧キーは残しておく（index.js 側でも参照している可能性がある移行期間中）
    // 完全移行後に localStorage.removeItem(OLD_STORAGE_KEY) を呼んでもよい
  }

  /** CSS変数を :root に適用 */
  function applyWidth(value) {
    document.documentElement.style.setProperty(CSS_VAR, value);
  }

  /** 保存されている幅を取得 */
  function getSavedWidth() {
    return localStorage.getItem(STORAGE_KEY) || DEFAULT_VALUE;
  }

  /** セレクト要素を作成してヘッダーに挿入 */
  function createSelector(header) {
    // 既にセレクターがある場合はスキップ
    if (header.querySelector('.header-width-select')) return;

    var select = document.createElement('select');
    select.className = 'header-width-select';
    select.setAttribute('aria-label', '横幅設定');

    for (var i = 0; i < OPTIONS.length; i++) {
      var opt = document.createElement('option');
      opt.value = OPTIONS[i];
      opt.textContent = OPTIONS[i];
      select.appendChild(opt);
    }

    var saved = getSavedWidth();
    select.value = saved;

    select.addEventListener('change', function () {
      var w = select.value;
      localStorage.setItem(STORAGE_KEY, w);
      applyWidth(w);
      // image.js 等がコンテナ幅の変化に追従できるよう resize を発火
      if (typeof window.adjustImages === 'function') {
        setTimeout(window.adjustImages, 50);
      }
    });

    header.appendChild(select);
  }

  /** 初期化 */
  function init() {
    migrateStorage();

    // 保存値がある場合のみ CSS変数を適用
    // （未設定時は各CSSのフォールバック値に任せる）
    var saved = localStorage.getItem(STORAGE_KEY);
    if (saved) {
      applyWidth(saved);
    }

    // ヘッダーを探してセレクターを挿入
    var header = document.querySelector('.app-header') || document.querySelector('header');
    if (header) {
      createSelector(header);
    }
  }

  // DOM が読み込まれたら初期化
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
