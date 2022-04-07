//#![windows_subsystem = "windows"]

use eframe::NativeOptions;
use eframe::egui::Vec2;
use serde_json;
use serde::{Deserialize, Serialize};
use directories::BaseDirs;
use core::fmt;
use std::path::Path;
use std::fs;
use std::net::Ipv4Addr;
use eframe::run_native;

mod ui;
mod routing;

use ui::VORGUI;
use routing::RouterConfig;

#[derive(Clone)]
pub enum AppConfigCheck {
    IV(InputValidation),
    AC(AppConflicts),
    SUCCESS,
}

#[derive(Clone)]
pub enum AppConflicts {
    NONE,
    CONFLICT((String, String)),
}

#[derive(Clone)]
pub enum InputValidation {
    AP(bool),
    AH(bool),
    BP(bool),
    BH(bool),
    CLEAN,
}

pub struct VORConfigWrapper {
    config_data: VORConfig,
    config_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VORConfig {
    app_port: String,
    app_host: String,
    bind_port: String,
    bind_host: String,
    app_name: String,
}

pub struct VORAppError {
    id: i32,
    msg: String,

}

pub enum VORAppStatus {
    Stopped,
    Running,
    AppError(VORAppError),
}

impl fmt::Display for VORAppStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VORAppStatus::Stopped => write!(f, "Stopped"),
            VORAppStatus::Running => write!(f, "Running"),
            VORAppStatus::AppError(e) => write!(f, "{}: {}", e.msg, e.id),
        }
    }
}

impl fmt::Display for AppConflicts {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppConflicts::CONFLICT((app, con_comp)) => write!(f, "{} -> {}", app, con_comp),
            AppConflicts::NONE => write!(f, "NONE"),
        }
    }
}

impl fmt::Display for InputValidation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            InputValidation::AH(_b) => write!(f, "App host: Invalid input."),
            InputValidation::AP(_b) => write!(f, "App port: Invalid input."),
            InputValidation::BH(_b) => write!(f, "Bind Host: Invalid input."),
            InputValidation::BP(_b) => write!(f, "Bind Port: Invalid input."),
            InputValidation::CLEAN => write!(f, "CLEAN"),
        }
    }
}

pub struct VORAppIdentifier {
    index: i64,
    status: VORAppStatus,
}

#[derive(Clone)]
pub enum AppConfigState {
    EDIT(AppConfigCheck),
    SAVED,
}

fn app_error(ai: i64, err_id: i32, msg: String) -> VORAppIdentifier {
    VORAppIdentifier {
        index: ai,
        status: VORAppStatus::AppError(VORAppError{id: err_id, msg}),
    }
}

fn check_valid_port(port: &String) -> bool {
    if let Ok(p) = port.parse::<u64>() {
        if p > 0 && p < 65535 {
            true
        } else {
            false
        }
    } else {
        false
    }
}

fn check_valid_ipv4(ip: &String) -> bool {

    if ip.parse::<Ipv4Addr>().is_err() {
        false
    } else {
        true
    }
}

fn path_exists(p: &String) -> bool {
    Path::new(&p).is_dir()
}

fn file_exists(p: &String) -> bool {
    Path::new(&p).is_file()
}

fn get_user_home_dir() -> String {
    let bd = BaseDirs::new().expect("[-] Could not get user's directories.");
    let bd = bd.home_dir().to_str().expect("[-] Failed to get user's home directory.");
    bd.to_string()
}

fn read_configs() -> (RouterConfig, Vec<VORConfigWrapper>) {
    
    let mut configs = Vec::<VORConfigWrapper>::new();
    let vor_root_dir = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR", get_user_home_dir());
    let vor_config_file = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VORConfig.json", get_user_home_dir());
    let vor_app_configs_dir = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VORAppConfigs", get_user_home_dir());

    //If vor & vor config folder doesnt exist make it
    if !path_exists(&vor_root_dir) {
        fs::create_dir_all(&vor_root_dir).expect("[-] Cannot create VOR root directory.");
        println!("[+] Created VOR root directory.")
    } else {
        println!("[*] VOR root directory exists.");
    }

    if !path_exists(&vor_app_configs_dir) {
        fs::create_dir(&vor_app_configs_dir).expect("[-] Cannot create VOR configs directory.");
        println!("[+] Created VOR configs directory.");
    } else {
        println!("[*] VOR configs directory exists.");
    }

    //Generate Default VOR config if not exist.
    if !file_exists(&vor_config_file) {
        fs::write(&vor_config_file, serde_json::to_string(
            &RouterConfig {
                bind_host: "127.0.0.1".to_string(),
                bind_port: "9001".to_string(),
                vrc_host: "127.0.0.1".to_string(),
                vrc_port: "9000".to_string(),
                vor_buffer_size: "1024".to_string(),
            }
        ).unwrap()).unwrap();
        println!("[+] Created VOR router config.");
    } else {
        println!("[*] VOR router config exists.");
    }

    let file_con = match fs::read_to_string(&vor_config_file) {
        Ok(c) => c,
        Err(_e) => {
            println!("[-] Could not parse bytes from file: {} [{}].. Skipping..", vor_config_file, _e);
            std::process::exit(0);
        }
    };

    let router_config = match serde_json::from_str(&file_con) {
        Ok(c) => c,
        Err(_e) => {
            println!("[-] Failed to parse json from file: {} [{}]", vor_config_file, _e);
            std::process::exit(0);
        }
    };

    // Read configs from folder
    let config_files = fs::read_dir(&vor_app_configs_dir).expect("[-] Could not read VOR configs directory.");
    for f in config_files {
        let file = f.unwrap();
        if file.file_type().unwrap().is_file() {

            let file_n = file.file_name().to_str().expect("[-] Failed to parse file name.").to_string();
            let file_p = file.path().as_os_str().to_str().expect("[-] Failed to parse file path.").to_string();

            let file_con = match fs::read_to_string(&file_p) {
                Ok(c) => c,
                Err(_e) => {
                    println!("[-] Could not parse bytes from file: {} [{}].. Skipping..", file_n, _e);
                    continue;
                }
            };
            match serde_json::from_str(&file_con) {
                Ok(c) => configs.push(VORConfigWrapper{config_data: c, config_path: file_p}),
                Err(_e) => {
                    println!("[-] Failed to parse json from file: {} [{}]", file_n, _e);
                    continue;
                }
            };
        }
    }
    (router_config, configs)
}

fn config_construct() -> (RouterConfig, Vec<(VORConfigWrapper, VORAppStatus, AppConfigState)>) {
    let (vor_router_config, configs) = read_configs();
    if configs.len() < 1 {
        println!("[?] Please put OSC application VOR configs in the [\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VORAppConfigs] directory.");
    } else {
        for c in &configs {
            println!("[App]: {}\n [*] Route -> {}:{}", c.config_data.app_name, c.config_data.app_host, c.config_data.app_port);
        }
    }

    let mut gconfs = vec![];
    for c in configs {
        gconfs.push((c, VORAppStatus::Stopped, AppConfigState::SAVED));
    }
    return (vor_router_config, gconfs);
}

fn main() {

    let (vor_router_config, configs) = config_construct();

    let mut native_opts = NativeOptions::default();
    native_opts.initial_window_size = Some(Vec2::new(325., 450.));
    native_opts.max_window_size = Some(Vec2::new(325., 450.));
    native_opts.min_window_size = Some(Vec2::new(325., 450.));

    run_native(
        Box::new(
            VORGUI::new(configs, vor_router_config)), native_opts);
}