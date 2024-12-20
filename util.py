import os
import importlib
import configparser
import json
from datetime import datetime

def init_import(site_dic):
    globals().update(import_modules(site_dic))

#初期設定の読み込み
def load_config():
    site_dic = {}
    login_dic = {}

    folder_path = {}
    cookie_path = {}

    # 設定の読み込み
    config = configparser.ConfigParser()
    config.read('setting.ini')

    # Get the path from the data key
    data_path = config['setting']['data']

    # 指定されないならカレントディレクトリ
    if not data_path:
        data_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'data')

    #Cookieの保存先を指定
    cookie_folder = config['setting']['cookie']

    # 指定されないならカレントディレクトリ
    if not cookie_folder:
        cookie_folder = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'cookie')


    # ないなら作れdataフォルダ
    if not os.path.exists(data_path):
        os.makedirs(data_path)

    # dataフォルダcookieフォルダとサイト名のマトリョシカを作成
    for key in config['crawler']:
        folder_name = key
        site_dic[key] = config['crawler'][key]
        login_dic[key] = int(config['login'][key])
        folder_path[key] = os.path.join(data_path, folder_name)
        cookie_path[key] = os.path.join(cookie_folder, folder_name)
        if not os.path.exists(folder_path[key]):
            os.makedirs(folder_path[key])
        if not os.path.exists(cookie_path[key]):
            os.makedirs(cookie_path[key])

    print("Initialize successfully!")
    return config, int(config['setting']['reload']), int(config['setting']['interval']), site_dic, login_dic, folder_path, data_path, cookie_path, int(config['server']['key']), int(config['server']['ssl']), int(config['server']['port']), config['server']['domain']

# clawler　フォルダ内のモジュールをインポート
def import_modules(site_dic):
    modules = {}
    for site_key, value in site_dic.items():
        module_name = 'crawler.' + value.replace('.py', '')
        modules[site_key] = importlib.import_module(module_name)
    return modules

#小説の更新処理
def update(update_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):

    if not update_param == 'all':
        # 更新処理
        for site_key, value in site_dic.items():
            if update_param == site_key:
                site = site_key
                if int(login_dic[site])==0|1:
                    is_login = int(login_dic[site])
                else:
                    return 400
                
                break
        else:
            return 400
        
        print(f'Update: {site}\n')
        globals()[site].init(cookie_path[site], is_login, interval)
        globals()[site].update(folder_path[site], key_data, data_path, host_name)

    else:
        # 全更新処理
        for site_key, value in site_dic.items():
            site = site_key
            if int(login_dic[site])==0|1:
                is_login = int(login_dic[site])
            else:
                return 400
            
            print(f'Update: {site}\n')
            globals()[site].init(cookie_path[site], is_login, interval)
            globals()[site].update(folder_path[site], key_data, data_path, host_name)

#小説の再ダウンロード処理
def re_download(re_download_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):

    if not re_download_param == 'all':
        # 更新処理
        for site_key, value in site_dic.items():
            if value.replace('_', '.').replace('.py', '') in re_download_param:
                if re_download_param == site_key:
                    site = site_key
                    if int(login_dic[site])==0|1:
                        is_login = int(login_dic[site])
                    else:
                        return 400
                    
                    break
            else:
                return 400
        else:
            return 400

        print(f'Re Download: {site}\n')
        globals()[site].init(cookie_path[site], is_login, interval)
        globals()[site].re_download(folder_path[site], key_data, data_path, host_name)

    else:
        # 全更新処理
        for site_key, value in site_dic.items():
            site = site_key
            if int(login_dic[site])==0|1:
                is_login = int(login_dic[site])
            else:
                return 400
            
            print(f'Re Download: {site}\n')
            globals()[site].init(cookie_path[site], is_login, interval)
            globals()[site].re_download(folder_path[site], key_data, data_path, host_name)

#小説の変換処理
def convert(convert_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    
    if not convert_param == 'all':
        # 変換処理
        for site_key, value in site_dic.items():
            if convert_param == site_key:
                site = site_key
                if int(login_dic[site])==0|1:
                    is_login = int(login_dic[site])
                else:
                    return 400

                break
        else:
            return 400

        print(f'Convert: {site}\n')
        globals()[site].init(cookie_path[site], is_login, interval)
        globals()[site].convert(folder_path[site], key_data, data_path, host_name)

    else:
        # 全変換処理
        for site_key, value in site_dic.items():
            site = site_key
            if int(login_dic[site])==0|1:
                is_login = int(login_dic[site])
            else:
                return 400

            print(f'Convert: {site}\n')
            globals()[site].init(cookie_path[site], is_login, interval)
            globals()[site].convert(folder_path[site], key_data, data_path, host_name)

#小説のダウンロード処理
def download(add_param, site_dic, login_dic, folder_path, data_path, cookie_path, key_data, interval, host_name):
    # webサイトの判別
    for site_key, value in site_dic.items():
        if value.replace('_', '.').replace('.py', '') in add_param:
            site = site_key
            if int(login_dic[site])==0|1:
                is_login = int(login_dic[site])
            else:
                return 400
            break
    else:
        if int(login_dic[site])==0|1:
            is_login = int(login_dic[site])
        else:
            return 400

    print(f'Web site: {site}')
    print(f'URL: {add_param}')
    globals()[site].init(cookie_path[site], is_login, interval)
    globals()[site].download(add_param, folder_path[site], key_data, data_path, host_name)

#リクエストIDの削除
def cleanup_expired_requests(requests_dict, expiration_time):
    current_time = datetime.now()
    for key in list(requests_dict):
        if (current_time - requests_dict[key]).total_seconds() > expiration_time:
            del requests_dict[key]
    
    return requests_dict