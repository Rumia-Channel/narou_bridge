import os
import server

#共通設定の読み込み
import util

if __name__ == '__main__':
    config, reload_time, auto_update, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, key, use_ssl, ssl_crt, ssl_key, port, domain, use_proxy, proxy_port, proxy_ssl = util.load_config()

    # Indexファイルを作成
    util.create_index(data_path, config)

    server.http_run(config, reload_time, auto_update, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, key, use_ssl, ssl_crt, ssl_key, port, domain, use_proxy, proxy_port, proxy_ssl)