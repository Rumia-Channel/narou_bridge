// データフェッチと描画周りの全スクリプト
let columns = [
    'serialization',
    'title',
    'author',
    'type',
    'tags',
    'create_date',
    'update_date'
];
let tableData = {};
let currentPage = 1;
let rowsPerPage = 10;
let hiddenCols = [];
let filteredAuthors = [];
let hiddenAuthors = [];
let typeFilter = "all";
let includedTags = [];
let excludedTags = [];
let includeOperator = "AND";
let excludeOperator = "AND";
let selectedRows = new Set();
const fixedWidthMapping = { serialization: 6, type: 3, create_date: 14, update_date: 14 };
const variableWeightMapping = { title: 50, author: 20, tags: 30 };
let sortInfo = { column: null, ascending: true };

async function fetchData() {
    document.getElementById("loading-overlay").style.display = "flex";
    try {
        const response = await fetch('index.json');
        tableData = await response.json();
        loadSettings();
        renderTagFilters();
        updateAuthorDropdownOptions();
        updateAuthorDropdownValue();
        renderHiddenAuthors();
        renderTableHeaders();
        renderTable();
        updatePagination();
    } catch (e) {
        console.error(e);
    } finally {
        document.getElementById("loading-overlay").style.display = "none";
    }
}

// 以降、既存の全 JS 関数 (loadSettings, saveSettings, renderTagFilters, etc.) をそのままここに貼り付け

fetchData().then(applySettingsToUI);