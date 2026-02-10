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
