document.addEventListener('DOMContentLoaded', async () => {
    const query = getQueryParams();
  
    // クエリパラメータにsite, nidがあれば、リーダー画面として動作
    if (query.site && query.nid) {
      await renderReaderScreen(query);
      return; // 以降の本棚画面の描画は行わない
    }
  
    // 本棚画面（既存コードをそのまま）
    const app = document.getElementById('app');
    const novelsList = (await Promise.all(sources.map(loadSource))).flat();
    await Promise.all(novelsList.map(preloadCoverImage));
    await renderLibrary(app, novelsList);
  });
  
  
  async function loadSource(source) {
    const path = `../${source}/index.json`;
    const data = await loadJSON(path);
    return Object.entries(data).map(([id, novel]) => ({ id, source, ...novel }));
  }
  
  async function loadJSON(path) {
    const response = await fetch(path);
    return await response.json();
  }
  
  // カバー画像を事前ロード
  async function preloadCoverImage(novel) {
    const img = new Image();
    img.src = await findCoverImage(novel.source, novel.id);
  }
  
  async function renderLibrary(container, novelsList) {
    container.innerHTML = '';
  
    const authors = {};
    const now = new Date();
  
    novelsList.forEach(novel => {
      if (!authors[novel.author]) authors[novel.author] = [];
      authors[novel.author].push(novel);
    });
  
    const sortedAuthors = Object.keys(authors).sort();
  
    for (const author of sortedAuthors) {
      const group = document.createElement('div');
      group.className = 'author-group';
  
      const header = document.createElement('div');
      header.className = 'author-header';
      header.textContent = author;
  
      const authorHasNew = authors[author].some(novel => {
        return (now - new Date(novel.update_date)) / (1000 * 3600 * 24) <= 7;
      });
  
      if (authorHasNew) {
        const authorNewTag = document.createElement('span');
        authorNewTag.className = 'new-tag';
        authorNewTag.textContent = 'NEW';
        header.appendChild(authorNewTag);
      }
  
      header.addEventListener('click', () => {
        content.style.display = content.style.display === 'none' ? 'block' : 'none';
      });
  
      const content = document.createElement('div');
      content.className = 'author-content';
  
      for (const novel of authors[author]) {
        const novelElem = document.createElement('div');
        novelElem.className = 'novel';
  
        const coverContainer = document.createElement('div');
        coverContainer.className = 'cover-container';
  
        const cover = document.createElement('img');
        cover.src = await findCoverImage(novel.source, novel.id);
        coverContainer.appendChild(cover);
  
        // クリック時のURL生成
        const novelUrl = createNovelURL(novel);
  
        // 画像クリックで本文・目次へ移動
        coverContainer.addEventListener('click', () => {
          window.location.href = novelUrl;
        });
  
        const title = document.createElement('div');
        title.className = 'novel-title';
        title.textContent = novel.title;
  
        const daysSinceUpdate = (now - new Date(novel.update_date)) / (1000 * 3600 * 24);
        if (daysSinceUpdate <= 7) {
          const newTag = document.createElement('span');
          newTag.className = 'new-tag';
          newTag.textContent = 'NEW';
          title.appendChild(newTag);
        }
  
        novelElem.appendChild(coverContainer);
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
  
  async function findCoverImage(source, id) {
    const base = `../${source}/${id}/`;
    const covers = ['cover.jpg', 'cover.png', 'cover.gif'];
  
    for (const cover of covers) {
      try {
        const response = await fetch(base + cover, { method: 'HEAD' });
        if (response.ok) {
          return base + cover;
        }
      } catch {
        continue;
      }
    }
    return '../images/default_cover.png';
  }
  
  // URLのクエリを解析
  function getQueryParams() {
    const params = new URLSearchParams(window.location.search);
    return {
      site: params.get('site'),
      nid: params.get('nid'),
      eid: params.get('eid'),
    };
  }
  
  async function renderReaderScreen(query) {
    const app = document.getElementById('app');
    app.innerHTML = '<div class="loading">読み込み中です……</div>';
  
    const novelPath = `../${query.site}/${query.nid}/raw/raw.json`;
    const novelData = await loadJSON(novelPath);
  
    if (!novelData) {
      app.innerHTML = '<div>小説データが見つかりません。</div>';
      return;
    }
  
    // 短編の場合 or eidが指定されている場合は本文を表示
    if (novelData.serialization === "短編" || query.eid) {
      const episode = query.eid 
        ? novelData.episodes[query.eid] 
        : Object.values(novelData.episodes)[0];
  
      renderEpisode(app, novelData, episode);
    } else {
      // 目次を表示
      renderToc(app, novelData, query);
    }
  }
  
  function renderEpisode(container, novel, episode) {
    container.innerHTML = `
      <div class="episode-container">
        <h2>${episode.title}</h2>
        ${episode.introduction ? `<div class="introduction">${episode.introduction}</div>` : ''}
        <div class="main-text">${formatText(episode.text)}</div>
        ${episode.postscript ? `<div class="postscript">${episode.postscript}</div>` : ''}
        <a href="?site=${novel.type}&nid=${novel.id}">目次に戻る</a>
      </div>
    `;
  }
  
  // 改行やタグの処理（必要に応じて追加）
  function formatText(text) {
    return text.replace(/\n/g, '<br>').replace(/\[newpage\]/g, '<hr>');
  }
  
  function renderToc(container, novel, query) {
    const episodes = Object.values(novel.episodes);
    const episodesList = episodes.map(ep => `
      <li>
        <a href="?site=${query.site}&nid=${query.nid}&eid=${ep.id}">
          ${ep.title} (${ep.textCount}文字)
        </a>
      </li>
    `).join('');
  
    container.innerHTML = `
      <div class="toc-container">
        <h2>${novel.title} - 目次</h2>
        <ul>${episodesList}</ul>
        <a href="../">本棚に戻る</a>
      </div>
    `;
  }
  
  