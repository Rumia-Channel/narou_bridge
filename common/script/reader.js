// スクリプト冒頭で Map を用意
const coverUrlMap = new Map();
// BlobのSHA-256ハッシュ → ObjectURL の共有マップ
const coverHashMap = new Map();

// Cache Storage 名
const CACHE_NAME = 'cover-images';
const INDEX_CACHE = 'index-json-cache';

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

  const btn = document.createElement('button');
  btn.id = 'btn-nav-reader';

  let label, href;
  const isShort = novelData.serialization === '短編';
  const hasEid = Boolean(query.eid);

  if (isShort || hasEid) {
    // --- 本文表示中 ---
    if (isShort) {
      label = '本棚に戻る';
      href = '/reader/';  // 本棚へのパス
    } else {
      label = '目次に戻る';
      href = `?site=${novelData.source}&nid=${novelData.id}`;
    }
  } else {
    // --- 目次表示中 ---
    label = '本棚に戻る';
    href = '/reader/';
  }

  btn.textContent = label;
  btn.style.marginLeft = '1em';
  btn.addEventListener('click', () => {
    window.location.href = href;
  });
  header.appendChild(btn);
}


document.addEventListener('DOMContentLoaded', async () => {

  const query = getQueryParams();

  // --- キャッシュクリアボタンの設定 ---
  const btnClear = document.getElementById('btn-clear-cache');
  btnClear.addEventListener('click', async () => {
    // 1) Cache Storage 削除
    await caches.delete(CACHE_NAME);

    // 2) localStorage からキャッシュ関連キーを削除
    Object.keys(localStorage).forEach(key => {
      if (key.startsWith('coverFail_')) {
        localStorage.removeItem(key);
      }
    });

    await caches.delete(INDEX_CACHE);

    // 3) メモリ上マップもクリア
    coverUrlMap.clear();
    coverHashMap.clear();

    alert('キャッシュをクリアしました。ページを再読み込みします。');
    window.location.reload();
  });

  if (!(query.site && query.nid)) {
    initNavOnLibrary();
  }

  const app = document.getElementById('app');
  const pc = document.getElementById('progress-container');
  const pb = document.getElementById('progress-bar');
  const pi = document.getElementById('progress-info');

  // ------- リーダー画面なら幅セレクタをセットアップ -------
  if (query.site && query.nid) {
    initWidthSelector();
    if (pc) pc.style.display = 'none';
    await renderReaderScreen(query);
    return;
  }

  // 進捗バーの状態管理
  let completed = 0, total = 0;
  function setTotal(n) {
    total = n;
    completed = 0;
    updateBar();
  }
  function increment() {
    completed++;
    updateBar();
  }
  function updateBar() {
    const pct = total > 0 ? (completed / total * 100) : 0;
    const pctText = pct.toFixed(2) + '%';
    pb.style.width = pct + '%';
    pi.textContent = `${pctText} (${completed}/${total})`;
  }

  // ② 本棚画面：index.json 読み込み
  setTotal(sources.length);
  const novelsList = [];
  for (const src of sources) {
    novelsList.push(...await loadIndexWithCache(src));
    increment();
  }

  // ③ カバーURL解決＋キャッシュ
  setTotal(novelsList.length);
  const coverCache = await caches.open(CACHE_NAME);
  for (const novel of novelsList) {
    await preloadAndMapCover(novel, coverCache);
    increment();
  }

  // ④ プログレスバーを隠して本棚表示
  pc.style.display = 'none';
  await renderLibrary(app, novelsList);
});



// キャッシュ付き index.json 読み込み
async function loadIndexWithCache(source) {
  const path = `../${source}/index.json`;
  const cache = await caches.open(INDEX_CACHE);

  // 1) まず Cache Storage を覗く
  const cachedResp = await cache.match(path);
  if (cachedResp) {
    // キャッシュがあればその JSON を返す
    return await cachedResp.json();
  }

  // 2) なければフェッチしてキャッシュに入れつつ返す
  const resp = await fetch(path);
  if (!resp.ok) throw new Error(`Failed to fetch ${path}: ${resp.status}`);
  cache.put(path, resp.clone());
  return await resp.json();
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
  const defaultUrl = '/images/default_cover.png';

  // ローカルストレージで失敗回数を管理
  const failKey = `coverFail_${novel.source}_${novel.id}`;
  let failCount = parseInt(localStorage.getItem(failKey)) || 0;
  if (failCount >= 3) {
    // 失敗3回以上は defaultCoverBlobURL を使って即返す
    coverUrlMap.set(key, await getDefaultCoverObjectURL(coverCache, defaultUrl));
    return;
  }

  // 1) 既存の cover.jpg/png/gif キャッシュチェック
  for (const ext of ['jpg', 'png', 'gif']) {
    const url = base + `cover.${ext}`;
    const cached = await coverCache.match(url);
    if (cached) {
      const blob = await cached.blob();
      const objectURL = await dedupeBlob(blob);
      coverUrlMap.set(key, objectURL);
      return;
    }
  }

  // 2) HEAD→fetch→低解像度化→ハッシュチェック→キャッシュ登録
  for (const ext of ['jpg', 'png', 'gif']) {
    const url = base + `cover.${ext}`;
    try {
      const head = await fetch(url, { method: 'HEAD' });
      if (!head.ok) continue;

      const origBlob = await (await fetch(url)).blob();
      const smallBlob = await shrinkBlob(origBlob, 300, 0.7);
      const objectURL = await dedupeBlob(smallBlob);

      await coverCache.put(url, new Response(smallBlob));
      coverUrlMap.set(key, objectURL);
      return;
    } catch {
      continue;
    }
  }

  // 3) どの拡張子も見つからなかった → 失敗カウント＆default処理
  failCount++;
  localStorage.setItem(failKey, String(failCount));
  coverUrlMap.set(key, await getDefaultCoverObjectURL(coverCache, defaultUrl));
}

/** 
 * default_cover.png を一度 Cache Storage に ensure → blob→ObjectURL を返す
 */
async function getDefaultCoverObjectURL(coverCache, defaultUrl) {
  // キャッシュに無ければ fetch＆put
  const cachedDef = await coverCache.match(defaultUrl);
  if (!cachedDef) {
    const resp = await fetch(defaultUrl);
    if (resp.ok) await coverCache.put(defaultUrl, resp.clone());
  }
  // blob→ObjectURL
  const blob = await (await coverCache.match(defaultUrl)).blob();
  return URL.createObjectURL(blob);
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

  // LocalStorage から折りたたみ状態を取得
  const collapsed = JSON.parse(localStorage.getItem('collapsedAuthors') || '[]');
  const now = new Date();

  // 作者でグループ化
  const authors = {};
  novelsList.forEach(novel => {
    if (!authors[novel.author]) authors[novel.author] = [];
    authors[novel.author].push(novel);
  });

  // 作者名順にソート（日本語ロケール対応）
  const sortedAuthors = Object.keys(authors)
    .sort((a, b) => a.localeCompare(b, 'ja'));

  for (const author of sortedAuthors) {
    const group = document.createElement('div');
    group.className = 'author-group';

    // ヘッダー
    const header = document.createElement('div');
    header.className = 'author-header';
    header.textContent = author;

    // NEWマーク判定
    const hasNew = authors[author].some(novel =>
      (now - new Date(novel.update_date)) / (1000 * 3600 * 24) <= 7
    );
    if (hasNew) {
      const tag = document.createElement('span');
      tag.className = 'new-tag';
      tag.textContent = 'NEW';
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

      // 連載状況タグ
      const statusTag = document.createElement('span');
      statusTag.className = 'status-tag';
      // 短編 / 完結済み / 連載中 を振り分け
      if (novel.serialization === '短編') {
        statusTag.textContent = '短編';
      } else if (novel.serialization === '完結済み') {
        statusTag.textContent = '完結済み';
      } else {
        statusTag.textContent = '連載中';
      }
      novelElem.appendChild(statusTag);

      // 種類バッジ
      const typeBadge = document.createElement('span');
      typeBadge.className = 'type-badge';
      typeBadge.textContent = novel.type === 'comic' ? '漫画' : '小説';
      novelElem.appendChild(typeBadge);

      // カバー
      const covCont = document.createElement('div');
      covCont.className = 'cover-container';
      const img = document.createElement('img');
      const key = `${novel.source}_${novel.id}`;
      img.src = coverUrlMap.get(key);
      covCont.appendChild(img);
      covCont.addEventListener('click', () => {
        window.location.href = createNovelURL(novel);
      });

      // タイトル
      const title = document.createElement('div');
      title.className = 'novel-title';
      title.textContent = novel.title;

      // NEWマーク
      const days = (now - new Date(novel.update_date)) / (1000 * 3600 * 24);
      if (days <= 7) {
        const nt = document.createElement('span');
        nt.className = 'new-tag';
        nt.textContent = 'NEW';
        title.appendChild(nt);
      }

      novelElem.appendChild(covCont);
      novelElem.appendChild(title);
      content.appendChild(novelElem);
    }

    group.appendChild(header);
    group.appendChild(content);
    container.appendChild(group);
  }
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
  return { site: p.get('site'), nid: p.get('nid'), eid: p.get('eid') };
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



function renderEpisode(container, novel, episode, episodesArr) {
  // 1. 既存コンテンツをクリア
  container.innerHTML = '';

  // 2. 見出し（タイトル）
  const h2 = document.createElement('h2');
  h2.textContent = episode.title;
  container.appendChild(h2);

  // 3. 前書き
  if (episode.introduction) {
    const intro = document.createElement('div');
    intro.className = 'introduction';
    intro.innerHTML = formatText(episode.introduction);
    container.appendChild(intro);
  }

  // 4. 本文中の挿絵とテキストをパース
  const pattern = /\[image\]\(([^)]+)\)/g;
  let lastIndex = 0, match;
  while ((match = pattern.exec(episode.text)) !== null) {
    // テキスト部分
    if (match.index > lastIndex) {
      const textSeg = episode.text.slice(lastIndex, match.index);
      const p = document.createElement('div');
      p.className = 'main-text';
      p.innerHTML = formatText(textSeg);
      container.appendChild(p);
    }
    // 挿絵部分
    const img = document.createElement('img');
    img.className = 'episode-illustration';
    img.src = `/images/${match[1]}`;
    img.alt = '';
    container.appendChild(img);

    lastIndex = pattern.lastIndex;
  }
  // 最後のテキスト
  if (lastIndex < episode.text.length) {
    const textSeg = episode.text.slice(lastIndex);
    const p = document.createElement('div');
    p.className = 'main-text';
    p.innerHTML = formatText(textSeg);
    container.appendChild(p);
  }

  // 5. あとがき
  if (episode.postscript) {
    const post = document.createElement('div');
    post.className = 'postscript';
    post.innerHTML = formatText(episode.postscript);
    container.appendChild(post);
  }

  // 6. ナビゲーションリンク
  const nav = document.createElement('div');
  nav.className = 'nav-links';

  // 前へ
  const idx = episodesArr.findIndex(ep => String(ep.id) === String(episode.id));
  if (idx > 0) {
    const prev = episodesArr[idx - 1];
    const a = document.createElement('a');
    a.className = 'nav-link';
    a.href = `?site=${novel.source}&nid=${novel.id}&eid=${prev.id}`;
    a.textContent = '← 前へ';
    nav.appendChild(a);
  }

  // 目次／本棚に戻る
  const back = document.createElement('a');
  back.className = 'nav-link';
  if (novel.serialization === '短編') {
    back.href = '/reader/';
    back.textContent = '本棚に戻る';
  } else {
    back.href = `?site=${novel.source}&nid=${novel.id}`;
    back.textContent = '目次に戻る';
  }
  nav.appendChild(back);

  // 次へ
  if (idx < episodesArr.length - 1) {
    const next = episodesArr[idx + 1];
    const a = document.createElement('a');
    a.className = 'nav-link';
    a.href = `?site=${novel.source}&nid=${novel.id}&eid=${next.id}`;
    a.textContent = '次へ →';
    nav.appendChild(a);
  }

  container.appendChild(nav);
}

/** 単純な debounce ユーティリティ */
function debounce(fn, ms) {
  let t;
  return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); };
}


// 改行やタグの処理（必要に応じて追加）
function formatText(text) {
  return text.replace(/\n/g, '<br>').replace(/\[newpage\]/g, '<hr>');
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
  const storageKey = `tocCollapsed_${novel.source}_${novel.id}`;
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
        a.textContent = `${ep.title} — ${formatDate(ep.updateDate)}`;
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
        a.textContent = `${ep.title} — ${formatDate(ep.updateDate)}`;
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
  return `${d.getFullYear()}/${z(d.getMonth() + 1)}/${z(d.getDate())} `
    + `${z(d.getHours())}:${z(d.getMinutes())}`;
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