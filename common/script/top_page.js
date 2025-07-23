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

// ページ上部に色付きメッセージを表示
function showMessage(color, message) {
  // 既存のメッセージを削除
  var existingMessages = document.querySelectorAll(".response-message");
  for (var i = 0; i < existingMessages.length; i++) {
    var msg = existingMessages[i];
    if (msg.parentNode) {
      msg.parentNode.removeChild(msg);
    }
  }

  // メッセージ要素を作成
  var div = document.createElement("div");
  div.style.color = color;
  div.className = "response-message";
  div.textContent = message;
  document.body.appendChild(div);

  // 3秒後に削除
  setTimeout(function () {
    if (div.parentNode) {
      div.parentNode.removeChild(div);
    }
  }, 3000);
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
      if (xhr.status === 200) showMessage("green", res.message || "送信成功");
      else showMessage("red", res.message || "送信失敗");
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
  var requestId = generateRequestId();
  var url = POST_URL + "?update=" + encodeURIComponent(key);

  var xhr = new XMLHttpRequest();
  xhr.open("POST", url, true);
  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");
  xhr.onreadystatechange = function () {
    if (xhr.readyState === 4) {
      var res = JSON.parse(xhr.responseText);
      if (xhr.status === 200) showMessage("green", res.message || "更新成功");
      else showMessage("red", res.message || "更新失敗");
    }
  };
  xhr.send("update=" + encodeURIComponent(key) + "&request_id=" + requestId);
}

// 変換用
function submitConvert(key) {
  var requestId = generateRequestId();
  var url = POST_URL + "?convert=" + encodeURIComponent(key);

  var xhr = new XMLHttpRequest();
  xhr.open("POST", url, true);
  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");
  xhr.onreadystatechange = function () {
    if (xhr.readyState === 4) {
      var res = JSON.parse(xhr.responseText);
      if (xhr.status === 200) showMessage("green", res.message || "変換成功");
      else showMessage("red", res.message || "変換失敗");
    }
  };
  xhr.send("convert=" + encodeURIComponent(key) + "&request_id=" + requestId);
}

// 再ダウンロード用
function submitReDownload(key) {
  var requestId = generateRequestId();
  var url = POST_URL + "?re_download=" + encodeURIComponent(key);

  var xhr = new XMLHttpRequest();
  xhr.open("POST", url, true);
  xhr.setRequestHeader("Content-Type", "application/x-www-form-urlencoded");
  xhr.onreadystatechange = function () {
    if (xhr.readyState === 4) {
      var res = JSON.parse(xhr.responseText);
      if (xhr.status === 200) showMessage("green", res.message || "再ダウンロード成功");
      else showMessage("red", res.message || "再ダウンロード失敗");
    }
  };
  xhr.send("re_download=" + encodeURIComponent(key) + "&request_id=" + requestId);
}

// PDF 送信用
function submitPdfData() {
  var pdfFile = document.getElementById("pdfFile").files[0];
  var authorId = document.getElementById("authorId").value;
  var authorUrl = document.getElementById("authorUrl").value;
  var novelType = document.getElementById("novelType").value;
  var chapter = document.getElementById("chapter").value;

  if (!pdfFile) {
    alert("PDFファイルを選択してください。");
    return;
  }
  if (!authorId || !authorUrl) {
    alert("author_id と author_url を入力してください。");
    return;
  }

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
      if (xhr.status === 200) showMessage("green", res.message || "PDF送信成功");
      else showMessage("red", res.message || "PDF送信失敗");
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
  
}

// ZIP 送信用
function submitZipData() {
  var zipFile = document.getElementById("zipFile").files[0];

  if (!zipFile) {
    alert("ZIPファイルを選択してください。");
    return;
  }

  var formData = new FormData();
  formData.append("zip", zipFile);
  formData.append("request_id", generateRequestId());

  var xhr = new XMLHttpRequest();
  xhr.open("POST", POST_URL, true);
  xhr.onreadystatechange = function () {
    if (xhr.readyState === 4) {
      var res = JSON.parse(xhr.responseText);
      if (xhr.status === 200) showMessage("green", res.message || "ZIP送信成功");
      else showMessage("red", res.message || "ZIP送信失敗");
    }
  };
  xhr.send(formData);

  // 入力フィールドをクリア
  document.getElementById("zipFile").value = "";
  
}
