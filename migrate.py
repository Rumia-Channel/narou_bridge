import argparse

def ver_check(_json, ver):
    if "version" in _json and int(_json["version"]) >= int(ver):
        return True
    else:
        return False

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

            new_user_json["version"] = 2
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
            new_raw_json["version"] = 2
            new_raw_json["serialization"] = raw_json["type"]
            new_raw_json["type"] = "novel"

            with open(os.path.join(root, "raw.json"), "w", encoding="utf-8") as f:
                json.dump(new_raw_json, f, ensure_ascii=False, indent=4)


        


    print("Migration 0.0.1 completed")
            

#def migrate_0_0_2():
#    print("Running migrate_0_0_2")

def main():
    parser = argparse.ArgumentParser(description="Migrate with version argument")
    parser.add_argument("version", help="Specify a version number like 0.0.1")
    args = parser.parse_args()

    if args.version == "0.0.1":
        migrate_0_0_1()
#    elif args.version == "0.0.2":
#        migrate_0_0_2()
    else:
        print(f"No migration defined for version {args.version}")

if __name__ == "__main__":
    main()