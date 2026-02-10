/**
 * ui_components.js — 共通UIコンポーネント
 * ブラウザネイティブの alert / confirm / prompt を置き換えるインラインUI
 *
 * 提供する関数:
 *   showToast(message, options)        — 自動消去トースト通知 (alert 代替)
 *   showModal(options) → Promise       — 確認モーダル (confirm 代替)
 *   showContextMenu(x, y, items, options) — 位置指定コンテキストメニュー (prompt 代替)
 */

// ============================================================
//  showToast — トースト通知
// ============================================================
/**
 * @param {string} message - 表示メッセージ
 * @param {Object} [options]
 * @param {'info'|'success'|'warning'|'error'} [options.type='info']
 * @param {number} [options.duration=3000] - 自動消去までのミリ秒
 */
function showToast(message, options) {
  options = options || {};
  var type = options.type || 'info';
  var duration = typeof options.duration === 'number' ? options.duration : 3000;

  // コンテナが無ければ作成
  var container = document.getElementById('ui-toast-container');
  if (!container) {
    container = document.createElement('div');
    container.id = 'ui-toast-container';
    container.className = 'ui-toast-container';
    document.body.appendChild(container);
  }

  var toast = document.createElement('div');
  toast.className = 'ui-toast ui-toast-' + type;
  toast.textContent = message;

  // 閉じるボタン
  var closeBtn = document.createElement('button');
  closeBtn.className = 'ui-toast-close';
  closeBtn.textContent = '\u00d7'; // ×
  closeBtn.addEventListener('click', function () {
    removeToast(toast);
  });
  toast.appendChild(closeBtn);

  // スワイプで閉じる（モバイル対応）
  var touchStartX = 0;
  var touchStartY = 0;
  var swiping = false;

  toast.addEventListener('touchstart', function (e) {
    var touch = e.touches[0];
    touchStartX = touch.clientX;
    touchStartY = touch.clientY;
    swiping = false;
    toast.style.transition = 'none';
  }, { passive: true });

  toast.addEventListener('touchmove', function (e) {
    var touch = e.touches[0];
    var dx = touch.clientX - touchStartX;
    var dy = touch.clientY - touchStartY;

    // 水平方向のスワイプのみ（縦スクロールと干渉しないよう角度チェック）
    if (!swiping && Math.abs(dx) > 10 && Math.abs(dx) > Math.abs(dy)) {
      swiping = true;
    }

    if (swiping && dx > 0) {
      toast.style.transform = 'translateX(' + dx + 'px)';
      toast.style.opacity = Math.max(0, 1 - dx / 150);
    }
  }, { passive: true });

  toast.addEventListener('touchend', function () {
    if (!swiping) {
      toast.style.transition = '';
      toast.style.transform = '';
      toast.style.opacity = '';
      return;
    }

    // スワイプ距離が80px以上なら閉じる
    var currentX = parseFloat(toast.style.transform.replace(/[^0-9.-]/g, '')) || 0;
    toast.style.transition = '';

    if (currentX > 80) {
      removeToast(toast, true);
    } else {
      // 元の位置に戻す
      toast.style.transform = '';
      toast.style.opacity = '';
    }
  }, { passive: true });

  container.appendChild(toast);

  // アニメーション開始
  requestAnimationFrame(function () {
    toast.classList.add('ui-toast-show');
  });

  // 自動消去
  if (duration > 0) {
    setTimeout(function () {
      removeToast(toast);
    }, duration);
  }

  return toast;
}

function removeToast(toast, swipe) {
  if (!toast || toast._removing) return;
  toast._removing = true;
  toast.classList.remove('ui-toast-show');
  toast.classList.add(swipe ? 'ui-toast-swipe-hide' : 'ui-toast-hide');
  setTimeout(function () {
    if (toast.parentNode) toast.parentNode.removeChild(toast);
  }, 300);
}

// ============================================================
//  showModal — 確認モーダルダイアログ
// ============================================================
/**
 * @param {Object} options
 * @param {string} [options.title='確認'] - タイトル
 * @param {string} options.message - 本文
 * @param {string} [options.confirmText='OK']
 * @param {string} [options.cancelText='キャンセル']
 * @param {'primary'|'danger'|'warning'} [options.confirmStyle='primary']
 * @returns {Promise<boolean>} - OKなら true, キャンセルなら false
 */
function showModal(options) {
  options = options || {};
  var title = options.title || '確認';
  var message = options.message || '';
  var confirmText = options.confirmText || 'OK';
  var cancelText = options.cancelText || 'キャンセル';
  var confirmStyle = options.confirmStyle || 'primary';

  return new Promise(function (resolve) {
    // オーバーレイ
    var overlay = document.createElement('div');
    overlay.className = 'ui-modal-overlay';

    // モーダル本体
    var modal = document.createElement('div');
    modal.className = 'ui-modal';

    // タイトル
    var titleEl = document.createElement('div');
    titleEl.className = 'ui-modal-title';
    titleEl.textContent = title;
    modal.appendChild(titleEl);

    // メッセージ
    var msgEl = document.createElement('div');
    msgEl.className = 'ui-modal-message';
    // 改行を <br> に変換して表示
    msgEl.innerHTML = message.replace(/\n/g, '<br>');
    modal.appendChild(msgEl);

    // ボタン群
    var btnGroup = document.createElement('div');
    btnGroup.className = 'ui-modal-buttons';

    var cancelBtn = document.createElement('button');
    cancelBtn.className = 'ui-modal-btn ui-modal-btn-cancel';
    cancelBtn.textContent = cancelText;
    cancelBtn.addEventListener('click', function () {
      close(false);
    });
    btnGroup.appendChild(cancelBtn);

    var confirmBtn = document.createElement('button');
    confirmBtn.className = 'ui-modal-btn ui-modal-btn-' + confirmStyle;
    confirmBtn.textContent = confirmText;
    confirmBtn.addEventListener('click', function () {
      close(true);
    });
    btnGroup.appendChild(confirmBtn);

    modal.appendChild(btnGroup);
    overlay.appendChild(modal);
    document.body.appendChild(overlay);

    // フォーカスをconfirmに
    requestAnimationFrame(function () {
      overlay.classList.add('ui-modal-overlay-show');
      confirmBtn.focus();
    });

    // Escキーでキャンセル
    function onKeydown(e) {
      if (e.key === 'Escape') {
        e.stopPropagation();
        close(false);
      }
    }
    document.addEventListener('keydown', onKeydown, true);

    // オーバーレイクリックでキャンセル
    overlay.addEventListener('click', function (e) {
      if (e.target === overlay) close(false);
    });

    function close(result) {
      document.removeEventListener('keydown', onKeydown, true);
      overlay.classList.remove('ui-modal-overlay-show');
      overlay.classList.add('ui-modal-overlay-hide');
      setTimeout(function () {
        if (overlay.parentNode) overlay.parentNode.removeChild(overlay);
      }, 200);
      resolve(result);
    }
  });
}

// ============================================================
//  showContextMenu — コンテキストメニュー
// ============================================================
/**
 * @param {number} x - 表示位置 (clientX)
 * @param {number} y - 表示位置 (clientY)
 * @param {Array<{label:string, value:*}>} items - メニュー項目
 * @param {Object} [options]
 * @param {string} [options.title] - メニュータイトル
 * @returns {Promise<*>} - 選択された項目の value、キャンセルなら null
 */
function showContextMenu(x, y, items, options) {
  options = options || {};

  return new Promise(function (resolve) {
    // 既存メニューを閉じる
    closeContextMenu();

    // 背景
    var backdrop = document.createElement('div');
    backdrop.className = 'ui-ctx-backdrop';
    backdrop.id = 'ui-ctx-backdrop';

    // メニュー
    var menu = document.createElement('div');
    menu.className = 'ui-ctx-menu';

    // タイトル
    if (options.title) {
      var titleEl = document.createElement('div');
      titleEl.className = 'ui-ctx-title';
      titleEl.textContent = options.title;
      menu.appendChild(titleEl);
    }

    // 項目
    var itemButtons = [];
    items.forEach(function (item) {
      var btn = document.createElement('button');
      btn.className = 'ui-ctx-item';
      btn.textContent = item.label;
      btn.addEventListener('click', function () {
        close(item.value);
      });
      menu.appendChild(btn);
      itemButtons.push(btn);
    });

    // キャンセル
    var cancelBtn = document.createElement('button');
    cancelBtn.className = 'ui-ctx-item ui-ctx-cancel';
    cancelBtn.textContent = 'キャンセル';
    cancelBtn.addEventListener('click', function () {
      close(null);
    });
    menu.appendChild(cancelBtn);
    itemButtons.push(cancelBtn);

    // キーボードナビゲーション用の現在フォーカスインデックス (-1 = なし)
    var focusIndex = -1;

    function setFocusIndex(idx) {
      // 範囲をクランプ
      if (idx < 0) idx = itemButtons.length - 1;
      if (idx >= itemButtons.length) idx = 0;
      focusIndex = idx;
      // 全アイテムからフォーカス表示を外す
      for (var i = 0; i < itemButtons.length; i++) {
        itemButtons[i].classList.remove('ui-ctx-item-focus');
      }
      itemButtons[focusIndex].classList.add('ui-ctx-item-focus');
      itemButtons[focusIndex].focus();
    }

    backdrop.appendChild(menu);
    document.body.appendChild(backdrop);

    // 位置調整（画面外にはみ出さないように）
    // backdrop は position:fixed なので座標はビューポート基準
    requestAnimationFrame(function () {
      var rect = menu.getBoundingClientRect();
      var posX = x;
      var posY = y;

      // 右端チェック
      if (posX + rect.width > window.innerWidth - 8) {
        posX = window.innerWidth - rect.width - 8;
      }
      // 左端チェック
      if (posX < 8) posX = 8;

      // 下端チェック
      if (y + rect.height > window.innerHeight - 8) {
        posY = y - rect.height;
      }

      menu.style.left = posX + 'px';
      menu.style.top = posY + 'px';
      menu.classList.add('ui-ctx-menu-show');

      // 最初の項目にフォーカス
      if (itemButtons.length > 0) {
        setFocusIndex(0);
      }
    });

    // 背景クリックでキャンセル
    backdrop.addEventListener('click', function (e) {
      if (e.target === backdrop) close(null);
    });

    // Escキー / 矢印キー / Enterでナビゲーション
    function onKeydown(e) {
      if (e.key === 'Escape') {
        e.stopPropagation();
        e.preventDefault();
        close(null);
      } else if (e.key === 'ArrowDown') {
        e.stopPropagation();
        e.preventDefault();
        setFocusIndex(focusIndex + 1);
      } else if (e.key === 'ArrowUp') {
        e.stopPropagation();
        e.preventDefault();
        setFocusIndex(focusIndex - 1);
      } else if (e.key === 'Enter' && focusIndex >= 0) {
        e.stopPropagation();
        e.preventDefault();
        itemButtons[focusIndex].click();
      }
    }
    document.addEventListener('keydown', onKeydown, true);

    function close(result) {
      document.removeEventListener('keydown', onKeydown, true);
      menu.classList.remove('ui-ctx-menu-show');
      menu.classList.add('ui-ctx-menu-hide');
      setTimeout(function () {
        if (backdrop.parentNode) backdrop.parentNode.removeChild(backdrop);
      }, 200);
      resolve(result);
    }
  });
}

function closeContextMenu() {
  var existing = document.getElementById('ui-ctx-backdrop');
  if (existing && existing.parentNode) {
    existing.parentNode.removeChild(existing);
  }
}

// グローバルに公開
window.showToast = showToast;
window.showModal = showModal;
window.showContextMenu = showContextMenu;
window.closeContextMenu = closeContextMenu;
