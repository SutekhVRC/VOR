use directories::BaseDirs;
use std::net::Ipv4Addr;
use std::path::Path;

pub fn check_valid_port(port: &String) -> bool {
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

pub fn check_valid_ipv4(ip: &String) -> bool {
    if ip.parse::<Ipv4Addr>().is_err() {
        false
    } else {
        true
    }
}

pub fn path_exists(p: &String) -> bool {
    Path::new(&p).is_dir()
}

pub fn file_exists(p: &String) -> bool {
    Path::new(&p).is_file()
}

#[allow(unused)]
pub fn get_user_home_dir() -> String {
    let bd = BaseDirs::new().expect("[-] Could not get user's directories.");
    let bd = bd
        .home_dir()
        .to_str()
        .expect("[-] Failed to get user's home directory.");
    bd.to_string()
}
