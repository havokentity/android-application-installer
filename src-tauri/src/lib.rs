mod adb;
mod cmd;
mod java;
mod package;
mod tools;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(tokio::sync::Mutex::new(adb::DeviceTracker::default()))
        .invoke_handler(tauri::generate_handler![
            adb::find_adb,
            adb::get_devices,
            adb::start_device_tracking,
            adb::stop_device_tracking,
            adb::adb_pair,
            adb::adb_connect,
            adb::adb_disconnect,
            adb::adb_mdns_check,
            adb::adb_mdns_services,
            adb::install_apk,
            adb::install_aab,
            adb::extract_apk_from_aab,
            adb::launch_app,
            adb::uninstall_app,
            adb::stop_app,
            adb::list_packages,
            package::get_package_name,
            package::get_aab_package_name,
            package::get_file_size,
            package::get_apk_metadata,
            package::get_aab_metadata,
            java::check_java,
            java::find_bundletool,
            java::list_key_aliases,
            cmd::set_cancel_flag,
            cmd::save_text_file,
            cmd::send_notification,
            tools::status::get_tools_status,
            tools::download::setup_platform_tools,
            tools::download::setup_bundletool,
            tools::download::setup_java,
            tools::status::check_for_stale_tools,
            tools::recent::get_recent_files,
            tools::recent::add_recent_file,
            tools::recent::remove_recent_file,
            tools::profiles::get_signing_profiles,
            tools::profiles::save_signing_profile,
            tools::profiles::delete_signing_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
