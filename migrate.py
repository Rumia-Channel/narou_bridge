import argparse

def ver_check(_json, ver):
    if "version" in _json and int(_json["version"]) >= int(ver):
        return True
    else:
        return False

#0.0.1からの移行
def migrate_0_0_1():
    import util
    import os
    import json

    mv = 2

    user_json = {}

    config, reload_time, auto_update, save_log, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, key, use_ssl, ssl_crt, ssl_key, port, domain, use_proxy, proxy_port, proxy_ssl = util.load_config()

    for root, dirs, files in os.walk(data_path):

        #user.json

        if "user.json" in files:
            with open(os.path.join(root, "user.json"), "r", encoding="utf-8") as f:
                user_json = json.load(f)

            if ver_check(user_json, mv):
                continue

            new_user_json = {}

            new_user_json["version"] = mv
            for key, value in user_json.items():
                new_user_json[key] = {}
                new_user_json[key]["novel"] = value
                new_user_json[key]["comic"] = "disable"

            with open(os.path.join(root, "user.json"), "w", encoding="utf-8") as f:
                json.dump(new_user_json, f, ensure_ascii=False, indent=4)

        #raw.json

        if "raw.json" in files:
            with open(os.path.join(root, "raw.json"), "r", encoding="utf-8") as f:
                raw_json = json.load(f)

            if ver_check(raw_json, mv):
                if raw_json["serialization"] == "novel":
                    print(root)
                    if str(os.path.join("pixiv", "s")) in root:
                        raw_json["serialization"] = "連載中"
                    elif str(os.path.join("pixiv", "n")) in root:
                        raw_json["serialization"] = "短編"
                    
                    with open(os.path.join(root, "raw.json"), "w", encoding="utf-8") as f:
                        json.dump(raw_json, f, ensure_ascii=False, indent=4)
                continue

            new_raw_json = {}
            new_raw_json = raw_json
            new_raw_json["version"] = mv
            new_raw_json["serialization"] = raw_json["type"]
            new_raw_json["type"] = "novel"

            with open(os.path.join(root, "raw.json"), "w", encoding="utf-8") as f:
                json.dump(new_raw_json, f, ensure_ascii=False, indent=4)

    print("Migration 0.0.1 completed")
            
#0.0.3からの移行
def migrate_0_0_3():
    import util
    import os
    import re
    import json
    import shutil
    import hashlib
    import base64
    from tqdm import tqdm

    mv = 3

    config, reload_time, auto_update, save_log, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, key, use_ssl, ssl_crt, ssl_key, port, domain, use_proxy, proxy_port, proxy_ssl = util.load_config()

    if not os.path.exists(os.path.join(data_path, "images")):
        os.makedirs(os.path.join(data_path, "images"))

    if os.path.exists(os.path.join(data_path, "images", "database.json")):
        with open(os.path.join(data_path, "images", "database.json"), "r", encoding="utf-8") as f:
            database_json = json.load(f)
    else:
        database_json = {}

    total_files = 0
    for root, dirs, files in os.walk(data_path):
        total_files += 1

    for root, dirs, files in tqdm(os.walk(data_path), total=total_files):
        #user.json

        if "user.json" in files:
            with open(os.path.join(root, "user.json"), "r", encoding="utf-8") as f:
                user_json = json.load(f)

            if ver_check(user_json, mv):
                continue

            if not ver_check(user_json, mv-1):
                migrate_0_0_1()

            new_user_json = user_json

            new_user_json["version"] = mv

            with open(os.path.join(root, "user.json"), "w", encoding="utf-8") as f:
                json.dump(new_user_json, f, ensure_ascii=False, indent=4)

        #raw.json

        if "raw.json" in files:
            with open(os.path.join(root, "raw.json"), "r", encoding="utf-8") as f:
                raw_json = json.load(f)

            if ver_check(raw_json, mv):
                continue

            if not ver_check(user_json, mv-1):
                migrate_0_0_1()

            new_raw_json = {}
            new_raw_json = raw_json
            new_raw_json["version"] = mv

            image_hash = ''

            for id, value in new_raw_json['episodes'].items():

                pattern = r'\[image\]\((.*?)\)'

                if new_raw_json['serialization'] == "短編":
                    image_file_folder = os.path.dirname(root)
                else:
                    image_file_folder = os.path.join(os.path.dirname(root), str(value['id']))

                image_files = re.findall(pattern, value['text'])

                for image_file in image_files:
                    
                    if image_file in database_json.keys():
                        continue

                    if 'pixiv' in root:
                        if '_' in image_file:
                            if 'pixiv_' in image_file:
                                new_image_file = image_file
                            else:
                                new_image_file = 'pixiv_' + str(image_file)
                        else:
                            if any(char.isdigit() == False for char in image_file.split('.')[0]):
                                for k, v in database_json.items():
                                    if v == image_file.split('.')[0]:
                                        new_image_file = k
                                        break
                                with open(os.path.join(data_path, 'images', image_file), "rb") as f:
                                    image_data = f.read()
                                    image_hash = base64.urlsafe_b64encode(hashlib.sha3_256(image_data).digest()).rstrip(b'=').decode('utf-8')
                                    database_json[new_image_file] = str(image_hash)
                            else:
                                new_image_file = f'pixiv_{value['id']}_' + str(image_file)
                    else:
                        site_id = os.path.dirname(os.path.dirname(root)).reprace(data_path, '')
                        new_image_file = f'{site_id}_{image_file}'

                    if os.path.exists(os.path.join(image_file_folder, image_file)):
                        with open(os.path.join(image_file_folder, image_file), "rb") as f:
                            image_data = f.read()
                            image_hash = base64.urlsafe_b64encode(hashlib.sha3_256(image_data).digest()).rstrip(b'=').decode('utf-8')
                        flag = True
                    else:
                        flag = False

                    if new_image_file in database_json:
                        if os.path.exists(os.path.join(image_file_folder, image_file)):
                            os.remove(os.path.join(image_file_folder, image_file))
                    elif new_image_file not in database_json and image_hash in database_json.values():
                        database_json[new_image_file] = str(image_hash)
                        if os.path.exists(os.path.join(image_file_folder, image_file)):
                            os.remove(os.path.join(image_file_folder, image_file))
                    else:
                        database_json[new_image_file] = str(image_hash)
                        if flag:
                            shutil.move(os.path.join(image_file_folder, image_file), os.path.join(data_path, "images", f"{image_hash}{os.path.splitext(image_file)[1]}"))

                    new_raw_json['episodes'][id]['text'] = new_raw_json['episodes'][id]['text'].replace(f"[image]({image_file})", f"[image]({image_hash}{os.path.splitext(new_image_file)[1]})")

            with open(os.path.join(root, "raw.json"), "w", encoding="utf-8") as f:
                json.dump(new_raw_json, f, ensure_ascii=False, indent=4)

    with open(os.path.join(data_path, "images", "database.json"), "w", encoding="utf-8") as f:
        json.dump(database_json, f, ensure_ascii=False, indent=4)

    print("Migration 0.0.3 completed")

def migrate_0_0_3_c():
    import util
    import json
    import os
    from tqdm import tqdm

    mv = 3

    config, reload_time, auto_update, save_log, interval, auto_update_interval, site_dic, login_dic, folder_path, data_path, cookie_path, log_path, queue_path, pdf_path, key, use_ssl, ssl_crt, ssl_key, port, domain, use_proxy, proxy_port, proxy_ssl = util.load_config()

    total_files = 0
    for root, dirs, files in os.walk(data_path):
        total_files += 1

    for root, dirs, files in tqdm(os.walk(data_path), total=total_files):
        
        if "user.json" in files:
            with open(os.path.join(root, "user.json"), "r", encoding="utf-8") as f:
                user_json = json.load(f)

            if ver_check(user_json, mv):
                if user_json["version"] == 3:
                    for key, value in user_json.items():
                        user_json[key]["comic"] = "enable"
            else:
                migrate_0_0_3()

def main():
    parser = argparse.ArgumentParser(description="Migrate with version argument")
    parser.add_argument("version", help="Specify a version number like 0.0.1")
    args = parser.parse_args()

    if args.version == "0.0.1":
        migrate_0_0_1()
    elif args.version == "0.0.3":
        migrate_0_0_3()
    elif args.version == "0.0.3-c":
        migrate_0_0_3_c()
    else:
        print(f"No migration defined for version {args.version}")

if __name__ == "__main__":
    main()