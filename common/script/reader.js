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
  const btnToText = document.createElement('button');
  btnToText.textContent = '本文へ進む';
  btnToText.style.marginLeft = '1em';
  btnToText.addEventListener('click', () => {
    window.location.href = `/${novelData.source}/${novelData.id}/`;
  });
  header.appendChild(btnToText);
}

/**
 * 同時接続数を 10 に制限しつつ、完了次第すぐに次のダウンロードを開始する版
 * @param {Array}  novelsList - 小説オブジェクトの配列
 * @param {Cache}  coverCache  - Cache Storage オブジェクト
 * @param {Function} onComplete - １ファイルが完了するたびに呼ばれるコールバック
 */
async function preloadAllCoversWithLimit(novelsList, coverCache, onComplete) {
  const CONCURRENCY_LIMIT = 4;
  let index = 0;        // 次に処理すべき novelsList のインデックス
  const executing = []; // 現在ダウンロード中の Promise を格納

  // 1件分のダウンロードタスクを作成して executing に追加し、完了時に executing から外しつつ onComplete() を呼ぶ
  function enqueueOne() {
    if (index >= novelsList.length) return null;
    const novel = novelsList[index++];
    const p = preloadAndMapCover(novel, coverCache)
      .then(() => {
        // 成功したら進捗コールバックを呼ぶ
        if (typeof onComplete === 'function') onComplete();
      })
      .catch(() => {
        // 失敗でも進捗コールバックを呼ぶ(状況に応じて)
        if (typeof onComplete === 'function') onComplete();
      })
      .finally(() => {
        // 完了したら executing から外す
        const i = executing.indexOf(p);
        if (i !== -1) executing.splice(i, 1);
      });
    executing.push(p);
    return p;
  }

  // 最初に最大 CONCURRENCY_LIMIT 件だけキューに乗せる
  for (let i = 0; i < CONCURRENCY_LIMIT; i++) {
    const t = enqueueOne();
    if (!t) break;
  }

  // いずれかが終わるたびに新しいタスクを enqueueOne していく
  while (executing.length > 0) {
    await Promise.race(executing);
    enqueueOne();
  }
}


document.addEventListener('DOMContentLoaded', async () => {
  const query = getQueryParams();

 // キャッシュクリアボタン（そのまま）
  const btnClear = document.getElementById('btn-clear-cache');
  btnClear.addEventListener('click', async () => {
    await caches.delete(CACHE_NAME);
    Object.keys(localStorage).forEach(key => {
      if (key.startsWith('coverFail_')) {
        localStorage.removeItem(key);
      }
    });
    coverUrlMap.clear();
    coverHashMap.clear();
    alert('キャッシュをクリアしました。ページを再読み込みします。');
    window.location.reload();
  });

  // リーダー画面かどうか判定
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
  const pc  = document.getElementById('progress-container');
  const pb  = document.getElementById('progress-bar');
  const pi  = document.getElementById('progress-info');

  //
  // (1) index.json 取得フェーズ
  //
  if (pc) pc.style.display = 'block';
  let completedIndex = 0;
  const totalIndex   = sources.length;

  // index.json取得中に呼び出す関数
  function updateIndexBar() {
    const pct = totalIndex > 0 ? (completedIndex / totalIndex * 100) : 0;
    pb.style.width = pct + '%';
    pi.textContent = `index.json を取得中: ${pct.toFixed(2)}% (${completedIndex}/${totalIndex})`;
  }
  updateIndexBar();

  // sources 配列を並列に fetch しつつ、完了ごとに進捗更新
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
  // index取得完了したので一度バーを100%にして短時間表示
  pi.textContent = '目次を読み込み中…';
  pb.style.width = '100%';
  // 目次構築中の状態を少しだけ見せる（不要なら delay を 0 に変更可）
  await new Promise(r => setTimeout(r, 200));

  //
  // (3) カバーキャッシュチェック → キャッシュミス分をDL
  //
  const coverCache = await caches.open(CACHE_NAME);

  // 「キャッシュミスのノベル一覧」を作る
  const needFetchList = [];
  for (const novel of novelsList) {
    const base = `../${novel.source}/${novel.id}/`;
    let cached = false;
    for (const ext of ['jpg', 'png', 'gif']) {
      const url = base + `cover.${ext}`;
      // Cache Storage 内に同一URLがあれば cached=true
      const resp = await coverCache.match(url);
      if (resp) {
        cached = true;
        break;
      }
    }
    if (!cached) {
      needFetchList.push(novel);
    }
  }

  // もしキャッシュミスがゼロなら、バーを隠して一度だけ目次を描画して終了
  if (needFetchList.length === 0) {
    if (pc) pc.style.display = 'none';
    renderLibrary(app, novelsList);
    return;
  }

  // 「カバーDL中」フェーズの進捗用変数を用意
  let completedCover = 0;
  const totalCover   = needFetchList.length;

  function updateCoverBar() {
    const pct = totalCover > 0 ? (completedCover / totalCover * 100) : 0;
    pb.style.width = pct + '%';
    pi.textContent = `カバーをダウンロード中: ${pct.toFixed(2)}% (${completedCover}/${totalCover})`;
  }

  // フェーズ開始時にバーを 0% に初期化して文字表示
  completedCover = 0;
  updateCoverBar();

  // 同時最大 10 件でダウンロードし、完了ごとに updateCoverBar を呼ぶ
  await preloadAllCoversWithLimit(
    needFetchList,
    coverCache,
    () => {
      completedCover++;
      updateCoverBar();
    }
  );

  // カバーDL完了後にバーを隠して最終的に目次を描画
  if (pc) pc.style.display = 'none';
  renderLibrary(app, novelsList);
});


// index.json のソースリスト
const INDEX_CACHE = 'index-json-cache';              // Cache Storage 名
const INDEX_ETAG_KEY_PREFIX = 'indexETag_';          // localStorage に ETag を保存する際のキー接頭辞

/**
 * キャッシュ付き index.json の読み込み（条件付き GET 版）
 * ※ 戻り値を「配列」に変換して返すように修正
 * @param {string} source - サイト名またはディレクトリ名
 * @returns {Promise<Array>} - [{ id, source, ...novelObject }, …] の配列
 */
async function loadIndexWithCache(source) {
  const url = new URL(`../${source}/index.json`, location.href).toString();
  const cache = await caches.open(INDEX_CACHE);
  const etagKey = `${INDEX_ETAG_KEY_PREFIX}${source}`;
  const storedEtag = localStorage.getItem(etagKey);

  // --- 1) Cache Storage からキャッシュ済みレスポンスを取得 ---
  const cachedResp = await cache.match(url);
  let cachedObj = null;
  if (cachedResp) {
    try {
      cachedObj = await cachedResp.clone().json();  // オブジェクト形式で取得
    } catch {
      cachedObj = null;
    }
  }

  // --- 2) 条件付き GET 用ヘッダーを準備 ---
  const headers = {};
  if (storedEtag) {
    headers['If-None-Match'] = storedEtag;
    // サーバーが Last-Modified のみ返す場合は、ここで headers['If-Modified-Since'] = storedEtag; を使う
  }

  // --- 3) サーバーへ GET 要請（If-None-Match / If-Modified-Since を付与） ---
  let resp;
  try {
    resp = await fetch(url, {
      method: 'GET',
      headers: headers,
      cache: 'no-store'
    });
  } catch (e) {
    // ネットワークエラーなどで失敗したら、キャッシュ版があればそれを配列化して返す
    if (cachedObj) {
      return Object.entries(cachedObj).map(([id, novel]) => ({ id, source, ...novel }));
    }
    throw e;
  }

  // --- 4) 304 Not Modified：キャッシュ版をそのまま配列で返す ---
  if (resp.status === 304 && cachedObj) {
    return Object.entries(cachedObj).map(([id, novel]) => ({ id, source, ...novel }));
  }

  // --- 5) 200 OK：更新があった → JSONをパースしてキャッシュも更新 ---
  if (resp.status === 200) {
    let freshObj;
    try {
      freshObj = await resp.clone().json();  // オブジェクト形式で取得
    } catch (e) {
      // パース失敗でもキャッシュ版があれば配列化して返す
      if (cachedObj) {
        return Object.entries(cachedObj).map(([id, novel]) => ({ id, source, ...novel }));
      }
      throw e;
    }

    // Cache Storage に最新の index.json を保存
    await cache.put(url, resp.clone());

    // 新しい ETag（または Last-Modified）を localStorage に保存
    const newEtag = resp.headers.get('ETag') || resp.headers.get('Last-Modified');
    if (newEtag) {
      localStorage.setItem(etagKey, newEtag);
    }

    // オブジェクトを配列に変換して返す
    return Object.entries(freshObj).map(([id, novel]) => ({ id, source, ...novel }));
  }

  // --- 6) その他ステータス（404, 500 など）はキャッシュ版あればそれを返し、なければ例外 ---
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

async function renderLibrary(container, novelsList) {
  container.innerHTML = '';

  // 折りたたみ状態を取得
  const collapsed = JSON.parse(localStorage.getItem('collapsedAuthors') || '[]');
  const now = new Date();

  // 作者ごとにグループ化
  const authors = {};
  novelsList.forEach(novel => {
    if (!authors[novel.author]) authors[novel.author] = [];
    authors[novel.author].push(novel);
  });

  // 作者内で更新日時順（新しい順）にソート
  Object.values(authors).forEach(list => {
    list.sort((a, b) => new Date(b.update_date) - new Date(a.update_date));
  });

  // 作者名順にソート
  const sortedAuthors = Object.keys(authors).sort((a, b) => a.localeCompare(b, 'ja'));

  for (const author of sortedAuthors) {
    const group = document.createElement('div');
    group.className = 'author-group';

    // ヘッダー
    const header = document.createElement('div');
    header.className = 'author-header';
    header.textContent = author;

    // NEW バッジ
    const hasNew = authors[author].some(novel =>
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

    // 各小説カード
    for (const novel of authors[author]) {
      const novelElem = document.createElement('div');
      novelElem.className = 'novel';

      // カバー部分
      const covCont = document.createElement('div');
      covCont.className = 'cover-container';

      const img = document.createElement('img');
      // preloadAllCoversWithLimit ですでに coverUrlMap にセット済みのはず
      const key = `${novel.source}_${novel.id}`;
      img.src = coverUrlMap.get(key) || '/images/default_cover.png';
      covCont.appendChild(img);

      // バッジ表示用ラッパー
      const badgeWrap = document.createElement('div');
      badgeWrap.className = 'cover-badges';

      const days = (now - new Date(novel.update_date)) / (1000 * 3600 * 24);
      const readStatus = getReadStatus(novel);

      // 1) NEW
      badgeWrap.appendChild(makeBadge('NEW', days <= 7));

      // 2) 読書状態
      const statusLabel = readStatus === 'unread' ? '未読'
        : readStatus === 'read' ? '既読'
          : '完読';
      badgeWrap.appendChild(makeBadge(statusLabel));

      // 3) 連載状況
      badgeWrap.appendChild(makeBadge(novel.serialization));

      // 4) 種別
      badgeWrap.appendChild(makeBadge(novel.type === 'comic' ? '漫画' : '小説'));

      covCont.appendChild(badgeWrap);

      // クリックで作品ページへ遷移
      covCont.addEventListener('click', () => {
        window.location.href = createNovelURL(novel);
      });

      novelElem.appendChild(covCont);

      // タイトル
      const title = document.createElement('div');
      title.className = 'novel-title';
      const titleText = document.createElement('div');
      titleText.className = 'title-text';
      titleText.textContent = novel.title;
      title.appendChild(titleText);

      novelElem.appendChild(title);
      content.appendChild(novelElem);
    }

    group.appendChild(header);
    group.appendChild(content);
    container.appendChild(group);
  }
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