// URL に現在のクエリパラメータを付与してリダイレクト
function redirectWithParams(baseURL) {
  var params = document.location.search;
  window.location.href = baseURL + params;
}

// UUID 形式のリクエスト ID を生成
function generateRequestId() {
  return "xxxx-xxxx-4xxx-yxxx-xxxx".replace(/[xy]/g, function (c) {
    var r = Math.random() * 16 | 0;
    var v = (c === "x") ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

// 関数を指定ミリ秒後に実行するデバウンス
function debounce(func, delay) {
  var timeoutId;
  return function () {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(func, delay);
  };
}

// submit() を 1秒デバウンス
var debouncedSubmit = debounce(submit, 1000);

// 新規登録用
function submit() {
  var input1 = document.getElementById("input1").value;
  var requestId = generateRequestId();
  var url = POST_URL + "?add=" + encodeURIComponent(input1);
  
  // WebKit compatibility: use alternative to URL().searchParams
  var keyParam = getUrlParameter("key");
  if (keyParam) url += "&key=" + encodeURIComponent(keyParam);

  var xhr = new XMLHttpRequest();
  xhr.open("POST", url, true);
  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");
  xhr.onreadystatechange = function () {
    if (xhr.readyState === 4) {
      var res = JSON.parse(xhr.responseText);
      if (xhr.status === 200) showToast(res.message || "送信成功", { type: 'success' });
      else showToast(res.message || "送信失敗", { type: 'error' });
    }
  };
  xhr.send("add=" + encodeURIComponent(input1) + "&request_id=" + requestId);
  // 入力フィールドをクリア
  document.getElementById("input1").value = "";
  // フォーカスを戻す
  document.getElementById("input1").focus();
}

// WebKit compatibility: URL parameter extraction helper
function getUrlParameter(name) {
  var query = window.location.search.substring(1);
  var params = query.split("&");
  for (var i = 0; i < params.length; i++) {
    var param = params[i].split("=");
    if (param[0] === name) {
      return decodeURIComponent(param[1]);
    }
  }
  return null;
}

// 更新用
function submitUpdate(key) {
  var label = key === 'all' ? '全サイト' : key;
  showModal({
    title: '更新',
    message: '「' + label + '」の更新を実行しますか？',
    confirmText: '更新を実行',
    confirmStyle: 'primary'
  }).then(function (ok) {
    if (!ok) return;

    var requestId = generateRequestId();
    var url = POST_URL + "?update=" + encodeURIComponent(key);

    var xhr = new XMLHttpRequest();
    xhr.open("POST", url, true);
    xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");
    xhr.onreadystatechange = function () {
      if (xhr.readyState === 4) {
        var res = JSON.parse(xhr.responseText);
        if (xhr.status === 200) showToast(res.message || "更新成功", { type: 'success' });
        else showToast(res.message || "更新失敗", { type: 'error' });
      }
    };
    xhr.send("update=" + encodeURIComponent(key) + "&request_id=" + requestId);
  });
}

// 変換用
function submitConvert(key) {
  var label = key === 'all' ? '全サイト' : key;
  showModal({
    title: '変換',
    message: '「' + label + '」の変換を実行しますか？',
    confirmText: '変換を実行',
    confirmStyle: 'primary'
  }).then(function (ok) {
    if (!ok) return;

    var requestId = generateRequestId();
    var url = POST_URL + "?convert=" + encodeURIComponent(key);

    var xhr = new XMLHttpRequest();
    xhr.open("POST", url, true);
    xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");
    xhr.onreadystatechange = function () {
      if (xhr.readyState === 4) {
        var res = JSON.parse(xhr.responseText);
        if (xhr.status === 200) showToast(res.message || "変換成功", { type: 'success' });
        else showToast(res.message || "変換失敗", { type: 'error' });
      }
    };
    xhr.send("convert=" + encodeURIComponent(key) + "&request_id=" + requestId);
  });
}

// 再ダウンロード用
function submitReDownload(key) {
  var label = key === 'all' ? '全サイト' : key;
  showModal({
    title: '再ダウンロード',
    message: '「' + label + '」の再ダウンロードを実行しますか？\nすべてのデータを再取得します。',
    confirmText: '再DLを実行',
    confirmStyle: 'danger'
  }).then(function (ok) {
    if (!ok) return;

    var requestId = generateRequestId();
    var url = POST_URL + "?re_download=" + encodeURIComponent(key);

    var xhr = new XMLHttpRequest();
    xhr.open("POST", url, true);
    xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");
    xhr.onreadystatechange = function () {
      if (xhr.readyState === 4) {
        var res = JSON.parse(xhr.responseText);
        if (xhr.status === 200) showToast(res.message || "再ダウンロード成功", { type: 'success' });
        else showToast(res.message || "再ダウンロード失敗", { type: 'error' });
      }
    };
    xhr.send("re_download=" + encodeURIComponent(key) + "&request_id=" + requestId);
  });
}

// データ修復用
function submitRepair(key) {
  showModal({
    title: 'DBデータ検証',
    message: 'SQLiteに保存された作品データの整合性を検証しますか？',
    confirmText: '検証を実行',
    confirmStyle: 'warning'
  }).then(function (ok) {
    if (!ok) return;

    var requestId = generateRequestId();
    var url = POST_URL + "?repair=" + encodeURIComponent(key);

    var xhr = new XMLHttpRequest();
    xhr.open("POST", url, true);
    xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");
    xhr.onreadystatechange = function () {
      if (xhr.readyState === 4) {
        var res = JSON.parse(xhr.responseText);
        if (xhr.status === 200) showToast(res.message || "データ修復を開始しました", { type: 'success' });
        else showToast(res.message || "データ修復の開始に失敗しました", { type: 'error' });
      }
    };
    xhr.send("repair=" + encodeURIComponent(key) + "&request_id=" + requestId);
  });
}

// === 章構成の動的行管理 ===

// 章構成の行を追加
function addChapterRow() {
  var container = document.getElementById("chapterRows");
  var row = document.createElement("div");
  row.className = "chapter-row";

  row.innerHTML =
    '<input type="number" class="form-input chapter-start" placeholder="開始" min="1">' +
    '<span class="chapter-separator">〜</span>' +
    '<input type="number" class="form-input chapter-end" placeholder="終了" min="1">' +
    '<input type="text" class="form-input chapter-title" placeholder="章タイトル">' +
    '<button type="button" class="btn btn-sm btn-danger chapter-remove" onclick="removeChapterRow(this)">' +
      '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
        '<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>' +
      '</svg>' +
    '</button>';

  container.appendChild(row);
}

// 章構成の行を削除
function removeChapterRow(btn) {
  var row = btn.closest(".chapter-row");
  if (row) row.remove();
}

// 全行から "1-3:タイトルA,4-5:タイトルB" 形式の文字列を組み立て
function assembleChapterString() {
  var rows = document.querySelectorAll("#chapterRows .chapter-row");
  var parts = [];
  for (var i = 0; i < rows.length; i++) {
    var s = rows[i].querySelector(".chapter-start").value.trim();
    var e = rows[i].querySelector(".chapter-end").value.trim();
    var t = rows[i].querySelector(".chapter-title").value.trim();
    if (s && e && t) {
      parts.push(s + "-" + e + ":" + t);
    }
  }
  return parts.join(",");
}

// 章構成行をすべてクリア
function resetChapterRows() {
  var container = document.getElementById("chapterRows");
  container.innerHTML = "";
}

// PDF 送信用
function submitPdfData() {
  var pdfFile = document.getElementById("pdfFile").files[0];
  var authorId = document.getElementById("authorId").value;
  var authorUrl = document.getElementById("authorUrl").value;
  var novelType = document.getElementById("novelType").value;
  var chapter = assembleChapterString();

  if (!pdfFile) {
    showToast("PDFファイルを選択してください。", { type: 'warning' });
    return;
  }
  if (!authorId || !authorUrl) {
    showToast("author_id と author_url を入力してください。", { type: 'warning' });
    return;
  }

  showModal({
    title: 'PDF変換',
    message: '「' + pdfFile.name + '」を変換しますか？',
    confirmText: '変換を実行',
    confirmStyle: 'primary'
  }).then(function (ok) {
    if (!ok) return;

    var formData = new FormData();
    formData.append("pdf", pdfFile);
    formData.append("author_id", authorId);
    formData.append("author_url", authorUrl);
    formData.append("novel_type", novelType);
    formData.append("chapter", chapter);
    formData.append("request_id", generateRequestId());

    var xhr = new XMLHttpRequest();
    xhr.open("POST", POST_URL, true);
    xhr.onreadystatechange = function () {
      if (xhr.readyState === 4) {
        var res = JSON.parse(xhr.responseText);
        if (xhr.status === 200) showToast(res.message || "PDF送信成功", { type: 'success' });
        else showToast(res.message || "PDF送信失敗", { type: 'error' });
      }
    };
    xhr.send(formData);

    // 入力フィールドをクリア
    document.getElementById("pdfFile").value = "";
    document.getElementById("authorId").value = "";
    document.getElementById("authorUrl").value = "";
    document.getElementById("novelType").value = "1";
    resetChapterRows();
    // ファイルラベルをリセット
    var pdfLabel = document.getElementById("pdf-label");
    if (pdfLabel) pdfLabel.querySelector("span").textContent = "PDFファイルを選択";
    // フォーカスを戻す
    document.getElementById("pdfFile").focus();
  });
}

// ZIP 送信用
function submitZipData() {
  var zipFile = document.getElementById("zipFile").files[0];

  if (!zipFile) {
    showToast("ZIPファイルを選択してください。", { type: 'warning' });
    return;
  }

  showModal({
    title: 'ZIP展開',
    message: '「' + zipFile.name + '」を展開しますか？',
    confirmText: '展開を実行',
    confirmStyle: 'primary'
  }).then(function (ok) {
    if (!ok) return;

    var formData = new FormData();
    formData.append("zip", zipFile);
    formData.append("request_id", generateRequestId());

    var xhr = new XMLHttpRequest();
    xhr.open("POST", POST_URL, true);
    xhr.onreadystatechange = function () {
      if (xhr.readyState === 4) {
        var res = JSON.parse(xhr.responseText);
        if (xhr.status === 200) showToast(res.message || "ZIP送信成功", { type: 'success' });
        else showToast(res.message || "ZIP送信失敗", { type: 'error' });
      }
    };
    xhr.send(formData);

    // 入力フィールドをクリア
    document.getElementById("zipFile").value = "";
  });
}
