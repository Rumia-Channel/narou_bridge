// WebKit compatibility: Map polyfill
if (typeof Map === 'undefined') {
  window.Map = function () {
    this.data = {};
  };
  window.Map.prototype.set = function (key, value) {
    this.data[key] = value;
    return this;
  };
  window.Map.prototype.get = function (key) {
    return this.data[key];
  };
  window.Map.prototype.has = function (key) {
    return key in this.data;
  };
  window.Map.prototype.delete = function (key) {
    delete this.data[key];
  };
  window.Map.prototype.clear = function () {
    this.data = {};
  };
}

// WebKit compatibility: URLSearchParams polyfill
if (typeof URLSearchParams === 'undefined') {
  window.URLSearchParams = function (search) {
    this.params = {};
    if (search) {
      var pairs = search.substring(1).split('&');
      for (var i = 0; i < pairs.length; i++) {
        var pair = pairs[i].split('=');
        if (pair.length === 2) {
          this.params[decodeURIComponent(pair[0])] = decodeURIComponent(pair[1]);
        }
      }
    }
  };
  window.URLSearchParams.prototype.get = function (name) {
    return this.params[name] || null;
  };
}

// WebKit compatibility: requestAnimationFrame polyfill
if (!window.requestAnimationFrame) {
  window.requestAnimationFrame = function (callback) {
    return setTimeout(callback, 1000 / 60);
  };
}

// WebKit compatibility: Object.entries polyfill
if (!Object.entries) {
  Object.entries = function (obj) {
    var entries = [];
    for (var key in obj) {
      if (obj.hasOwnProperty(key)) {
        entries.push([key, obj[key]]);
      }
    }
    return entries;
  };
}


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
 * @param {object} novelData DB API から読み込んだ作品データ（.serialization を使う）
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




document.addEventListener('DOMContentLoaded', async () => {
  const query = getQueryParams();

  const btnClear = document.getElementById('btn-clear-cache');
  btnClear.addEventListener('click', () => {
    showToast('ページを再読み込みします。', { type: 'success', duration: 800 });
    setTimeout(function () { window.location.reload(); }, 800);
  });

  if (query.site && query.nid) {
    initWidthSelector();
    const pcReader = document.getElementById('progress-container');
    if (pcReader) pcReader.style.display = 'none';
    await renderReaderScreen(query);
    return;
  }

  initWidthSelector();
  initNavOnLibrary();
  const app = document.getElementById('app');
  const pc = document.getElementById('progress-container');
  const pb = document.getElementById('progress-bar');
  const pi = document.getElementById('progress-info');

  async function showLibraryPage(requestedPage) {
    if (pc) pc.style.display = 'block';
    if (pb) pb.style.width = '0%';
    if (pi) pi.textContent = '作品一覧をDBから読み込み中…';

    const page = await loadLibraryPage(sources, requestedPage, function (completed, total) {
      if (!pb || !pi) return;
      const pct = total > 0 ? Math.min(100, completed / total * 100) : 0;
      pb.style.width = pct + '%';
      pi.textContent = `作品一覧をDBから読み込み中: ${completed}/${total}`;
    });
    if (pc) pc.style.display = 'none';
    await renderLibraryWithProgress(app, page.works);
    renderLibraryPagination(app, page.page, page.totalPages, showLibraryPage);

    const url = new URL(window.location.href);
    if (page.page > 1) {
      url.searchParams.set('page', String(page.page));
    } else {
      url.searchParams.delete('page');
    }
    window.history.replaceState(null, '', url);
    window.scrollTo({ top: 0, behavior: 'smooth' });
  }

  await showLibraryPage(query.page || 1);
});

const LIBRARY_PAGE_SIZE = 100;

async function loadLibraryPage(siteSources, requestedPage, onProgress) {
  const pageNumber = Math.max(1, requestedPage);
  const allWorks = [];
  const totals = [];
  let completed = 0;
  let total = 0;

  for (const source of siteSources) {
    const offset = (pageNumber - 1) * LIBRARY_PAGE_SIZE;
    const url = '/api/library/works?site=' + encodeURIComponent(source)
      + '&sort=updated_desc&limit=' + LIBRARY_PAGE_SIZE + '&offset=' + offset;
    const response = await fetch(url, { cache: 'default' });
    if (!response.ok) throw new Error(`作品API ${source}: HTTP ${response.status}`);
    const page = await response.json();
    const works = Array.isArray(page.works) ? page.works : [];
    for (const work of works) {
      allWorks.push({ ...work, id: work.work_key, source: work.site });
    }
    totals.push(page.total);
    completed += Math.min(offset + works.length, page.total);
    total += page.total;
    onProgress(completed, total);
  }

  const totalPages = Math.max(1, ...totals.map(value => Math.ceil(value / LIBRARY_PAGE_SIZE)));
  if (pageNumber > totalPages) {
    return loadLibraryPage(siteSources, totalPages, onProgress);
  }
  return { works: allWorks, page: pageNumber, totalPages };
}

function renderLibraryPagination(container, page, totalPages, onPage) {
  if (totalPages <= 1) return;
  const controls = document.createElement('nav');
  controls.className = 'reader-pagination';
  controls.setAttribute('aria-label', '作品一覧ページ');

  const previous = document.createElement('button');
  previous.type = 'button';
  previous.textContent = '前へ';
  previous.disabled = page <= 1;
  previous.addEventListener('click', () => onPage(page - 1));

  const status = document.createElement('span');
  status.textContent = `${page} / ${totalPages}`;

  const next = document.createElement('button');
  next.type = 'button';
  next.textContent = '次へ';
  next.disabled = page >= totalPages;
  next.addEventListener('click', () => onPage(page + 1));

  controls.append(previous, status, next);
  container.appendChild(controls);
}




async function loadJSON(path) {
  const response = await fetch(path, { cache: 'default' });
  if (!response.ok) return null;
  return await response.json();
}


function getReadStatus(novel) {
  const episodeIds = Array.isArray(novel.episode_ids)
    ? novel.episode_ids.map(String)
    : Object.keys(novel.episodes_data || novel.episodes || {}).map(function (key) {
        const episode = (novel.episodes_data || novel.episodes)[key];
        return String(episode.id);
      });
  if (episodeIds.length === 0) return 'unread';

  const epDoneKey = `epFinished_${novel.source}_${novel.id}`;
  const doneSet = new Set(JSON.parse(localStorage.getItem(epDoneKey) || '[]').map(String));
  let anyProgress = false;
  let finishedCount = 0;

  for (const epId of episodeIds) {
    if (doneSet.has(epId)) {
      finishedCount++;
      continue;
    }
    if (localStorage.getItem(`readPage_${novel.source}_${novel.id}_${epId}`)) {
      anyProgress = true;
    }
  }

  if (finishedCount === episodeIds.length) return 'finished';
  if (!anyProgress && finishedCount === 0) return 'unread';
  return 'reading';
}


/**
 * プログレスバーと文字表示を更新する共通関数
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

  // LocalStorage から折りたたみ状態を取得（author_id をキーに保存します）
  const collapsed = JSON.parse(localStorage.getItem('collapsedAuthors') || '[]');
  const now = new Date();

  // author_id でグループ化
  const authorsMap = {};
  novelsList.forEach(novel => {
    const aid = novel.author_id;
    if (!authorsMap[aid]) authorsMap[aid] = [];
    authorsMap[aid].push(novel);
  });

  // 各グループ内を更新日時順（新しい順）にソート
  Object.values(authorsMap).forEach(list => {
    list.sort((a, b) => new Date(b.update_date) - new Date(a.update_date));
  });

  // グループごとに表示する作者名を最新作品から取得
  const authorNameMap = {};
  Object.entries(authorsMap).forEach(([aid, list]) => {
    authorNameMap[aid] = list[0].author;
  });

  // author_id のリストを表示名の五十音順でソート
  const sortedAuthorIds = Object.keys(authorsMap).sort((a, b) => {
    return authorNameMap[a].localeCompare(authorNameMap[b], 'ja');
  });

  // ここから「目次を読み込み中」フェーズ
  const totalItems = novelsList.length;
  let doneItems = 0;
  updateProgress(doneItems, totalItems, '目次を読み込み中:');

  // 少し待ってから描画開始（UI が固まるのを防ぐため）
  await new Promise(r => setTimeout(r, 50));

  for (const aid of sortedAuthorIds) {
    // 作者グループ全体のコンテナを作成
    const group = document.createElement('div');
    group.className = 'author-group';

    // ヘッダーに表示用作者名をセット
    const header = document.createElement('div');
    header.className = 'author-header';
    header.textContent = authorNameMap[aid];

    // NEW バッジ（7日以内に更新があれば）
    const hasNew = authorsMap[aid].some(novel =>
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
    content.style.display = collapsed.includes(aid) ? 'none' : 'block';

    header.addEventListener('click', () => {
      const isNow = content.style.display === 'block';
      content.style.display = isNow ? 'none' : 'block';
      const idx = collapsed.indexOf(aid);
      if (isNow) {
        if (idx === -1) collapsed.push(aid);
      } else {
        if (idx !== -1) collapsed.splice(idx, 1);
      }
      localStorage.setItem('collapsedAuthors', JSON.stringify(collapsed));
    });

    group.appendChild(header);
    group.appendChild(content);
    container.appendChild(group);

    // カードを１つずつ作成
    for (const novel of authorsMap[aid]) {
      const novelElem = document.createElement('div');
      novelElem.className = 'novel';

      // カバー画像
      const covCont = document.createElement('div');
      covCont.className = 'cover-container';
      const img = document.createElement('img');
      img.loading = 'lazy';
      img.decoding = 'async';
      img.src = `/covers/${encodeURIComponent(novel.source)}/${encodeURIComponent(novel.id)}`;
      covCont.appendChild(img);

      // バッジ表示
      const badgeWrap = document.createElement('div');
      badgeWrap.className = 'cover-badges';

      const days = (now - new Date(novel.update_date)) / (1000 * 3600 * 24);
      const readStatus = getReadStatus(novel);
      badgeWrap.appendChild(makeBadge('NEW', days <= 7));
      const statusLabel = readStatus === 'unread' ? '未読'
        : readStatus === 'reading' ? '既読'
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

      // 100件ごとに少しだけ待つ（UI 更新が詰まるのを防ぐ）
      if (doneItems % 100 === 0) {
        await new Promise(r => setTimeout(r, 10));
      }
    }
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

  const novelPath = `/api/library/works/${encodeURIComponent(query.site)}/${encodeURIComponent(query.nid)}`;
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
    let episodeMetadata;
    let episodeRef;
    if (hasEpisodeId) {
      episodeMetadata = episodesObj[query.eid]
        || episodesArr.find(ep => String(ep.id) === String(query.eid));
      episodeRef = query.eid;
    } else {
      const firstEpisode = Object.entries(episodesObj)[0];
      episodeMetadata = firstEpisode && firstEpisode[1];
      episodeRef = episodeMetadata && episodeMetadata.id
        ? String(episodeMetadata.id)
        : firstEpisode && firstEpisode[0];
    }

    if (!episodeMetadata || !episodeRef) {
      app.innerHTML = '<div>指定されたエピソードが見つかりません。</div>';
      return;
    }

    const episodePath = `/api/library/works/${encodeURIComponent(query.site)}`
      + `/${encodeURIComponent(query.nid)}/episodes/${encodeURIComponent(episodeRef)}`;
    const episode = await loadJSON(episodePath);
    if (!episode) {
      app.innerHTML = '<div>エピソード本文を読み込めませんでした。</div>';
      return;
    }
    if (!episode.id) episode.id = episodeMetadata.id || episodeRef;
    await renderEpisode(app, novelData, episode, episodesArr);

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
        const spanTitle = document.createElement('span');
        spanTitle.className = 'toc-ep-title';
        spanTitle.textContent = ep.title;
        const spanDate = document.createElement('span');
        spanDate.className = 'toc-ep-date';
        spanDate.textContent = formatDate(ep.updateDate);
        a.appendChild(spanTitle);
        a.appendChild(spanDate);
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
        const spanTitle = document.createElement('span');
        spanTitle.className = 'toc-ep-title';
        spanTitle.textContent = ep.title;
        const spanDate = document.createElement('span');
        spanDate.className = 'toc-ep-date';
        spanDate.textContent = formatDate(ep.updateDate);
        a.appendChild(spanTitle);
        a.appendChild(spanDate);
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
  const header = document.querySelector('header');
  const headerH = header ? header.offsetHeight : 0;
  const maxH = (window.innerHeight - headerH) * 0.95;

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