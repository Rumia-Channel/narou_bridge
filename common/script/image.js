function adjustImages() {
    var viewW = window.innerWidth;
    var viewH = window.innerHeight;
    var images = document.querySelectorAll("img");
    
    // WebKit compatibility: use for loop instead of forEach
    for (var i = 0; i < images.length; i++) {
        var img = images[i];
        
        // 一度元のサイズに戻す
        img.style.width = "";
        img.style.height = "";
        if (!img.naturalWidth || !img.naturalHeight) continue;

        // 画像の自然サイズ
        var naturalW = img.naturalWidth;
        var naturalH = img.naturalHeight;

        // ウィンドウより大きい場合のみ縮小率を計算
        if (naturalW > viewW || naturalH > viewH) {
            var scaleW = viewW / naturalW;
            var scaleH = viewH / naturalH;
            var scale = Math.min(scaleW, scaleH);
            if (scale < 1) {
                img.style.width = (naturalW * scale) * 0.99 + "px";
                img.style.height = (naturalH * scale) * 0.99 + "px";
            }
        }
    }
}

window.addEventListener("load", adjustImages);
window.addEventListener("resize", adjustImages);
