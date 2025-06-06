// スクリプト冒頭で Map を用意
const coverUrlMap = new Map();
// BlobのSHA-256ハッシュ → ObjectURL の共有マップ
const coverHashMap = new Map();

// Cache Storage 名
const CACHE_NAME = 'cover-images';
//const INDEX_CACHE = 'index-json-cache';

/**
 * 本棚画面用：ヘッダーに「トップページに戻る」ボタンを追加
 */
function initNavOnLibrary() {
  const header = document.querySelector('header');
  if (!header || document.getElementById('btn-nav-home')) return;
  const btn = document.createElement('button');
  btn.id = 'btn-nav-home';
  btn.textContent = 'トップページに戻る';
  btn.style.marginLeft = '1em';  // お好みで微調整
  btn.addEventListener('click', () => {
    window.location.href = '/';    // 必要に応じてルートパスを変更
  });
  header.appendChild(btn);
}

/**
 * リーダー画面用：ヘッダーに「本棚に戻る／目次に戻る」ボタンを追加
 * @param {object} novelData raw.json から読み込んだ小説データ（.serialization を使う）
 * @param {{site:string,nid:string,eid?:string}} query URL パラメータ
 */
function initNavOnReader(novelData, query) {
  const header = document.querySelector('header');
  if (!header || document.getElementById('btn-nav-reader')) return;

  const isShort = novelData.serialization === '短編';
  const hasEid = Boolean(query.eid);

  // --- 目次 or 本文に戻るボタン ---
  const btnBack = document.createElement('button');
  btnBack.id = 'btn-nav-reader';

  let label, href;
  if (isShort || hasEid) {
    // --- 本文表示中 ---
    if (isShort) {
      label = '本棚に戻る';
      href = '/reader/';
    } else {
      label = '目次に戻る';
      href = `?site=${novelData.source}&nid=${novelData.id}`;
    }
  } else {
    // --- 目次表示中 ---
    label = '本棚に戻る';
    href = '/reader/';
  }

  btnBack.textContent = label;
  btnBack.style.marginLeft = '1em';
  btnBack.addEventListener('click', () => {
    window.location.href = href;
  });
  header.appendChild(btnBack);

  // --- 本文へ進むボタン ---
  const h1 = header.querySelector('h1');
  const btnToText = document.createElement('button');
  btnToText.textContent = '本文へ進む';
  btnToText.style.marginLeft = '1em';
  btnToText.addEventListener('click', () => {
    window.location.href = `/${novelData.source}/${novelData.id}/`;
  });
  if (h1) {
    header.insertBefore(btnToText, h1);
  } else {
    header.appendChild(btnToText);
  }
}

/**
 * 同時接続数を制限しつつ preloadAndMapCover を呼び出す
 * @param {Array}  novelsList     - 小説オブジェクトの配列
 * @param {Cache}  coverCache     - Cache Storage オブジェクト
 * @param {Function} onComplete   - 1 件ダウンロード完了ごとに呼ぶコールバック
 * @param {number} concurrencyLimit - 同時に処理する coverDL の上限
 */
async function preloadAllCoversWithLimit(novelsList, coverCache, onComplete, concurrencyLimit) {
  let i = 0;
  const executing = [];

  async function enqueue() {
    if (i === novelsList.length) {
      return Promise.resolve();
    }

    const novel = novelsList[i];
    const taskPromise = preloadAndMapCover(novel, coverCache)
      .then(() => {
        executing.splice(executing.indexOf(taskPromise), 1);
        onComplete();
      })
      .catch(() => {
        executing.splice(executing.indexOf(taskPromise), 1);
        onComplete();
      });

    executing.push(taskPromise);
    i++;

    let next = Promise.resolve();
    if (executing.length >= concurrencyLimit) {
      // いずれかのタスクが終わるまで待つ
      next = Promise.race(executing);
    }
    return next.then(enqueue);
  }

  await enqueue();
}



document.addEventListener('DOMContentLoaded', async () => {
  const query = getQueryParams();

  // ──────────── 「同時ダウンロード数」入力フィールドの生成 ────────────
  // header 要素を取得
  const header = document.querySelector('header');
  // wrapper <div> を作成し、横並び (inline-block) に設定
  const wrapper = document.createElement('div');
  wrapper.style.display = 'inline-block';
  wrapper.style.marginRight = '1em';
  wrapper.style.fontSize = '0.9em';

  // ラベルを作成
  const label = document.createElement('label');
  label.htmlFor = 'concurrency-input';
  label.textContent = '同時ダウンロード数：';

  // 数値入力フィールドを作成
  const input = document.createElement('input');
  input.id = 'concurrency-input';
  input.type = 'number';
  input.min = '1';
  input.max = '10';
  input.step = '1';
  input.style.width = '3em';
  input.style.marginLeft = '0.5em';

  // localStorage から前回値を復元（なければ「4」を初期値に）
  const stored = parseInt(localStorage.getItem('concurrencyLimit'), 10);
  if (!Number.isInteger(stored) || stored < 1) {
    input.value = '4';
  } else {
    input.value = String(stored);
  }

  // 値が変わったら即座に localStorage に保存
  input.addEventListener('change', () => {
    let v = parseInt(input.value, 10);
    if (!Number.isInteger(v) || v < 1) v = 1;
    else if (v > 10) v = 10;
    input.value = String(v);
    localStorage.setItem('concurrencyLimit', String(v));
  });

  wrapper.appendChild(label);
  wrapper.appendChild(input);

  // ヘッダー内の最初の <button>（通常はキャッシュクリア）がある場所を取得
  const h1 = header.querySelector('h1');
  if (h1) {
    header.insertBefore(wrapper, h1);
  } else {
    header.appendChild(wrapper);
  }
  // ──────────────────────────────────────────────────────────

  // ───── 「キャッシュクリア」ボタンのクリック処理 ─────
  const btnClear = document.getElementById('btn-clear-cache');
  btnClear.addEventListener('click', async () => {
    // (A) Cover 画像キャッシュを削除
    await caches.delete(CACHE_NAME);

    // (B) index.json 用の Cache Storage も削除
    await caches.delete(INDEX_CACHE);

    // (C) localStorage 中の coverFail_* をすべて削除
    Object.keys(localStorage).forEach(key => {
      if (key.startsWith('coverFail_')) {
        localStorage.removeItem(key);
      }
    });

    // (D) localStorage 中の indexETag_* をすべて削除
    Object.keys(localStorage).forEach(key => {
      if (key.startsWith(INDEX_ETAG_KEY_PREFIX)) {
        localStorage.removeItem(key);
      }
    });

    // (E) メモリ上の Map もクリア
    coverUrlMap.clear();
    coverHashMap.clear();

    alert('キャッシュをクリアしました。ページを再読み込みします。');
    window.location.reload();
  });
  // ────────────────────────────────────────────────────────

  // 「リーダー画面かどうか」を判定し、リーダーならそちらの処理へ
  if (query.site && query.nid) {
    initWidthSelector();
    const pcReader = document.getElementById('progress-container');
    if (pcReader) pcReader.style.display = 'none';
    await renderReaderScreen(query);
    return;
  }

  // ─── ライブラリ画面フロー ───
  initNavOnLibrary();
  const app = document.getElementById('app');
  const pc = document.getElementById('progress-container');
  const pb = document.getElementById('progress-bar');
  const pi = document.getElementById('progress-info');

  //
  // (1) index.json 取得フェーズ
  //
  if (pc) pc.style.display = 'block';
  let completedIndex = 0;
  const totalIndex = sources.length;

  function updateIndexBar() {
    const pct = totalIndex > 0 ? (completedIndex / totalIndex * 100) : 0;
    pb.style.width = pct + '%';
    pi.textContent = `index.json を取得中: ${pct.toFixed(2)}% (${completedIndex}/${totalIndex})`;
  }
  updateIndexBar();

  const indexPromises = sources.map(async (src) => {
    const arr = await loadIndexWithCache(src);
    completedIndex++;
    updateIndexBar();
    return arr.map(item => ({ ...item, source: src }));
  });
  const allArrays = await Promise.all(indexPromises);

  // novelsList にまとめる
  const novelsList = [];
  allArrays.forEach(arr => novelsList.push(...arr));

  //
  // (2) 目次構築中フェーズ
  //
  // index.json 取得が終わったので、一度バーを 100% にして短時間表示
  pi.textContent = '目次を読み込み中…';
  pb.style.width = '100%';
  await new Promise(r => setTimeout(r, 200));

  //
  // (3) カバーキャッシュチェック → キャッシュ済みは coverUrlMap に登録、未キャッシュは needFetchList へ
  //
  const coverCache = await caches.open(CACHE_NAME);
  const needFetchList = [];

  for (const novel of novelsList) {
    const key = `${novel.source}_${novel.id}`;
    const base = `../${novel.source}/${novel.id}/`;
    let foundInCache = false;

    // 「jpg → png → gif」の順でキャッシュを探し、見つかれば ObjectURL を生成して coverUrlMap に登録
    for (const ext of ['jpg', 'png', 'gif']) {
      const url = base + `cover.${ext}`;
      const cachedRs = await coverCache.match(url);
      if (cachedRs) {
        const blob = await cachedRs.blob();
        const objectURL = await dedupeBlob(blob);
        coverUrlMap.set(key, objectURL);
        foundInCache = true;
        break;
      }
    }

    // キャッシュが見つからなかった場合だけリストに追加
    if (!foundInCache) {
      needFetchList.push(novel);
    }
  }

  // (4a) キャッシュミスがゼロなら、バーを隠して一度だけ目次を描画して終了
  if (needFetchList.length === 0) {
    if (pc) pc.style.display = 'none';
    await renderLibraryWithProgress(app, novelsList);
    return;
  }

  //
  // (4b) キャッシュミス分のカバーDLフェーズ
  //
  let completedCover = 0;
  const totalCover = needFetchList.length;

  function updateCoverBar() {
    const pct = totalCover > 0 ? (completedCover / totalCover * 100) : 0;
    pb.style.width = pct + '%';
    pi.textContent = `カバーをダウンロード中: ${pct.toFixed(2)}% (${completedCover}/${totalCover})`;
  }
  // フェーズ開始時にバーと文字を初期化
  completedCover = 0;
  updateCoverBar();

  // まず localStorage に保存された同時ダウンロード数を取得
  let concurrencyLimit = parseInt(localStorage.getItem('concurrencyLimit'), 10);

  // localStorage に正しい値が入っていなければ、input.value を参照
  if (!Number.isInteger(concurrencyLimit) || concurrencyLimit < 1) {
    concurrencyLimit = parseInt(input.value, 10);
    if (!Number.isInteger(concurrencyLimit) || concurrencyLimit < 1) {
      concurrencyLimit = 1;
    }
  }

  // 上限を設ける（例：最大 10）
  if (concurrencyLimit > 10) {
    concurrencyLimit = 10;
  }

  // ここで preloadAllCoversWithLimit に渡す
  await preloadAllCoversWithLimit(
    needFetchList,
    coverCache,
    () => {
      completedCover++;
      updateCoverBar();
    },
    concurrencyLimit
  );

  // (5) カバーDL完了後、バーを隠して最終的に目次を描画
  if (pc) pc.style.display = 'none';
  await renderLibraryWithProgress(app, novelsList);
});


// index.json のソースリスト
const INDEX_CACHE = 'index-json-cache';              // Cache Storage 名
const INDEX_ETAG_KEY_PREFIX = 'indexETag_';          // localStorage に ETag を保存する際のキー接頭辞

/**
 * キャッシュ付き index.json の読み込み（条件付き GET 版）
 * ※ 「キャッシュが無いのに 304 が返ってくる」ケースを回避する
 * @param {string} source - サイト名またはディレクトリ名
 * @returns {Promise<Array>} - [{ id, source, ...novelObject }, …] の配列
 */
async function loadIndexWithCache(source) {
  const url = new URL(`../${source}/index.json`, location.href).toString();
  const cache = await caches.open(INDEX_CACHE);
  const etagKey = `${INDEX_ETAG_KEY_PREFIX}${source}`;
  let storedEtag = localStorage.getItem(etagKey);

  // 1) Cache Storage からキャッシュ済みレスポンスを取得
  const cachedResp = await cache.match(url);
  let cachedObj = null;
  if (cachedResp) {
    try {
      cachedObj = await cachedResp.clone().json(); // オブジェクト形式で取得
    } catch {
      cachedObj = null;
    }
  }

  // 2) 条件付き GET 用ヘッダーを準備
  const headers = {};
  // 「キャッシュ版（cachedObj）が存在するときだけ ETag を使う」
  if (storedEtag && cachedObj) {
    headers['If-None-Match'] = storedEtag;
  } else {
    // キャッシュが無いのに ETag が残っている→破棄して再取得させる
    localStorage.removeItem(etagKey);
    storedEtag = null;
  }

  // 3) サーバーへ GET 要請（If-None-Match を付与するかどうか）
  let resp;
  try {
    resp = await fetch(url, {
      method: 'GET',
      headers: headers,
      cache: 'no-store'
    });
  } catch (e) {
    // ネットワークエラーなどで失敗したら、キャッシュ版があればそれを返す
    if (cachedObj) {
      return Object.entries(cachedObj).map(([id, novel]) => ({ id, source, ...novel }));
    }
    throw e;
  }

  // 4) 304 Not Modified：キャッシュが最新なのでキャッシュデータを返す
  if (resp.status === 304 && cachedObj) {
    return Object.entries(cachedObj).map(([id, novel]) => ({ id, source, ...novel }));
  }

  // 5) 200 OK：更新があった → JSON をパースしてキャッシュを更新
  if (resp.status === 200) {
    let freshObj;
    try {
      freshObj = await resp.clone().json(); // オブジェクト形式で取得
    } catch (e) {
      // JSON パースエラーでもキャッシュ版があればそれを返す
      if (cachedObj) {
        return Object.entries(cachedObj).map(([id, novel]) => ({ id, source, ...novel }));
      }
      throw e;
    }

    // Cache Storage に最新の index.json を保存
    await cache.put(url, resp.clone());

    // サーバーから返ってきた新しい ETag（または Last-Modified）を localStorage に保存
    const newEtag = resp.headers.get('ETag') || resp.headers.get('Last-Modified');
    if (newEtag) {
      localStorage.setItem(etagKey, newEtag);
    }

    // オブジェクトを配列に変換して返す
    return Object.entries(freshObj).map(([id, novel]) => ({ id, source, ...novel }));
  }

  // 6) その他ステータス（404, 500, 304＋キャッシュ無し など）は、
  //    キャッシュ版があればそれを返し、無ければ例外
  if (cachedObj) {
    return Object.entries(cachedObj).map(([id, novel]) => ({ id, source, ...novel }));
  }
  throw new Error(`index.json fetch failed: ${resp.status}`);
}


async function loadJSON(path) {
  const response = await fetch(path);
  return await response.json();
}

/**
 * 低解像度化＋重複チェック＋失敗回数管理＋
 * default_cover.png も Blob→ObjectURL で利用するプリロード関数
 */
async function preloadAndMapCover(novel, coverCache) {
  const key = `${novel.source}_${novel.id}`;
  const base = `../${novel.source}/${novel.id}/`;
  // 絶対パスで指定
  const defaultUrl = `${window.location.origin}/images/default_cover.png`;

  // ローカルストレージで失敗回数を管理
  const failKey = `coverFail_${novel.source}_${novel.id}`;
  let failCount = parseInt(localStorage.getItem(failKey)) || 0;

  // 3回以上失敗していれば即デフォルトを返す
  if (failCount >= 3) {
    coverUrlMap.set(key, await getDefaultCoverObjectURL(coverCache, defaultUrl));
    return;
  }

  // 1) キャッシュ済みの cover.jpg/png/gif を探す
  for (const ext of ['jpg', 'png', 'gif']) {
    const url = base + `cover.${ext}`;
    const cachedResp = await coverCache.match(url);
    if (cachedResp) {
      const blob = await cachedResp.blob();
      coverUrlMap.set(key, await dedupeBlob(blob));
      return;
    }
  }

  // 2) HEAD → fetch → 低解像度化 → キャッシュ登録
  for (const ext of ['jpg', 'png', 'gif']) {
    const url = base + `cover.${ext}`;
    try {
      const head = await fetch(url, { method: 'HEAD' });
      if (!head.ok) continue;

      const origBlob = await (await fetch(url)).blob();
      const smallBlob = await shrinkBlob(origBlob, 400, 0.75);
      const objectURL = await dedupeBlob(smallBlob);

      // キャッシュにも登録
      await coverCache.put(url, new Response(smallBlob));
      coverUrlMap.set(key, objectURL);
      return;
    } catch {
      // 次の拡張子へ
    }
  }

  // 3) 全滅 → 失敗カウントアップ＆デフォルトを返す
  failCount++;
  localStorage.setItem(failKey, String(failCount));
  coverUrlMap.set(key, await getDefaultCoverObjectURL(coverCache, defaultUrl));
}

/** 
 * default_cover.png を一度 Cache Storage に ensure → blob→ObjectURL を返す
 */
async function getDefaultCoverObjectURL(coverCache, defaultUrl) {
  // 絶対URL の Request オブジェクトを生成しておく
  const req = new Request(defaultUrl, { method: 'GET' });

  // キャッシュに無ければ fetch＆put
  let cachedDef = await coverCache.match(req);
  if (!cachedDef) {
    const resp = await fetch(req);
    if (!resp.ok) {
      console.error('default cover fetch failed:', resp.status, defaultUrl);
      throw new Error('default cover not found');
    }
    await coverCache.put(req, resp.clone());
    cachedDef = resp;
  }

  // blob → ObjectURL
  const blob = await (await coverCache.match(req)).blob();
  return URL.createObjectURL(blob);
}

function getReadStatus(novel) {
  // episodes_data があればそちらを優先、なければ episodes を利用
  const episodes = novel.episodes_data || novel.episodes || {};
  const keys = Object.keys(episodes);
  if (keys.length === 0) {
    return 'unread';
  }

  // 完読済みエピソード一覧を取得
  const epDoneKey = `epFinished_${novel.source}_${novel.id}`;
  const doneSet = new Set(JSON.parse(localStorage.getItem(epDoneKey) || '[]'));

  let anyProgress = false;
  let finishedCount = 0;

  // 各エピソードについて判定
  for (const key of keys) {
    // episodes_data[key].id を実際のエピソードIDとして利用
    const epId = episodes[key].id;

    // 完読済みであればカウント
    if (doneSet.has(String(epId))) {
      finishedCount++;
      continue;
    }

    // 途中既読判定用のキー
    const readKey = `readPage_${novel.source}_${novel.id}_${epId}`;
    if (localStorage.getItem(readKey)) {
      anyProgress = true;
    }
  }

  // すべて完読済み
  if (finishedCount === keys.length) {
    return 'finished';
  }
  // まったく進捗がない
  if (!anyProgress && finishedCount === 0) {
    return 'unread';
  }
  // それ以外は途中まで既読
  return 'read';
}

/** 
 * Blob を canvas で縮小して JPEG Blob にするユーティリティ 
 */
async function shrinkBlob(blob, maxWidth, quality) {
  const img = await new Promise(res => {
    const i = new Image();
    i.onload = () => res(i);
    i.src = URL.createObjectURL(blob);
  });
  const scale = Math.min(1, maxWidth / img.width);
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(img.width * scale);
  canvas.height = Math.round(img.height * scale);
  const ctx = canvas.getContext('2d');
  ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
  return await new Promise(r => canvas.toBlob(r, 'image/jpeg', quality));
}

/**
 * Blob を SHA-256 でハッシュし、重複排除＆ObjectURL管理を行うユーティリティ
 */
async function dedupeBlob(blob) {
  const buf = await blob.arrayBuffer();
  const hashBuf = await crypto.subtle.digest('SHA-256', buf);
  const hashArr = Array.from(new Uint8Array(hashBuf));
  const hashHex = hashArr.map(b => b.toString(16).padStart(2, '0')).join('');
  if (coverHashMap.has(hashHex)) {
    return coverHashMap.get(hashHex);
  }
  const url = URL.createObjectURL(blob);
  coverHashMap.set(hashHex, url);
  return url;
}

/**
 * プログレスバーと文字表示を更新する共通関数
 * @param {number} doneCount    - 現在完了している件数
 * @param {number} totalCount   - 全体の件数
 * @param {string} messageLabel - 「○○中:」などの先頭メッセージ
 */
function updateProgress(doneCount, totalCount, messageLabel) {
  const pc = document.getElementById('progress-container');
  const pb = document.getElementById('progress-bar');
  const pi = document.getElementById('progress-info');
  if (!pc || !pb || !pi) return;

  pc.style.display = 'block';
  const pct = totalCount > 0 ? (doneCount / totalCount * 100) : 0;
  pb.style.width = pct + '%';
  pi.textContent = `${messageLabel} ${pct.toFixed(2)}% (${doneCount}/${totalCount})`;
}


/**
 * 進捗表示付きでライブラリ（目次）をレンダリングする
 * @param {HTMLElement} container  - #app のような、小説カードを入れる要素
 * @param {Array} novelsList      - 小説オブジェクトの配列
 */
async function renderLibraryWithProgress(container, novelsList) {
  container.innerHTML = '';

  // LocalStorage から折りたたみ状態を取得
  const collapsed = JSON.parse(localStorage.getItem('collapsedAuthors') || '[]');
  const now = new Date();

  // 作者でグループ化
  const authorsMap = {};
  novelsList.forEach(novel => {
    if (!authorsMap[novel.author]) authorsMap[novel.author] = [];
    authorsMap[novel.author].push(novel);
  });

  // 作者内で更新日時順（新しい順）に並べ替え
  Object.values(authorsMap).forEach(list => {
    list.sort((a, b) => new Date(b.update_date) - new Date(a.update_date));
  });

  // 作者名順にソート（日本語ロケール）
  const sortedAuthors = Object.keys(authorsMap).sort((a, b) => a.localeCompare(b, 'ja'));

  // ここから「目次を読み込み中」フェーズ
  const totalItems = novelsList.length;  // 全体の小説数を総数とする
  let doneItems = 0;
  updateProgress(doneItems, totalItems, '目次を読み込み中:');

  // 小説カードを作る前に、暫定的に progress bar を表示しつつ、少しだけ待機してユーザーが見やすいようにする
  await new Promise(r => setTimeout(r, 50));

  for (const author of sortedAuthors) {
    // 作者グループ全体のコンテナを作成
    const group = document.createElement('div');
    group.className = 'author-group';

    // 【ヘッダー部分】
    const header = document.createElement('div');
    header.className = 'author-header';
    header.textContent = author;

    // NEW バッジ（7日以内に更新があれば）
    const hasNew = authorsMap[author].some(novel =>
      (now - new Date(novel.update_date)) / (1000 * 3600 * 24) <= 7
    );
    if (hasNew) {
      const tag = document.createElement('span');
      tag.className = 'new-tag';
      tag.textContent = 'NEW';
      tag.style.marginLeft = '0.5em';
      header.appendChild(tag);
    }

    // 折りたたみ状態復元
    const content = document.createElement('div');
    content.className = 'author-content';
    content.style.display = collapsed.includes(author) ? 'none' : 'block';

    header.addEventListener('click', () => {
      const isNow = content.style.display === 'block';
      content.style.display = isNow ? 'none' : 'block';
      const idx = collapsed.indexOf(author);
      if (isNow) {
        if (idx === -1) collapsed.push(author);
      } else {
        if (idx !== -1) collapsed.splice(idx, 1);
      }
      localStorage.setItem('collapsedAuthors', JSON.stringify(collapsed));
    });

    // カードを１つずつ作成
    for (const novel of authorsMap[author]) {
      const novelElem = document.createElement('div');
      novelElem.className = 'novel';

      // カバー画像
      const covCont = document.createElement('div');
      covCont.className = 'cover-container';
      const img = document.createElement('img');
      const key = `${novel.source}_${novel.id}`;
      img.src = coverUrlMap.get(key) || '/images/default_cover.png';
      covCont.appendChild(img);

      // バッジ表示
      const badgeWrap = document.createElement('div');
      badgeWrap.className = 'cover-badges';

      const days = (now - new Date(novel.update_date)) / (1000 * 3600 * 24);
      const readStatus = getReadStatus(novel);
      badgeWrap.appendChild(makeBadge('NEW', days <= 7));
      const statusLabel = readStatus === 'unread' ? '未読'
        : readStatus === 'read' ? '既読'
          : '完読';
      badgeWrap.appendChild(makeBadge(statusLabel));
      badgeWrap.appendChild(makeBadge(novel.serialization));
      badgeWrap.appendChild(makeBadge(novel.type === 'comic' ? '漫画' : '小説'));
      covCont.appendChild(badgeWrap);

      covCont.addEventListener('click', () => {
        window.location.href = createNovelURL(novel);
      });
      novelElem.appendChild(covCont);

      // タイトル部分
      const title = document.createElement('div');
      title.className = 'novel-title';
      const titleText = document.createElement('div');
      titleText.className = 'title-text';
      titleText.textContent = novel.title;
      title.appendChild(titleText);
      novelElem.appendChild(title);

      content.appendChild(novelElem);

      // １件レンダリングごとに進捗更新
      doneItems++;
      updateProgress(doneItems, totalItems, '目次を読み込み中:');
      // ほんの数ミリ秒ウェイトを入れると表示がスムーズになることがあります（必要なければ省略可）
      if (doneItems % 100 === 0) {
        // 100 件ごとに少しだけ待つ（UI 更新が詰まるのを防ぐため）
        await new Promise(r => setTimeout(r, 10));
      }
    }

    group.appendChild(header);
    group.appendChild(content);
    container.appendChild(group);
  }

  // レンダリング完了したらプログレスバーを非表示に
  const pcEl = document.getElementById('progress-container');
  if (pcEl) pcEl.style.display = 'none';
}

function makeBadge(label, active = true) {
  const span = document.createElement('span');
  span.className = 'cover-badge';
  span.dataset.type = label;
  if (!active) span.classList.add('disabled-badge');
  span.textContent = label;
  return span;
}


// URL生成関数を追加
function createNovelURL(novel, episodeId = null) {
  let url = `?site=${novel.source}&nid=${novel.id}`;

  if (novel.serialization !== "短編") {
    if (episodeId) {
      url += `&eid=${episodeId}`; // 個別エピソードのURL
    } // エピソード指定なしなら目次
  }
  // 短編なら nid だけで即本文表示

  return url;
}

// カバー探索 (既存の findCoverImage を流用) 
async function resolveCoverUrl(novel) {
  const key = `${novel.source}_${novel.id}`;

  // まずメモリ上 Map にあればそれを返す
  if (coverUrlMap.has(key)) {
    return coverUrlMap.get(key);
  }

  // 無ければ HEAD で確認（最初の一回だけ）
  const base = `../${novel.source}/${novel.id}/`;
  for (const ext of ['jpg', 'png', 'gif']) {
    const url = base + `cover.${ext}`;
    try {
      const res = await fetch(url, { method: 'HEAD' });
      if (res.ok) {
        coverUrlMap.set(key, url);
        return url;
      }
    } catch { }
  }

  // デフォルト
  const def = '../images/default_cover.png';
  coverUrlMap.set(key, def);
  return def;
}

// URLのクエリを解析

function getQueryParams() {
  const p = new URLSearchParams(window.location.search);
  return {
    site: p.get('site'),
    nid: p.get('nid'),
    eid: p.get('eid'),
    page: (() => {
      const raw = p.get('page');
      const n = parseInt(raw, 10);
      return Number.isInteger(n) ? n : undefined;
    })()
  };
}



async function renderReaderScreen(query) {
  const app = document.getElementById('app');
  document.body.classList.add('reader');
  app.innerHTML = '<div class="loading">読み込み中です……</div>';

  // raw.json 読み込み
  const novelPath = `../${query.site}/${query.nid}/raw/raw.json`;
  const novelData = await loadJSON(novelPath);
  if (!novelData) {
    app.innerHTML = '<div>小説データが見つかりません。</div>';
    return;
  }

  // site・id を保持
  novelData.source = query.site;
  novelData.id = query.nid;

  // ★ ヘッダーに戻るボタンを追加
  initNavOnReader(novelData, query);

  // episodes 情報取得
  const episodesObj = novelData.episodes || novelData.episodes_data;
  if (!episodesObj || !Object.keys(episodesObj).length) {
    app.innerHTML = '<div>エピソードが見つかりません。</div>';
    return;
  }
  const episodesArr = Object.values(episodesObj);

  // 短編か eid 指定か
  const isShort = novelData.serialization === '短編';
  const hasEpisodeId = Boolean(query.eid);

  if (isShort || hasEpisodeId) {
    // 本文表示パス
    let episode;
    if (hasEpisodeId) {
      // キー or id プロパティで検索
      episode = episodesObj[query.eid]
        || episodesArr.find(ep => String(ep.id) === String(query.eid));
    } else {
      episode = episodesArr[0];
      // ★ 短編エピソードにIDがない場合に補う
      if (!episode.id) {
        episode.id = 1; // ← novel.id をエピソードIDとして扱う
        console.log('[短編補正] episode.id が未定義だったので補正:', episode.id)
        novel.episodes = { [novel.id]: episode }; // ← key 付きに整える
      }
    }


    if (!episode) {
      app.innerHTML = '<div>指定されたエピソードが見つかりません。</div>';
      return;
    }

    // ここで episodesArr も渡す
    renderEpisode(app, novelData, episode, episodesArr);

  } else {
    // 目次表示パス
    renderToc(app, novelData, query);
  }
}



async function renderEpisode(container, novel, episode, episodesArr) {
  // コンテナをクリア
  container.innerHTML = '';
  document.body.classList.add('reader');

  // タイトル
  const h2 = document.createElement('h2');
  h2.textContent = episode.title;
  container.appendChild(h2);

  // ページ分割
  const pageTexts = episode.text.split(/\[newpage\]/);
  const totalPages = pageTexts.length;

  // 現在のページを取得
  const query = getQueryParams();
  const rawId = query.nid;           // URL の nid
  const epId = episode.id;          // episodes_data 内の実 ID
  const storageKey = `readPage_${novel.source}_${rawId}_${epId}`;

  let currentPage;
  const hasExplicit = typeof query.page === 'number';
  if (hasExplicit) {
    currentPage = query.page;
  } else {
    const saved = parseInt(localStorage.getItem(storageKey), 10);
    currentPage = Number.isInteger(saved) && saved >= 1 ? saved : 1;
    if (currentPage > 1) {
      // URL を置き換えてリダイレクト
      const url = createEpisodeURL(novel, epId, currentPage, totalPages);
      window.location.replace(url);
      return;
    }
  }

  // 範囲補正
  currentPage = Math.min(Math.max(currentPage, 1), totalPages);

  // ★ 短編も含め、最終ページ到達時のみ完読扱いに
  if (currentPage === totalPages) {
    // ページキーは消して
    localStorage.removeItem(storageKey);

    // 完読配列に追加
    const epDoneKey = `epFinished_${novel.source}_${rawId}`;
    const doneArr = JSON.parse(localStorage.getItem(epDoneKey) || '[]');
    if (!doneArr.includes(String(epId))) {
      doneArr.push(String(epId));
      localStorage.setItem(epDoneKey, JSON.stringify(doneArr));
    }
  } else {
    // 最終ページでなければ進捗を保存
    localStorage.setItem(storageKey, String(currentPage));
  }

  // 前書き（1ページ目のみ）
  if (currentPage === 1 && episode.introduction) {
    const intro = document.createElement('div');
    intro.className = 'introduction';
    intro.innerHTML = formatText(episode.introduction, novel, episode);
    container.appendChild(intro);
  }

  // 本文
  const raw = pageTexts[currentPage - 1] || '';
  const main = document.createElement('div');
  main.className = 'main-text';
  // ★ innerHTML で挿入することで <img> タグなどを正しく解釈
  main.innerHTML = formatText(raw, novel, episode);
  container.appendChild(main);

  // あとがき（最終ページのみ）
  if (currentPage === totalPages && episode.postscript) {
    const post = document.createElement('div');
    post.className = 'postscript';
    post.innerHTML = formatText(episode.postscript, novel, episode);
    container.appendChild(post);
  }

  // ページ保存 or 完了処理
  if (currentPage < totalPages) {
    // 途中ページを保存
    localStorage.setItem(storageKey, String(currentPage));
  } else {
    // 最終ページ到達 → 途中保存を削除
    localStorage.removeItem(storageKey);

    // 完読一覧に追加
    const epDoneKey = `epFinished_${novel.source}_${rawId}`;
    const doneArr = JSON.parse(localStorage.getItem(epDoneKey) || '[]');
    if (!doneArr.includes(String(epId))) {
      doneArr.push(String(epId));
      localStorage.setItem(epDoneKey, JSON.stringify(doneArr));
    }
  }

  // --- ページナビゲーション ---
  const nav = document.createElement('div');
  nav.className = 'page-nav';

  if (currentPage > 1) {
    const prev = document.createElement('a');
    prev.className = 'nav-link';
    prev.href = createEpisodeURL(novel, epId, currentPage - 1, totalPages);
    prev.textContent = '← 前のページ';
    nav.appendChild(prev);
  } else {
    // クリック不可・表示だけのダミー
    const prev = document.createElement('span');
    prev.className = 'nav-link nav-link--disabled';
    prev.textContent = '← 前のページ';
    prev.style.pointerEvents = 'none';
    prev.style.opacity = '0.6';
    nav.appendChild(prev);
  }

  const selector = document.createElement('select');
  selector.className = 'page-select';
  for (let i = 1; i <= totalPages; i++) {
    const opt = document.createElement('option');
    opt.value = i;
    opt.textContent = `${i}ページ目`;
    if (i === currentPage) opt.selected = true;
    selector.appendChild(opt);
  }
  selector.addEventListener('change', () => {
    const sel = parseInt(selector.value, 10);
    window.location.href = createEpisodeURL(novel, epId, sel, totalPages);
  });
  nav.appendChild(selector);

  if (currentPage < totalPages) {
    const next = document.createElement('a');
    next.className = 'nav-link';
    next.href = createEpisodeURL(novel, epId, currentPage + 1, totalPages);
    next.textContent = '次のページ →';
    nav.appendChild(next);
  } else {
    // 非活性表示・クリック不可でレイアウト維持
    const next = document.createElement('span');
    next.className = 'nav-link nav-link--disabled';
    next.textContent = '次のページ →';
    next.style.pointerEvents = 'none';
    next.style.opacity = '0.6';
    nav.appendChild(next);
  }


  container.appendChild(nav);

  // --- エピソード間ナビ ---
  const epNav = document.createElement('div');
  epNav.className = 'episode-nav';

  const idx = episodesArr.findIndex(ep => String(ep.id) === String(epId));

  // ← 前の話へ
  const prevA = document.createElement('a');
  prevA.className = 'nav-link';
  prevA.textContent = '← 前の話へ';
  if (idx > 0) {
    const prevEp = episodesArr[idx - 1];
    prevA.href = createEpisodeURL(novel, prevEp.id, 1);
  } else {
    prevA.classList.add('nav-link--disabled');
  }
  epNav.appendChild(prevA);

  // 戻る
  const back = document.createElement('a');
  back.className = 'nav-link';
  if (novel.serialization === '短編') {
    back.href = '/reader/';
    back.textContent = '本棚に戻る';
  } else {
    back.href = `?site=${novel.source}&nid=${rawId}`;
    back.textContent = '目次に戻る';
  }
  epNav.appendChild(back);

  // 次の話へ →
  const nextA = document.createElement('a');
  nextA.className = 'nav-link';
  nextA.textContent = '次の話へ →';
  if (idx < episodesArr.length - 1) {
    const nextEp = episodesArr[idx + 1];
    nextA.href = createEpisodeURL(novel, nextEp.id, 1);
  } else {
    nextA.classList.add('nav-link--disabled');
  }
  epNav.appendChild(nextA);

  container.appendChild(epNav)

  // 画像サイズ調整
  requestAnimationFrame(adjustImages);
}


function createEpisodeURL(novel, episodeId = null, page = 1, totalPages) {
  let url = `?site=${novel.source}&nid=${novel.id}`;
  if (novel.serialization !== '短編' && episodeId) {
    url += `&eid=${episodeId}`;
  }

  // totalPages が undefined のときは常に付ける（安全のため）
  if (typeof totalPages === 'undefined' || totalPages > 1) {
    url += `&page=${page}`;
  }

  return url;
}




/** 単純な debounce ユーティリティ */
function debounce(fn, ms) {
  let t;
  return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); };
}


// 改行やタグの処理（必要に応じて追加）
function formatText(text, novel = null, episode = null) {
  return text
    // ルビ変換
    .replace(/\[ruby:<([^>]+)>\(([^)]+)\)\]/g, (_, rb, rt) =>
      `<ruby><rb>${rb}</rb><rt>${rt}</rt></ruby>`
    )

    // ページジャンプ
    .replace(/\[jump:(\d+)\]/g, (_, page) => {
      if (!novel || !episode) return `[jump:${page}]`;
      const url = createEpisodeURL(novel, episode.id, Number(page));
      return `<a href="${url}" class="page-jump">${page}ページ目へ移動</a>`;
    })

    // 画像表示
    .replace(/\[image\]\(([^)]+)\)/g, (_, filename) =>
      `<img src="/images/${filename}" class="inline-image" alt="">`
    )

    // 改行
    .replace(/\n/g, '<br>');
}



/**
 * 目次を更新日時付きで表示するように変更
 */
function renderToc(container, novel, query) {
  // 1) 全エピソード取得
  const episodes = Object.values(novel.episodes || novel.episodes_data);

  // 2) 章ごとにグループ化
  const groups = {};
  const order = [];
  episodes.forEach(ep => {
    const chap = ep.chapter || '__noChapter';
    if (!(chap in groups)) {
      groups[chap] = [];
      order.push(chap);
    }
    groups[chap].push(ep);
  });

  // 3) 折りたたみ状態を取得
  const storageKey = `tocCollapsed_${novel.source}_${novel.id} `;
  const collapsed = JSON.parse(localStorage.getItem(storageKey) || '[]');

  // 4) DOM生成
  const toc = document.createElement('div');
  toc.className = 'toc-container';

  const title = document.createElement('h2');
  title.textContent = `${novel.title} - 目次`;
  toc.appendChild(title);

  order.forEach(chap => {
    const eps = groups[chap];
    // 「章あり」グループ
    if (chap !== '__noChapter') {
      // 章ヘッダー
      const header = document.createElement('div');
      header.className = 'toc-chapter-header';
      header.textContent = chap;
      header.style.cursor = 'pointer';

      // 折りたたみマーカー
      const marker = document.createElement('span');
      const isCollapsed = collapsed.includes(chap);
      marker.textContent = isCollapsed ? ' [+]' : ' [-]';
      header.appendChild(marker);

      // クリックで toggle
      header.addEventListener('click', () => {
        const idx = collapsed.indexOf(chap);
        if (idx === -1) collapsed.push(chap);
        else collapsed.splice(idx, 1);
        localStorage.setItem(storageKey, JSON.stringify(collapsed));
        list.style.display = idx === -1 ? 'none' : 'block';
        marker.textContent = idx === -1 ? ' [+]' : ' [-]';
      });

      toc.appendChild(header);

      // エピソード一覧
      const list = document.createElement('ul');
      list.style.display = isCollapsed ? 'none' : 'block';
      eps.forEach(ep => {
        const li = document.createElement('li');
        const a = document.createElement('a');
        a.href = `?site=${query.site}&nid=${query.nid}&eid=${ep.id}`;
        a.textContent = `${ep.title} — ${formatDate(ep.updateDate)} `;
        li.appendChild(a);
        list.appendChild(li);
      });
      toc.appendChild(list);

    } else {
      // 「章なし」エピソードはそのまま
      eps.forEach(ep => {
        const li = document.createElement('li');
        const a = document.createElement('a');
        a.href = `?site=${query.site}&nid=${query.nid}&eid=${ep.id}`;
        a.textContent = `${ep.title} — ${formatDate(ep.updateDate)} `;
        li.appendChild(a);
        toc.appendChild(li);
      });
    }
  });

  // 5) 本棚へのリンク
  const backP = document.createElement('p');
  const backA = document.createElement('a');
  backA.href = '/reader/';
  backA.textContent = '本棚に戻る';
  backP.appendChild(backA);
  toc.appendChild(backP);

  // 6) 既存コンテンツをクリアして差し替え
  container.innerHTML = '';
  container.appendChild(toc);
}

/** 日付を "YYYY/MM/DD hh:mm" に整形 */
function formatDate(dateStr) {
  const d = new Date(dateStr);
  const z = v => String(v).padStart(2, '0');
  return `${d.getFullYear()} /${z(d.getMonth() + 1)}/${z(d.getDate())} `
    + `${z(d.getHours())}:${z(d.getMinutes())} `;
}


/**
* ヘッダーに「幅選択」ドロップダウンを出して、
* localStorage からの復元＆変更時に root の --reader-width を更新
*/
function initWidthSelector() {
  const header = document.querySelector('header');
  if (!header || document.getElementById('reader-width-select')) return;

  const select = document.createElement('select');
  select.id = 'reader-width-select';
  ['55%', '65%', '75%', '85%', '95%', '100%'].forEach(v => {
    const o = document.createElement('option');
    o.value = o.textContent = v;
    select.appendChild(o);
  });

  // localStorage から復元
  const saved = localStorage.getItem('readerWidth') || '65%';
  select.value = saved;
  applyWidth(saved);

  select.addEventListener('change', () => {
    const w = select.value;
    localStorage.setItem('readerWidth', w);
    applyWidth(w);
  });

  header.appendChild(select);
}

function adjustImages() {
  const app = document.getElementById('app');
  if (!app || !document.body.classList.contains('reader')) return;

  const rect = app.getBoundingClientRect();
  const cs = getComputedStyle(app);
  const padL = parseFloat(cs.paddingLeft);
  const padR = parseFloat(cs.paddingRight);
  const maxW = rect.width - padL - padR;
  const maxH = window.innerHeight;

  document.querySelectorAll('body.reader #app img').forEach(img => {
    if (!img.complete || !img.naturalWidth) {
      img.addEventListener('load', () => requestAnimationFrame(adjustImages), { once: true });
      return;
    }

    // リセット＋max制約
    img.style.width = 'auto';
    img.style.height = 'auto';
    img.style.maxWidth = maxW + 'px';
    img.style.maxHeight = maxH + 'px';

    // 必要ならさらにピクセル単位で縮小
    const nw = img.naturalWidth;
    const nh = img.naturalHeight;
    const scale = Math.min(1, maxW / nw, maxH / nh);
    if (scale < 1) {
      img.style.width = Math.floor(nw * scale) + 'px';
      img.style.height = Math.floor(nh * scale) + 'px';
    }

    // 中央寄せ
    img.style.display = 'block';
    img.style.margin = '0 auto';
  });
}

// ページ読み込み後・リサイズ時に実行
window.addEventListener('load', () => requestAnimationFrame(adjustImages));
window.addEventListener('resize', debounce(adjustImages, 100));

function applyWidth(w) {
  const root = document.documentElement;
  // CSS 変数を更新
  root.style.setProperty('--reader-width', w);
  // もともとの max-width 制約は外す
  root.style.setProperty('--reader-max-width', w);

  // #app に直接幅を当てる
  const app = document.getElementById('app');
  if (app) {
    app.style.setProperty('width', w, 'important');
    app.style.setProperty('max-width', w, 'important');
  }

  // 見開き部分にも同様に
  document.querySelectorAll('.book').forEach(book => {
    book.style.setProperty('width', w, 'important');
    book.style.setProperty('max-width', w, 'important');
  });

  requestAnimationFrame(adjustImages);
}