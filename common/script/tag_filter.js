/**
 * TagFilter - タグ絞り込み機能の共通ライブラリ
 * 各サイトのindexページと簡易小説リーダーで共用
 */

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

/**
 * TagFilter クラス
 * @param {Object} options - 設定オプション
 * @param {string} options.containerId - タグフィルター表示先の要素ID
 * @param {Function} options.onFilterChange - フィルター変更時のコールバック
 * @param {Array} [options.initialIncludedTags=[]] - 初期含むタグ
 * @param {Array} [options.initialExcludedTags=[]] - 初期含まないタグ
 * @param {string} [options.initialIncludeOperator='AND'] - 初期含むタグの演算子
 * @param {string} [options.initialExcludeOperator='AND'] - 初期含まないタグの演算子
 */
function TagFilter(options) {
  this.containerId = options.containerId;
  this.onFilterChange = options.onFilterChange || function() {};
  
  this.includedTags = options.initialIncludedTags || [];
  this.excludedTags = options.initialExcludedTags || [];
  this.includeOperator = options.initialIncludeOperator || 'AND';
  this.excludeOperator = options.initialExcludeOperator || 'AND';
  
  this.isIncludeCollapsed = false;
  this.isExcludeCollapsed = false;
}

/**
 * タグフィルターUIを描画
 */
TagFilter.prototype.render = function() {
  const container = document.getElementById(this.containerId);
  if (!container) return;
  
  container.innerHTML = '';
  
  // 含むタグセクション
  this._buildTagSection(container, 'include');
  
  // 含まないタグセクション
  this._buildTagSection(container, 'exclude');
};

/**
 * タグセクションを構築
 * @private
 */
TagFilter.prototype._buildTagSection = function(container, kind) {
  const isInclude = kind === 'include';
  const tagArr = isInclude ? this.includedTags : this.excludedTags;
  const collapsed = isInclude ? this.isIncludeCollapsed : this.isExcludeCollapsed;
  const operator = isInclude ? this.includeOperator : this.excludeOperator;
  const title = isInclude ? '含むタグ' : '含まないタグ';
  
  // セクションラッパー
  const section = document.createElement('div');
  section.className = 'tag-filter-section tag-filter-' + kind;
  
  // ヘッダー
  const header = document.createElement('div');
  header.className = 'tag-filter-header';
  header.style.cursor = 'pointer';
  header.innerHTML = '<span class="tag-filter-title">' + title + '</span><span class="tag-filter-toggle">' + (collapsed ? '[+]' : '[-]') + '</span>';
  
  const self = this;
  header.addEventListener('click', function() {
    if (isInclude) {
      self.isIncludeCollapsed = !self.isIncludeCollapsed;
    } else {
      self.isExcludeCollapsed = !self.isExcludeCollapsed;
    }
    self.render();
  });
  
  section.appendChild(header);
  
  // 折りたたまれている場合はここで終了
  if (collapsed) {
    container.appendChild(section);
    return;
  }
  
  // 演算子選択
  const opWrapper = document.createElement('div');
  opWrapper.className = 'tag-filter-operator';
  opWrapper.appendChild(document.createTextNode('条件: '));
  
  const sel = document.createElement('select');
  sel.className = 'tag-filter-op-select';
  ['AND', 'OR'].forEach(function(op) {
    var o = document.createElement('option');
    o.value = o.textContent = op;
    sel.appendChild(o);
  });
  sel.value = operator;
  sel.addEventListener('change', function() {
    if (isInclude) {
      self.includeOperator = sel.value;
    } else {
      self.excludeOperator = sel.value;
    }
    self.onFilterChange(self.getFilterState());
  });
  
  opWrapper.appendChild(sel);
  section.appendChild(opWrapper);
  
  // タグリスト
  const tagList = document.createElement('div');
  tagList.className = 'tag-filter-list';
  
  if (tagArr.length === 0) {
    const emptyMsg = document.createElement('div');
    emptyMsg.className = 'tag-filter-empty';
    emptyMsg.textContent = 'タグを選択してください';
    tagList.appendChild(emptyMsg);
  } else {
    tagArr.forEach(function(tag) {
      const tagItem = document.createElement('label');
      tagItem.className = 'tag-filter-item';
      
      const cb = document.createElement('input');
      cb.type = 'checkbox';
      cb.checked = true;
      cb.addEventListener('change', function() {
        self.removeTag(kind, tag);
      });
      
      const span = document.createElement('span');
      span.textContent = tag;
      
      tagItem.appendChild(cb);
      tagItem.appendChild(span);
      tagList.appendChild(tagItem);
    });
  }
  
  section.appendChild(tagList);
  container.appendChild(section);
};

/**
 * タグを追加
 * @param {string} kind - 'include' または 'exclude'
 * @param {string} tag - タグ名
 */
TagFilter.prototype.addTag = function(kind, tag) {
  if (kind === 'include') {
    if (!this.includedTags.includes(tag)) {
      this.includedTags.push(tag);
    }
    // 含むタグに追加する場合は含まないタグから削除
    this.excludedTags = this.excludedTags.filter(function(t) { return t !== tag; });
  } else {
    if (!this.excludedTags.includes(tag)) {
      this.excludedTags.push(tag);
    }
    // 含まないタグに追加する場合は含むタグから削除
    this.includedTags = this.includedTags.filter(function(t) { return t !== tag; });
  }
  
  this.render();
  this.onFilterChange(this.getFilterState());
};

/**
 * タグを削除
 * @param {string} kind - 'include' または 'exclude'
 * @param {string} tag - タグ名
 */
TagFilter.prototype.removeTag = function(kind, tag) {
  if (kind === 'include') {
    this.includedTags = this.includedTags.filter(function(t) { return t !== tag; });
  } else {
    this.excludedTags = this.excludedTags.filter(function(t) { return t !== tag; });
  }
  
  this.render();
  this.onFilterChange(this.getFilterState());
};

/**
 * タグをリセット
 * @param {string} [kind] - 'include', 'exclude', または undefined（両方）
 */
TagFilter.prototype.resetTags = function(kind) {
  if (!kind || kind === 'include') {
    this.includedTags = [];
  }
  if (!kind || kind === 'exclude') {
    this.excludedTags = [];
  }
  
  this.render();
  this.onFilterChange(this.getFilterState());
};

/**
 * 現在のフィルター状態を取得
 * @returns {Object}
 */
TagFilter.prototype.getFilterState = function() {
  return {
    includedTags: this.includedTags.slice(),
    excludedTags: this.excludedTags.slice(),
    includeOperator: this.includeOperator,
    excludeOperator: this.excludeOperator
  };
};

/**
 * フィルター状態を設定
 * @param {Object} state
 */
TagFilter.prototype.setFilterState = function(state) {
  this.includedTags = state.includedTags || [];
  this.excludedTags = state.excludedTags || [];
  this.includeOperator = state.includeOperator || 'AND';
  this.excludeOperator = state.excludeOperator || 'AND';
  this.render();
};

/**
 * アイテムがフィルター条件を満たすかチェック
 * @param {Object} item - チェック対象のアイテム
 * @param {Array} item.tags - アイテムのタグ配列
 * @returns {boolean}
 */
TagFilter.prototype.matches = function(item) {
  var tags = item.tags || item.all_tags || [];
  
  // 含むタグ判定
  var includeResult = true;
  if (this.includedTags.length) {
    if (this.includeOperator === 'AND') {
      includeResult = this.includedTags.every(function(tag) {
        return tags.indexOf(tag) !== -1;
      });
    } else {
      includeResult = this.includedTags.some(function(tag) {
        return tags.indexOf(tag) !== -1;
      });
    }
  }
  
  // 含まないタグ判定
  var excludeResult = true;
  if (this.excludedTags.length) {
    if (this.excludeOperator === 'AND') {
      excludeResult = !this.excludedTags.every(function(tag) {
        return tags.indexOf(tag) !== -1;
      });
    } else {
      excludeResult = !this.excludedTags.some(function(tag) {
        return tags.indexOf(tag) !== -1;
      });
    }
  }
  
  return includeResult && excludeResult;
};

/**
 * タグがクリックされた時の処理（確認ダイアログ付き）
 * @param {string} tag - タグ名
 * @param {HTMLElement} [anchorElement] - ツールチップ表示用の要素
 */
TagFilter.prototype.handleTagClick = function(tag, anchorElement) {
  var self = this;
  
  // 既に含まれている場合は削除
  if (this.includedTags.includes(tag)) {
    this.removeTag('include', tag);
    return;
  }
  if (this.excludedTags.includes(tag)) {
    this.removeTag('exclude', tag);
    return;
  }
  
  // カスタム確認UI
  if (anchorElement && typeof TagFilterTooltip !== 'undefined') {
    TagFilterTooltip.show(anchorElement, tag, function(action) {
      if (action === 'include') {
        self.addTag('include', tag);
      } else if (action === 'exclude') {
        self.addTag('exclude', tag);
      }
    });
  } else {
    // フォールバック: confirmダイアログ
    if (confirm('「' + tag + '」を含むフィルターに追加しますか？\nキャンセルを押すと含まないフィルターに追加します。')) {
      self.addTag('include', tag);
    } else {
      self.addTag('exclude', tag);
    }
  }
};

/**
 * TagFilterTooltip - タグ選択用ツールチップ
 */
var TagFilterTooltip = {
  currentTooltip: null,
  
  show: function(anchorElement, tag, callback) {
    this.hide();
    
    var tooltip = document.createElement('div');
    tooltip.className = 'tag-filter-tooltip';
    tooltip.style.cssText = 'position:absolute;z-index:1000;background:white;border:1px solid #ccc;border-radius:4px;padding:8px;box-shadow:0 2px 8px rgba(0,0,0,0.15);';
    
    var title = document.createElement('div');
    title.textContent = '「' + tag + '」を:';
    title.style.cssText = 'font-weight:bold;margin-bottom:8px;';
    tooltip.appendChild(title);
    
    var includeBtn = document.createElement('button');
    includeBtn.textContent = '含む';
    includeBtn.style.cssText = 'margin-right:8px;padding:4px 12px;background:#10b981;color:white;border:none;border-radius:4px;cursor:pointer;';
    includeBtn.onclick = function() {
      callback('include');
      TagFilterTooltip.hide();
    };
    tooltip.appendChild(includeBtn);
    
    var excludeBtn = document.createElement('button');
    excludeBtn.textContent = '含まない';
    excludeBtn.style.cssText = 'padding:4px 12px;background:#ef4444;color:white;border:none;border-radius:4px;cursor:pointer;';
    excludeBtn.onclick = function() {
      callback('exclude');
      TagFilterTooltip.hide();
    };
    tooltip.appendChild(excludeBtn);
    
    var cancelBtn = document.createElement('button');
    cancelBtn.textContent = 'キャンセル';
    cancelBtn.style.cssText = 'margin-left:8px;padding:4px 12px;background:#ccc;border:none;border-radius:4px;cursor:pointer;';
    cancelBtn.onclick = function() {
      TagFilterTooltip.hide();
    };
    tooltip.appendChild(cancelBtn);
    
    // 位置計算
    var rect = anchorElement.getBoundingClientRect();
    tooltip.style.left = rect.left + 'px';
    tooltip.style.top = (rect.bottom + window.scrollY) + 'px';
    
    document.body.appendChild(tooltip);
    this.currentTooltip = tooltip;
    
    // 外部クリックで閉じる
    setTimeout(function() {
      document.addEventListener('click', TagFilterTooltip._outsideClickHandler);
    }, 0);
  },
  
  hide: function() {
    if (this.currentTooltip) {
      this.currentTooltip.remove();
      this.currentTooltip = null;
      document.removeEventListener('click', this._outsideClickHandler);
    }
  },
  
  _outsideClickHandler: function(e) {
    if (TagFilterTooltip.currentTooltip && !TagFilterTooltip.currentTooltip.contains(e.target)) {
      TagFilterTooltip.hide();
    }
  }
};

// グローバルに公開
window.TagFilter = TagFilter;
window.TagFilterTooltip = TagFilterTooltip;
