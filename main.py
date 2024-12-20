import os
import server

#共通設定の読み込み
import util

if __name__ == '__main__':
    config, reload_time, interval, site_dic, login_dic, folder_path, data_path, cookie_path, key, use_ssl, port, domain = util.load_config()

    # Indexファイルを作成
    util.create_index(data_path, config)

    server.http_run(interval, site_dic, login_dic, folder_path, data_path, cookie_path,  key, use_ssl, port, domain)