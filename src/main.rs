//#![windows_subsystem = "windows"]

use eframe::NativeOptions;
use eframe::egui::{Vec2, ScrollArea, Layout, RichText, TopBottomPanel, Hyperlink, Context, Label};
use eframe::epaint::Color32;
use rosc;
use rosc::decoder::MTU;
use serde_json;
use serde::{Deserialize, Serialize};
use directories::BaseDirs;
use tokio::runtime::Runtime;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::{self, Sender as bcst_Sender, Receiver as bcst_Receiver};
use core::fmt;
use std::sync::mpsc::{self, Sender, Receiver};

//use std::fmt::format;
use std::path::Path;
use std::{fs, thread};
use std::net::{UdpSocket};
//use std::time::Duration;

use eframe::{epi::{App}, egui::{self, CentralPanel}, run_native};

/*
    Filter bad OSC packets?
    Should there be filter modes?
    Or just forward everything?
    Filter modes could make things faster possibly?
*/
// Create structure for handling router threads maybe
enum RouterMsg {
    ShutdownAll,
    //RestartAll,
}

struct VORConfigWrapper {
    config_data: VORConfig,
    config_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct VORConfig {
    app_port: String,
    app_host: String,
    bind_port: String,
    bind_host: String,
    app_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct RouterConfig {
    bind_host: String,
    bind_port: String,
    vrc_host: String,
    vrc_port: String,
    vor_buffer_size: String,
}

/*
struct RouterError {
    id: i64,
    msg: String,
}

struct RouterStatus {
    vrc_osc_recv: bool,
    
}
*/
struct VORAppError {
    id: i32,
    msg: String,

}

enum VORAppStatus {
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

struct VORAppIdentifier {
    index: i64,
    status: VORAppStatus,
}

enum VORGUITab {
    Main,
    Apps,
    Config,
    Firewall,
}

struct VORGUI {
    configs: Vec<(VORConfigWrapper, VORAppStatus, bool)>,
    running: bool,
    tab: VORGUITab,
    router_channel: Option<Sender<RouterMsg>>,
    vor_router_config: RouterConfig,
    adding_new_app: bool,
    new_app: Option<VORConfigWrapper>,
    new_app_cf_exists_err: bool,
    router_msg_recvr: Option<Receiver<VORAppIdentifier>>,
    //vor_status: RouterStatus,
}

impl VORGUI {
    fn set_tab(&mut self, ctx: &Context) {

        TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.with_layout(Layout::left_to_right(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(RichText::new("Main").monospace()).clicked() {
                            self.tab = VORGUITab::Main;
                        }
                        ui.separator();
    
    
                        if ui.button(RichText::new("Apps").monospace()).clicked() {
                            self.tab = VORGUITab::Apps;
                        }
    
                        ui.separator();
    
                        
                        if ui.button(RichText::new("Firewall").monospace()).clicked() {
                            self.tab = VORGUITab::Firewall;
                        }
                        ui.separator();
    
                        if ui.button(RichText::new("Config").monospace()).clicked() {
                            self.tab = VORGUITab::Config;
                        }

                        ui.separator();
                        if self.running {
                            ui.label(RichText::new("Routing").color(Color32::GREEN));
                        } else {
                            ui.label(RichText::new("Stopped").color(Color32::RED));
                        }
                        //ui.separator();
                    });
                    ui.separator();
                });
            });
        });
    }

    fn status_refresh(&mut self) {
        let status = match self.router_msg_recvr.as_ref(){
            Some(recvr) => {
                match recvr.try_recv() {
                    Ok(status) => status,
                    Err(_e) => {return;},
                }
            },
            None => return,
        };
        self.configs[status.index as usize].1 = status.status;
    }

    fn status(&mut self, ui: &mut egui::Ui) {

        
        //update vor status
        self.status_refresh();

        ui.group(|ui| {
            // Router Statuses
            ui.label("VOR Status");
            ui.separator();
            if self.running {
                ui.horizontal(|ui| {
                    ui.group(|ui| {
                        ui.label("Status: ");ui.label(RichText::new("Routing").color(Color32::GREEN));
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    ui.group(|ui| {
                        ui.label("Status: ");ui.label(RichText::new("Stopped").color(Color32::RED));
                    });
                });
            }
        });

        ui.group(|ui| {
            // App Statuses
            ui.label("VOR Apps");
            ui.separator();
            if self.configs.len() > 0 {
                for i in 0..self.configs.len() {
                    let mut status_color = Color32::GREEN;
                    match self.configs[i].1 {
                        VORAppStatus::Running => {},
                        VORAppStatus::Stopped => {status_color = Color32::RED},
                        VORAppStatus::AppError(_) => {status_color = Color32::GOLD}
                    }
                    ui.horizontal(|ui| {
                        ui.group(|ui| {
                            ui.label(format!("{}: ", self.configs[i].0.config_data.app_name));
                            ui.add(Label::new(RichText::new(format!("{}", self.configs[i].1)).color(status_color)).wrap(true));
                        });
                    });
                }
            }
        });
    }

    fn list_vor_config(&mut self, ui: &mut egui::Ui) {
        // UI for VOR config
        ui.label("Bind Host: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.bind_host));
        ui.label("Bind Port: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.bind_port));
        ui.label("VRChat Host: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.vrc_host));
        ui.label("VRChat Port: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.vrc_port));
        ui.label("VOR Buffer Size: ");ui.add(egui::TextEdit::singleline(&mut self.vor_router_config.vor_buffer_size));


        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(), |ui| {
                if ui.button(RichText::new("Save").color(Color32::GREEN)).clicked() {
                    self.save_vor_config();
                }
            });

        });
    }

    fn router_exec_button(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(), |ui| {
                if self.running {
                    ui.group(|ui| {
                        if ui.button("Stop").clicked() {
                            if self.running {
                                self.stop_router();
                                ctx.request_repaint();
                            }
                        }
                        if ui.button("Reload").clicked() {
                            // Reload app configs and VOR config and restart all threads
                            // Send ShutdownAll
                            // Reload configs
                            // Start router thread
                            if self.running {
                                self.stop_router();
                                let (router_config, app_configs) = config_construct();
                                self.vor_router_config = router_config;
                                self.configs = app_configs;
                                self.start_router();
                            }
                        }
                    });
                } else {
                    ui.group(|ui| {
                        if ui.button(RichText::new("Start").color(Color32::GREEN)).clicked() {
                            if !self.running {
                                self.start_router();
                                ctx.request_repaint();
                            }
                        }
                    });
                }
            });
            ui.separator();
        });
    }

    fn start_router(&mut self) {
        // Create main router thread - 1 channel store TX in GUI object
            // router thread recv msgs from GUI thread and controls child threads each with their own channel to comm with router thread
        // Generate / Start OSC threads here
        let confs: Vec<VORConfig> = self.configs.iter().map(|c| c.0.config_data.clone()).collect();

        let (router_tx, router_rx): (Sender<RouterMsg>, Receiver<RouterMsg>) = mpsc::channel();
        let (app_stat_tx, app_stat_rx): (Sender<VORAppIdentifier>, Receiver<VORAppIdentifier>) = mpsc::channel();
        self.router_channel = Some(router_tx);
        self.router_msg_recvr = Some(app_stat_rx);

        let bind_target = format!("{}:{}", self.vor_router_config.bind_host, self.vor_router_config.bind_port);
        let vor_buf_size = match self.vor_router_config.vor_buffer_size.parse::<usize>() {
            Ok(s) => s,
            Err(_) => {
                self.router_channel = None;
                self.router_msg_recvr = None;
                return;
            }
        };
        thread::spawn(move || {
            route_main(bind_target, router_rx, app_stat_tx, confs, vor_buf_size);
        });

        self.running = true;
    }

    fn stop_router(&mut self) {
        // Send shutdown signal to OSC threads here
        self.router_channel.take().unwrap().send(RouterMsg::ShutdownAll).unwrap();
        //self.router_msg_recvr = None;
        self.running = false;
    }

    fn save_vor_config(&mut self) {
        fs::write(format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VORConfig.json", get_user_home_dir()), serde_json::to_string(&self.vor_router_config).unwrap()).unwrap();
    }

    fn save_app_config(&mut self, app_index: usize) {
        let _ = fs::remove_file(&self.configs[app_index].0.config_path);
        self.configs[app_index].0.config_path = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VorAppConfigs\\{}.json", get_user_home_dir(), self.configs[app_index].0.config_data.app_name);
        fs::write(&self.configs[app_index].0.config_path, serde_json::to_string(&self.configs[app_index].0.config_data).unwrap()).unwrap();
    }

    fn add_app(&mut self, ui: &mut egui::Ui) {
        // Get inputs
        // Push object into configs
        // 
        ui.group(|ui| {
            if self.adding_new_app {
                ui.label("App Name");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_name));
                ui.label("App Host");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_host));
                ui.label("App Port");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.app_port));
                ui.label("Bind Host");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.bind_host));
                ui.label("Bind Port");ui.add(egui::TextEdit::singleline(&mut self.new_app.as_mut().unwrap().config_data.bind_port));

                ui.horizontal(|ui| {
                    if self.new_app_cf_exists_err {
                        ui.colored_label(Color32::RED, "App config name already being used.. Choose different app name.");
                        ui.separator();
                    }
                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(), |ui| {
                            if ui.button(RichText::new("Cancel").color(Color32::RED)).clicked() {
                                self.new_app = None;
                                self.adding_new_app = false;
                                self.new_app_cf_exists_err = false;
                            }
                            if ui.button(RichText::new("Add").color(Color32::GREEN)).clicked() {
                                self.new_app.as_mut().unwrap().config_path = format!("{}\\AppData\\LocalLow\\VRChat\\VRChat\\OSC\\VOR\\VORAppConfigs\\{}.json", get_user_home_dir(), self.new_app.as_ref().unwrap().config_data.app_name);
                                if !file_exists(&self.new_app.as_ref().unwrap().config_path) {
                                    self.configs.push((self.new_app.take().unwrap(), VORAppStatus::Stopped, false));
                                    self.save_app_config(self.configs.len()-1);
                                    self.adding_new_app = false;
                                    self.new_app_cf_exists_err = false;
                                } else {
                                    self.new_app_cf_exists_err = true;
                                }
                                
                            }
                        });
                    });

                });


            } else {
                ui.horizontal(|ui| {
                    ui.label("Add new VOR app");
                    ui.with_layout(Layout::right_to_left(), |ui| {
                        if ui.button(RichText::new("New").color(Color32::GREEN)).clicked() {
                            self.new_app = Some(VORConfigWrapper {
                                config_path: String::new(),
                                config_data: VORConfig {
                                    app_port: "9100".to_string(),
                                    app_host: "127.0.0.1".to_string(),
                                    bind_port: "9101".to_string(),
                                    bind_host: "127.0.0.1".to_string(),
                                    app_name: "New App".to_string(),
                                },
                            });// new_app defaults
                            self.adding_new_app = true;// Being added
                        }// New button
                    });
    
                });
            }

        });
    }

    fn gui_header(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("VOR");
            ui.add_space(3.);
        });
        ui.separator();
    }

    fn gui_footer(&mut self, ctx: &Context) {
        TopBottomPanel::bottom("footer").show(ctx, |ui|{
            ui.vertical_centered(|ui| {
                ui.add_space(5.0);
                ui.add(Hyperlink::from_label_and_url("VOR","https://github.com/SutekhVRC/VOR"));
                ui.add(Hyperlink::from_label_and_url(RichText::new("Made by Sutekh").monospace().color(Color32::WHITE),"https://github.com/SutekhVRC"));
                ui.add_space(5.0);
            });
        });
    }

    fn list_app_configs(&mut self, ui: &mut egui::Ui) {
        
        for i in 0..self.configs.len() {
        
            ui.group(|ui| {
                if self.configs[i].2 {
                    ui.label("App Name");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_name));
                    ui.label("App Host");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_host));
                    ui.label("App Port");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.app_port));
                    ui.label("Bind Host");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.bind_host));
                    ui.label("Bind Port");ui.add(egui::TextEdit::singleline(&mut self.configs[i].0.config_data.bind_port));

                    ui.horizontal(|ui| {
                        ui.with_layout(Layout::right_to_left(), |ui| {
                            ui.group(|ui| {
                                if ui.button(RichText::new("Save").color(Color32::GREEN)).clicked() {
                                    // Save config / uncollapse maybe
                                    self.save_app_config(i);
                                    self.configs[i].2 = false;// Being edited
                                }
                            });
                        });

                    });


                } else {
                    ui.horizontal(|ui| {
                        ui.label(self.configs[i].0.config_data.app_name.as_str());

                        ui.with_layout(Layout::right_to_left(), |ui| {
                            if !self.running {
                                if ui.button(RichText::new("Delete").color(Color32::RED)).clicked() {
                                    fs::remove_file(&self.configs[i].0.config_path).unwrap();
                                    self.configs.remove(i);
                                }
                                if ui.button(RichText::new("Edit").color(Color32::GOLD)).clicked() {
                                    self.configs[i].2 = true;// Being edited
                                }
                            } else {
                                ui.colored_label(Color32::RED, "Locked");
                            }
                        });
                    });
                }
            });
        }// For list
    }
}// impl VORGUI

impl App for VORGUI {
    fn setup(&mut self, _ctx: &egui::Context, _frame: &eframe::epi::Frame, _storage: Option<&dyn eframe::epi::Storage>) {
        // Read config values

        // Set fonts etc.
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &eframe::epi::Frame) {
        self.set_tab(&ctx);
        CentralPanel::default().show(ctx, |ui| {
            //ctx.request_repaint();

            self.gui_header(ui);

            match self.tab {
                VORGUITab::Main => {
                    ui.add(egui::Label::new("VOR Main"));
                    ui.separator();
                    self.status(ui);
                    self.router_exec_button(ui, &ctx);
                },
                VORGUITab::Apps => {
                    ui.add(egui::Label::new("VOR App Configs"));
                    ui.separator();
                    ScrollArea::new([false, true]).show(ui, |ui| {
                        self.list_app_configs(ui);
                        self.add_app(ui);
                        ui.add_space(40.);
                    });
                },
                VORGUITab::Firewall => {
                    ui.add(egui::Label::new("OSC Firewall"));
                    ui.separator();
                },
                VORGUITab::Config => {
                    ui.add(egui::Label::new("VOR Config"));
                    ui.separator();
                    self.list_vor_config(ui);
                },
            }
        });
        self.gui_footer(&ctx);
    }

    fn name(&self) -> &str {
        "VRChat OSC Router"
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
        fs::create_dir(&vor_root_dir).expect("[-] Cannot create VOR root directory.");
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
    //println!("{:?}", configs);
    (router_config, configs)
}

fn config_construct() -> (RouterConfig, Vec<(VORConfigWrapper, VORAppStatus, bool)>) {
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
        gconfs.push((c, VORAppStatus::Stopped, false));
    }
    return (vor_router_config, gconfs);
}

fn main() {

    let (vor_router_config, configs) = config_construct();

    let mut native_opts = NativeOptions::default();
    native_opts.initial_window_size = Some(Vec2::new(325., 400.));
    native_opts.max_window_size = Some(Vec2::new(325., 400.));
    native_opts.min_window_size = Some(Vec2::new(325., 400.));

    run_native(
        Box::new(
            VORGUI {
                configs,
                running: false,
                tab: VORGUITab::Main,
                router_channel: None,
                vor_router_config,
                adding_new_app: false,
                new_app: None,
                new_app_cf_exists_err: false,
                router_msg_recvr: None,
    }), native_opts);

    /*
        Load configs
        Start GUI
        GUI creates threads for app routes and 
    */
}

fn route_main(router_bind_target: String, router_rx: Receiver<RouterMsg>, app_stat_tx: Sender<VORAppIdentifier>, configs: Vec<VORConfig>, vor_queue_size: usize) {

    let vrc_sock = match UdpSocket::bind(router_bind_target) {
        Ok(s) => s,
        Err(_e) => {
            let _ = app_stat_tx.send(app_error(-1, -1, "Failed to bind VOR socket.".to_string()));
            return;
        }
    };
    vrc_sock.set_nonblocking(true).unwrap();

    //let mut app_channel_vector: Vec<Sender<Vec<u8>>> = Vec::new();
    let mut artc = Vec::new();
    let mut indexer = 0;

    let async_rt = Runtime::new().unwrap();
    let (bcst_tx, _bcst_rx) = broadcast::channel(vor_queue_size);

    for app in configs {

        let (router_tx, router_rx) = mpsc::channel();
        artc.push(router_tx);

        //let (app_r_tx, app_r_rx) = mpsc::channel();
        //app_channel_vector.push(app_r_tx);

        let app_stat_tx_at = app_stat_tx.clone();
        let bcst_app_rx = bcst_tx.subscribe();
        async_rt.spawn(route_app(bcst_app_rx, router_rx, app_stat_tx_at, indexer, app));
        indexer += 1;
    }
    drop(_bcst_rx);// Dont need this rx

    /*// Using Asynchronous queue for msging no need to wait
    println!("[*] Wait 3 seconds for listener channels..");
    thread::sleep(Duration::from_secs(3));*/

    let (osc_parse_tx, osc_parse_rx): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    thread::spawn(move || {parse_vrc_osc(bcst_tx, osc_parse_rx, vrc_sock);});
    println!("[+] Started VRChat OSC Router.");

    // Listen for GUI events
    loop {
        match router_rx.recv().unwrap() {
            RouterMsg::ShutdownAll => {
                // Send shutdown to all threads

                // Shutdown osc parse thread first
                osc_parse_tx.send(true).unwrap();
                println!("[*] Shutdown signal: OSC receive thread");

                // Shutdown app route threads second
                for app_route_thread_channel in artc {
                    let _ = app_route_thread_channel.send(true);
                }
                println!("[*] Shutdown signal: Route threads");

                // Shutdown router thread last
                println!("[*] Shutdown signal: Router thread");
                return;// Shutdown router thread.
            },
            //_ =>{},
        }
    }
}

fn parse_vrc_osc(bcst_tx: bcst_Sender<Vec<u8>>, router_rx: Receiver<bool>, vrc_sock: UdpSocket) {
    let mut buf = [0u8; MTU];
    vrc_sock.set_nonblocking(true).unwrap();
    loop {

        match vrc_sock.recv_from(&mut buf) {
            Ok((br, _a)) => {

                if br <= 0 {// If got bytes send them to routers otherwise restart loop
                    continue;
                } else {

                    bcst_tx.send(buf.to_vec()).unwrap();

                    match router_rx.try_recv() {
                        Ok(sig) => {
                            if sig {
                                println!("[!] VRC OSC thread shutdown");
                                return;
                            }
                        },
                        Err(_) => {},
                    }
                }
            },
            Err(_e) => {
                match router_rx.try_recv() {
                    Ok(sig) => {
                        if sig {
                            println!("[!] VRC OSC thread shutdown");
                            return;
                        }
                    },
                    Err(_) => {},
                }
            },
        }// vrc recv sock
    }// loop
}

async fn route_app(mut rx: bcst_Receiver<Vec<u8>>, router_rx: Receiver<bool>, app_stat_tx_at: Sender<VORAppIdentifier>, ai: i64, app: VORConfig) {
    let rhp = format!("{}:{}", app.app_host, app.app_port);
    let lhp = format!("{}:{}", app.bind_host, app.bind_port);
    let sock = match UdpSocket::bind(lhp) {
        Ok(s) => s,
        Err(_e) => {
            let _ = app_stat_tx_at.send(app_error(ai, -2, format!("Failed to bind app UdpSocket: {}", _e)));
            return;// Close app route thread because app failed to bind
        }
    };
    println!("[*] OSC App: [{}] Route Initialized..", app.app_name);
    let _ = app_stat_tx_at.send(VORAppIdentifier { index: ai, status: VORAppStatus::Running });
    loop {
        match router_rx.try_recv() {
            Ok(signal) => {
                //println!("[!] signal: {}", signal);
                if signal {
                    let _ = app_stat_tx_at.send(VORAppIdentifier { index: ai, status: VORAppStatus::Stopped });
                    println!("[!] Send Stopped status");
                    return;
                }
            },
            _ => {/*println!("[!] Try recv errors")*/},
        }

        // Get vrc OSC buffer
        let buffer = match rx.recv().await {
            Ok(b) => b,
            Err(RecvError::Lagged(_e)) => continue,
            Err(RecvError::Closed) => {
                //println!("[OSC BUFFER RECV FAIL");
                // VRC OSC BUFFER CHANNEL DIED SO KILL ROUTE THREAD
                let _ = app_stat_tx_at.send(VORAppIdentifier { index: ai, status: VORAppStatus::Stopped });
                return
            }
        };

        // Route buffer
        match sock.send_to(&buffer, &rhp) {
            Ok(_bs) => {},
            Err(_e) => {
                let _ = app_stat_tx_at.send(app_error(ai, -3, format!("Failed to send VRC OSC buffer to app: {}", _e)));
            }
        }
    }
}

fn app_error(ai: i64, err_id: i32, msg: String) -> VORAppIdentifier {
    VORAppIdentifier {
        index: ai,
        status: VORAppStatus::AppError(VORAppError{id: err_id, msg}),
    }
}