import os
import server

#共通設定の読み込み
import util

if __name__ == '__main__':
    # 設定の読み込み
    config, reload_time, auto_update, save_log, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, port, domain, use_proxy, proxy_port, proxy_ssl = util.load_config()

    # Indexファイルを作成
    util.create_index(data_path, config)

    # サーバー起動
    # server.http_run はキーワード引数 (**kwargs) で受け取る仕様になったため、
    # 全ての引数を「引数名=値」の形式で渡します。
    server.http_run(
        config=config,
        reload_time=reload_time,
        auto_update=auto_update,
        save_log=save_log,
        interval=interval,
        auto_update_interval=auto_update_interval,
        site_dic=site_dic,
        login_dic=login_dic,
        folder_path=folder_path,
        data_path=data_path,
        cookie_path=cookie_path,
        log_path=log_path,
        queue_path=queue_path,
        pdf_path=pdf_path,
        port=port,
        domain=domain,
        use_proxy=use_proxy,
        proxy_port=proxy_port,
        proxy_ssl=proxy_ssl
    )