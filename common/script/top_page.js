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
    title: 'データ修復',
    message: 'データ修復を実行しますか？\n一次ファイルを削除し、全てのraw.jsonからデータを再構築します。',
    confirmText: '修復を実行',
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

// PDF 送信用
function submitPdfData() {
  var pdfFile = document.getElementById("pdfFile").files[0];
  var authorId = document.getElementById("authorId").value;
  var authorUrl = document.getElementById("authorUrl").value;
  var novelType = document.getElementById("novelType").value;
  var chapter = document.getElementById("chapter").value;

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
    document.getElementById("novelType").value = "novel";
    document.getElementById("chapter").value = "";
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
