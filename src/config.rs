use crate::{
    vorerr::VORAppError,
    vorutils::{file_exists, get_user_home_dir, path_exists}, pf::PacketFilter,
};
use core::fmt;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone)]
pub struct VORConfigWrapper {
    pub config_data: VORConfig,
    pub config_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VORConfig {
    pub app_port: String,
    pub app_host: String,
    //pub bind_port: String,
    //pub bind_host: String,
    pub app_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RouterConfig {
    pub bind_host: String,
    pub bind_port: String,
    //pub vrc_host: String,
    //pub vrc_port: String,
    pub vor_buffer_size: String,
    pub async_mode: bool,
}

impl Default for RouterConfig {
    fn default() -> Self {
        RouterConfig {
            bind_host: "127.0.0.1".to_string(),
            bind_port: "9001".to_string(),
            //vrc_host: "127.0.0.1".to_string(),
            //vrc_port: "9000".to_string(),
            vor_buffer_size: "4096".to_string(),
            async_mode: true,
        }
    }
}

pub struct VORAppIdentifier {
    pub index: i64,
    pub status: VORAppStatus,
}

#[derive(Clone)]
pub enum AppConfigState {
    EDIT(AppConfigCheck),
    SAVED,
}

pub enum VORAppStatus {
    Disabled,
    Stopped,
    Running,
    AppError(VORAppError),
}

impl fmt::Display for VORAppStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VORAppStatus::Disabled => write!(f, "Disabled"),
            VORAppStatus::Stopped => write!(f, "Stopped"),
            VORAppStatus::Running => write!(f, "Running"),
            VORAppStatus::AppError(e) => write!(f, "{}: {}", e.msg, e.id),
        }
    }
}

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

#[allow(warnings)]
#[derive(Clone)]
pub enum InputValidation {
    AP(bool),
    AH(bool),
    BP(bool),
    BH(bool),
    CLEAN,
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

fn read_configs() -> (RouterConfig, Vec<VORConfigWrapper>, PacketFilter) {
    let mut configs = Vec::<VORConfigWrapper>::new();

    #[cfg(target_os = "linux")]
    let vor_root_dir = format!("{}/.vor", get_user_home_dir());

    #[cfg(target_os = "windows")]
    let vor_root_dir = format!(
        "{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR",
        get_user_home_dir()
    );

    let vor_config_file;
    let vor_app_configs_dir;
    let vor_pf_config_file;

    #[cfg(target_os = "windows")]
    {
        vor_config_file = format!("{}\\VORConfig.json", vor_root_dir);
        vor_app_configs_dir = format!("{}\\VORAppConfigs", vor_root_dir);
        vor_pf_config_file = format!("{}\\VOR_PF.json", vor_root_dir);
    }

    #[cfg(target_os = "linux")]
    {
        vor_config_file = format!("{}/VORConfig.json", vor_root_dir);
        vor_app_configs_dir = format!("{}/VORAppConfigs", vor_root_dir);
        vor_pf_config_file = format!("{}/VOR_PF.json", vor_root_dir);
    }

    //If vor & vor config folder doesnt exist make it
    if !path_exists(&vor_root_dir) {
        fs::create_dir_all(&vor_root_dir).expect("[-] Cannot create VOR root directory.");
        //println!("[+] Created VOR root directory.")
    } else {
        //println!("[*] VOR root directory exists.");
    }

    if !path_exists(&vor_app_configs_dir) {
        fs::create_dir(&vor_app_configs_dir).expect("[-] Cannot create VOR configs directory.");
        //println!("[+] Created VOR configs directory.");
    } else {
        //println!("[*] VOR configs directory exists.");
    }

    //Generate Default VOR config if not exist.
    if !file_exists(&vor_config_file) {
        fs::write(
            &vor_config_file,
            serde_json::to_string(&RouterConfig::default()).unwrap(),
        )
        .unwrap();
        //println!("[+] Created VOR router config.");
    } else {
        //println!("[*] VOR router config exists.");
    }

    // Generate Default PacketFilter config if not exist
    if !file_exists(&vor_pf_config_file) {
        fs::write(
            &vor_pf_config_file,
            serde_json::to_string(&PacketFilter {
                enabled: false,
                filter_bad_packets: false,
                wl_enabled: false,
                address_wl: vec![],
                bl_enabled: false,
                address_bl: vec![],
            })
            .unwrap(),
        )
        .unwrap();
        //println!("[+] Created VOR PF config.")
    } else {
        //println!("[*] VOR PF config exists.");
    }

    // Read VOR config
    let router_config = match fs::read_to_string(&vor_config_file) {
        Ok(c) => {
            match serde_json::from_str(&c) {
                Ok(c) => c,
                Err(_e) => {
                    //println!("[-] Failed to parse json from file: {} [{}]", vor_config_file, _e);

                    // Overwrite configs when fail to parse
                    fs::write(
                        &vor_config_file,
                        serde_json::to_string(&RouterConfig::default()).unwrap(),
                    )
                    .unwrap();
                    //println!("[+] Created VOR router config.");
                    RouterConfig::default()
                }
            }
        }
        Err(_e) => {
            //println!("[-] Could not parse bytes from file: {} [{}].. Generating and writing default..", vor_config_file, _e);
            // Overwrite configs when fail to parse
            fs::write(
                &vor_config_file,
                serde_json::to_string(&RouterConfig::default()).unwrap(),
            )
            .unwrap();
            //println!("[+] Created VOR router config.");
            RouterConfig::default()
        }
    };

    // Read VOR PF config
    let file_con = match fs::read_to_string(&vor_pf_config_file) {
        Ok(c) => c,
        Err(_e) => {
            //println!("[-] Could not parse bytes from file: {} [{}].. Skipping..", vor_pf_config_file, _e);
            std::process::exit(0);
        }
    };

    let pf = match serde_json::from_str(&file_con) {
        Ok(c) => c,
        Err(_e) => {
            //println!("[-] Failed to parse json from file: {} [{}]", vor_pf_config_file, _e);
            std::process::exit(0);
        }
    };

    // Read configs from folder
    let config_files =
        fs::read_dir(&vor_app_configs_dir).expect("[-] Could not read VOR configs directory.");
    for f in config_files {
        let file = f.unwrap();
        if file.file_type().unwrap().is_file() {
            //let file_n = file.file_name().to_str().expect("[-] Failed to parse file name.").to_string();
            let file_p = file
                .path()
                .as_os_str()
                .to_str()
                .expect("[-] Failed to parse file path.")
                .to_string();

            let file_con = match fs::read_to_string(&file_p) {
                Ok(c) => c,
                Err(_e) => {
                    //println!("[-] Could not parse bytes from file: {} [{}].. Skipping..", file_n, _e);
                    continue;
                }
            };
            match serde_json::from_str(&file_con) {
                Ok(c) => configs.push(VORConfigWrapper {
                    config_data: c,
                    config_path: file_p,
                }),
                Err(_e) => {
                    //println!("[-] Failed to parse json from file: {} [{}]", file_n, _e);
                    continue;
                }
            };
        }
    }
    (router_config, configs, pf)
}

pub fn config_construct() -> (
    RouterConfig,
    Vec<(VORConfigWrapper, VORAppStatus, AppConfigState)>,
    PacketFilter,
) {
    let (vor_router_config, configs, pf) = read_configs();
    /*
    if configs.len() < 1 {
        //println!("[?] Please put OSC application VOR configs in the [\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VORAppConfigs] directory.");
    } else {
        for c in &configs {
            //println!("[App]: {}\n [*] Route -> {}:{}", c.config_data.app_name, c.config_data.app_host, c.config_data.app_port);
        }
    }*/

    let mut gconfs = vec![];
    for c in configs {
        gconfs.push((c, VORAppStatus::Stopped, AppConfigState::SAVED));
    }
    return (vor_router_config, gconfs, pf);
}
