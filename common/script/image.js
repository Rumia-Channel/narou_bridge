function adjustImages() {
    var header = document.querySelector("header.app-header");
    var headerH = header ? header.offsetHeight : 0;
    var viewH = (window.innerHeight - headerH) * 0.95;
    var images = document.querySelectorAll("img");

    // WebKit compatibility: use for loop instead of forEach
    for (var i = 0; i < images.length; i++) {
        var img = images[i];

        // 一度インラインスタイルをクリア
        img.style.width = "";
        img.style.height = "";
        if (!img.naturalWidth || !img.naturalHeight) continue;

        // 画像の自然サイズ
        var naturalW = img.naturalWidth;
        var naturalH = img.naturalHeight;

        // 親要素（コンテナ）の幅を基準にする
        var container = img.parentElement;
        var containerW = container ? container.clientWidth : window.innerWidth;

        // コンテナ幅・ビューポート高さに収まるよう縮小
        // アスペクト比を保持するため、width のみ設定し height は auto
        if (naturalW > containerW || naturalH > viewH) {
            var scaleW = containerW / naturalW;
            var scaleH = viewH / naturalH;
            var scale = Math.min(scaleW, scaleH);
            if (scale < 1) {
                img.style.width = Math.floor(naturalW * scale) + "px";
                img.style.height = "auto";
            }
        }
    }
}

window.addEventListener("load", adjustImages);
window.addEventListener("resize", adjustImages);
