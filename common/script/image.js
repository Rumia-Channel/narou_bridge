function adjustImages() {
    const viewW = window.innerWidth;
    const viewH = window.innerHeight;
    document.querySelectorAll("img").forEach((img) => {
        // 一度元のサイズに戻す
        img.style.width = "";
        img.style.height = "";
        if (!img.naturalWidth || !img.naturalHeight) return;

        // 画像の自然サイズ
        const naturalW = img.naturalWidth;
        const naturalH = img.naturalHeight;

        // ウィンドウより大きい場合のみ縮小率を計算
        if (naturalW > viewW || naturalH > viewH) {
            const scaleW = viewW / naturalW;
            const scaleH = viewH / naturalH;
            const scale = Math.min(scaleW, scaleH);
            if (scale < 1) {
                img.style.width = (naturalW * scale) * 0.99 + "px";
                img.style.height = (naturalH * scale) * 0.99 + "px";
            }
        }
    });
}

window.addEventListener("load", adjustImages);
window.addEventListener("resize", adjustImages);
