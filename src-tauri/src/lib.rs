mod adb;
mod cmd;
mod java;
mod package;
mod tools;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            adb::find_adb,
            adb::get_devices,
            adb::install_apk,
            adb::install_aab,
            adb::extract_apk_from_aab,
            adb::launch_app,
            adb::uninstall_app,
            adb::stop_app,
            adb::list_packages,
            package::get_package_name,
            package::get_aab_package_name,
            java::check_java,
            java::find_bundletool,
            java::list_key_aliases,
            cmd::set_cancel_flag,
            tools::status::get_tools_status,
            tools::download::setup_platform_tools,
            tools::download::setup_bundletool,
            tools::download::setup_java,
            tools::status::check_for_stale_tools,
            tools::recent::get_recent_files,
            tools::recent::add_recent_file,
            tools::recent::remove_recent_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
