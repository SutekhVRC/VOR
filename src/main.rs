use rosc::{self};
use rosc::decoder::MTU;
use serde_json;
use serde::{Deserialize, Serialize};
use directories::BaseDirs;
use std::sync::mpsc::{self, Sender, Receiver};

//use std::fmt::format;
use std::path::Path;
use std::{fs, thread};
use std::net::UdpSocket;
use std::time::Duration;

/*
    Filter bad OSC packets?
    Should there be filter modes?
    Or just forward everything?
    Filter modes could make things faster possibly?
*/


#[derive(Debug, Serialize, Deserialize)]
struct VORConfig {
    app_port: u32,
    app_host: String,
    bind_port: u32,
    bind_host: String,
    app_name: String,
}

fn path_exists(p: &String) -> bool {
    Path::new(&p).is_dir()
}

fn get_user_home_dir() -> String {
    let bd = BaseDirs::new().expect("[-] Could not get user's directories.");
    let bd = bd.home_dir().to_str().expect("[-] Failed to get user's home directory.");
    bd.to_string()
}

fn read_configs() -> Vec<VORConfig> {
    
    let mut configs = Vec::<VORConfig>::new();
    let vor_root_dir = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR", get_user_home_dir());
    let vor_config_dir = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\Configs", get_user_home_dir());

    //If vor & vor config folder doesnt exist make it
    if !path_exists(&vor_root_dir) {
        fs::create_dir(&vor_root_dir).expect("[-] Cannot create VOR root directory.");
        println!("[+] Created VOR root directory.")
    } else {
        println!("[*] VOR root directory exists.");
    }

    if !path_exists(&vor_config_dir) {
        fs::create_dir(&vor_config_dir).expect("[-] Cannot create VOR configs directory.");
        println!("[+] Created VOR configs directory.")
    } else {
        println!("[*] VOR configs directory exists.");
    }

    // Read configs from folder
    let config_files = fs::read_dir(&vor_config_dir).expect("[-] Could not read VOR configs directory.");
    for f in config_files {
        let file = f.unwrap();
        if file.file_type().unwrap().is_file() {
            let file_n = file.file_name().to_str().expect("[-] Failed to parse file name.").to_string();
            let file_p = file.path().as_os_str().to_str().expect("[-] Failed to parse file path.").to_string();
            //println!("[+] Got config: {}\n\t[PATH] {}", file.file_name().to_str().unwrap(), file.path().as_os_str().to_str().unwrap());

            let file_con = match fs::read_to_string(&file_p) {
                Ok(c) => c,
                Err(_e) => {
                    println!("[-] Could not parse bytes from file: {} [{}].. Skipping..", file_n, _e);
                    continue;
                }
            };
            match serde_json::from_str(&file_con) {
                Ok(c) => configs.push(c),
                Err(_e) => {
                    println!("[-] Failed to parse json from file: {} [{}]", file_n, _e);
                    continue;
                }
            };
        }
    }
    //println!("{:?}", configs);
    configs


    // Parse each config and start each


}

/*
fn w_conf() {
    let p = Path::new("./blorp.json");
    fs::write(p, serde_json::to_string(&VORConfig{
        rport: 9003,
        rhost: "127.0.0.1".to_string(),
        lport: 9002,
        lhost: "127.0.0.1".to_string(),
        app_name: "SutekhSpotify".to_string(),
    }).unwrap());
}
*/

fn main() {
    //w_conf();

    let configs = read_configs();
    if configs.len() < 1 {
        println!("[?] Please put OSC application VOR configs in the [\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\Configs] directory.");
        println!("[-] No VOR configs found. Shutting down..");
        return;
    } else {
        for c in &configs {
            println!("[Load App]: {}\n\t[*] Route -> {}:{}", c.app_name, c.app_host, c.app_port);
        }
    }

    let vrc_sock = UdpSocket::bind("127.0.0.1:9001").unwrap();
    let mut channel_vector: Vec<Sender<Vec<u8>>> = Vec::new();
    for app in configs {
        let (tx, rx) = mpsc::channel();
        channel_vector.push(tx);
        thread::spawn(move || {
            route_app(rx, app);
        });
    }

    println!("[*] Wait 3 seconds for listener channels..");
    thread::sleep(Duration::from_secs(3));
    println!("[+] Started VRChat OSC Router.");
    parse_vrc_osc(channel_vector, vrc_sock);
}

fn parse_vrc_osc(tx: Vec<Sender<Vec<u8>>>, vrc_sock: UdpSocket) {
    let mut buf = [0u8; MTU];
    loop {
        let (br, _a) = vrc_sock.recv_from(&mut buf).unwrap();
        if br <= 0 {
            continue;
        } else {
            /* Filtering
            match rosc::decoder::decode(&mut buf) {
                Ok(pkt) => { if let OscPacket::Message(msg) = pkt { tx.send(pkt); }},
                Err(_e) => continue,
            }
            */
            for s in &tx {
                s.send(buf.to_vec()).unwrap_or_else(|_| {println!("[-] No rx listening.")});
            }
        }
    }
}


fn route_app(rx: Receiver<Vec<u8>>, app: VORConfig) {
    let rhp = format!("{}:{}", app.app_host, app.app_port);
    let lhp = format!("{}:{}", app.bind_host, app.bind_port);
    let sock = UdpSocket::bind(lhp).unwrap();
    println!("[*] OSC App: [{}] Route Initialized..", app.app_name);
    loop {
        let buffer = rx.recv().unwrap();
        sock.send_to(&buffer, &rhp).unwrap();
    }
}