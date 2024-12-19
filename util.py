import os
import importlib
import configparser
import json
from datetime import datetime

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

#リクエストIDのクリーンアップ
def cleanup_expired_requests(requests_dict, expiration_time):
    """期限切れのリクエストIDをクリーンアップ"""
    current_time = datetime.now()
    for key in list(requests_dict):
        if (current_time - requests_dict[key]).total_seconds() > expiration_time:
            del requests_dict[key]