/* ========================================
   Narou Bridge - Modern UI JavaScript
   ======================================== */

// Toggle collapsible sections
function toggleCollapsible(header) {
  header.classList.toggle('collapsed');
  const content = header.nextElementSibling;
  if (content && content.classList.contains('collapsible-content')) {
    content.classList.toggle('expanded');
  }
}

// Update file input label
function updateFileLabel(input, labelId) {
  const label = document.getElementById(labelId);
  if (input.files && input.files[0]) {
    label.querySelector('span').textContent = input.files[0].name;
  } else {
    label.querySelector('span').textContent = 'ファイルを選択';
  }
}

// Initialize collapsible sections on page load
document.addEventListener('DOMContentLoaded', function() {
  // Collapse file conversion section by default
  const fileSection = document.querySelector('.collapsible-header');
  if (fileSection) {
    fileSection.classList.add('collapsed');
    const content = fileSection.nextElementSibling;
    if (content && content.classList.contains('collapsible-content')) {
      content.classList.remove('expanded');
    }
  }
});

// ========================================
// Account Management Functions
// ========================================

// Load accounts for selected site
async function loadAccounts() {
  const site = document.getElementById('accountSite').value;
  const accountList = document.getElementById('accountList');
  
  if (!site) {
    accountList.innerHTML = '<p style="color: var(--text-secondary);">サイトを選択してください</p>';
    return;
  }
  
  try {
    const response = await fetch(`/api/account?site=${encodeURIComponent(site)}`);
    const data = await response.json();
    
    if (data.accounts && data.accounts.length > 0) {
      let html = '<div style="display: flex; flex-direction: column; gap: 0.5rem;">';
      data.accounts.forEach(account => {
        const isActive = account.name === 'login';
        html += `
          <div style="display: flex; align-items: center; justify-content: space-between; padding: 0.75rem; background: var(--bg-tertiary); border-radius: var(--radius-md); ${isActive ? 'border: 2px solid var(--accent-primary);' : ''}">
            <div style="display: flex; flex-direction: column; gap: 0.25rem;">
              <span style="font-weight: 500;">${account.name}${isActive ? ' (現在のログイン)' : ''}</span>
              <span style="font-size: 0.875rem; color: var(--text-secondary);">
                ${account.updated ? new Date(account.updated).toLocaleString('ja-JP') : '日時不明'}
              </span>
            </div>
            <div style="display: flex; gap: 0.5rem;">
              ${account.name !== 'login' ? `
                <button onclick="switchAccount('${site}', '${account.name}')" class="btn btn-sm btn-primary" title="このアカウントに切り替え">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="20 6 9 17 4 12"/>
                  </svg>
                  <span>使用</span>
                </button>
              ` : ''}
              <button onclick="deleteAccount('${site}', '${account.name}')" class="btn btn-sm btn-danger" title="削除">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="3 6 5 6 21 6"/>
                  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                </svg>
              </button>
            </div>
          </div>
        `;
      });
      html += '</div>';
      accountList.innerHTML = html;
    } else {
      accountList.innerHTML = '<p style="color: var(--text-secondary);">アカウントが見つかりません</p>';
    }
  } catch (error) {
    console.error('Failed to load accounts:', error);
    accountList.innerHTML = '<p style="color: var(--danger);">アカウントの読み込みに失敗しました</p>';
  }
}

// Upload account file
async function uploadAccount() {
  const site = document.getElementById('accountSite').value;
  const fileInput = document.getElementById('accountFile');
  const nameInput = document.getElementById('accountName');
  
  if (!site) {
    alert('サイトを選択してください');
    return;
  }
  
  if (!fileInput.files || !fileInput.files[0]) {
    alert('ファイルを選択してください');
    return;
  }
  
  const formData = new FormData();
  formData.append('file', fileInput.files[0]);
  if (nameInput.value) {
    formData.append('name', nameInput.value);
  }
  
  try {
    const response = await fetch(`/api/account?site=${encodeURIComponent(site)}`, {
      method: 'POST',
      body: formData
    });
    
    const data = await response.json();
    
    if (data.status === 'success') {
      alert(data.message);
      // Reset form
      fileInput.value = '';
      updateFileLabel(fileInput, 'account-label');
      nameInput.value = '';
      // Reload account list
      loadAccounts();
    } else {
      alert('エラー: ' + (data.message || 'アップロードに失敗しました'));
    }
  } catch (error) {
    console.error('Upload error:', error);
    alert('アップロードに失敗しました');
  }
}

// Switch to selected account
async function switchAccount(site, accountName) {
  if (!confirm(`アカウント「${accountName}」に切り替えますか？`)) {
    return;
  }
  
  try {
    const response = await fetch(`/api/account/switch?site=${encodeURIComponent(site)}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ account: accountName })
    });
    
    const data = await response.json();
    
    if (data.status === 'success') {
      alert(data.message);
      loadAccounts();
    } else {
      alert('エラー: ' + (data.message || '切り替えに失敗しました'));
    }
  } catch (error) {
    console.error('Switch error:', error);
    alert('切り替えに失敗しました');
  }
}

// Delete account
async function deleteAccount(site, accountName) {
  if (!confirm(`アカウント「${accountName}」を削除しますか？`)) {
    return;
  }
  
  try {
    const response = await fetch(
      `/api/account?site=${encodeURIComponent(site)}&account=${encodeURIComponent(accountName)}`,
      {
      method: 'DELETE'
      }
    );
    
    const data = await response.json();
    
    if (data.status === 'success') {
      alert(data.message);
      loadAccounts();
    } else {
      alert('エラー: ' + (data.message || '削除に失敗しました'));
    }
  } catch (error) {
    console.error('Delete error:', error);
    alert('削除に失敗しました');
  }
}
